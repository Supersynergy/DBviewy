//! Chat-only tool: `enrich_rows` - batch-fetch the URL each row points to and
//! write what came back into a column of the live GUI tab.
//!
//! This is the semi-automatic half of "compare rows against the live web":
//! each result lands as one pending edit op on the tab, so every enriched cell
//! shows up in the edit audit trail for the user to review, undo or accept -
//! nothing is silently written to disk. No MCP `handle` - this needs the GUI
//! write-back channel.
//!
//! `extract` decides what a row's cell means: a URL to fetch ("status",
//! "text", "pattern"), or a search query whose top hit is fetched instead
//! ("search"). With `provenance` on, two sibling columns record where each
//! value came from (`<target>_src`) and when (`<target>_at`).

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use serde::Deserialize;
use serde_json::{Map, Value};

use octa::data::CellValue;
use octa::data::links::detect_url;

use super::edit_open_tab::ColRef;
use super::{PendingTabEdit, ResolvedOp, ToolContext, web_fetch, web_search};

pub const DESCRIPTION: &str = "Enrich the LIVE open tab from the web: for every row, fetch the page \
in `url_column` and write the extracted value into `target_column` - created when missing, \
overwritten otherwise. `extract`: \"status\" = HTTP status code, \"text\" = page text (HTML \
converted, capped at `cell_chars`, default 500), \"pattern\" = first capture group of the first \
`pattern` regex match, \"search\" = treat the cell as a SEARCH QUERY, fetch the top result's page \
and apply `pattern`/`text`/`status` extraction to it (set `pattern` for a capture group, else page \
text). `top_k` (default 1, max 5) tries that many hits: with a `pattern` the first page yielding \
a capture wins, without one the top hit is used. `provenance` also writes `<target>_src` (the URL \
the value actually came from) and `<target>_at` (fetch time) - keep it on for price comparisons. \
`refresh` ignores the page cache (cached pages count as fresh for 6h). `limit` caps rows \
processed (default 50, max 500); rows are taken in the user's view order (sorted/filtered - \
filtered-out rows are skipped; storage order only when no view exists). Every written cell lands in the \
edit audit trail for review - the user still saves to persist. Only works in the in-GUI assistant \
with Write protection off.";

/// Upper bounds so a runaway table cannot turn into an all-night crawler.
const HARD_LIMIT: usize = 500;
const DEFAULT_LIMIT: usize = 50;
const DEFAULT_CELL_CHARS: usize = 500;
/// Concurrent page fetches. Enough overlap that one slow page stops costing
/// a whole batch; low enough to stay polite to one host.
const FETCH_WORKERS: usize = 8;
/// Concurrent requests against ONE host. A mixed-source batch still fans out
/// to eight; a single-domain one (the common case) politely holds four.
const PER_HOST_LIMIT: usize = 4;
/// Queries fed to the search engine get trimmed to this - a paste of a whole
/// description is a worse query than its first line.
const MAX_QUERY_CHARS: usize = 200;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Which open tab to enrich: a handle like `#2`, `@active`, or the tab name.
    pub open_tab: String,
    /// Column holding the URL to fetch - or the search query, for
    /// `extract: "search"` - per row (index or name). Empty cells are skipped.
    pub url_column: ColRef,
    /// Column the extracted value is written into (created when missing).
    pub target_column: String,
    /// What to pull from each page: "status", "text", "pattern", or "search".
    #[serde(default)]
    pub extract: Extract,
    /// Regex with at least one capture group; the first group's first match is
    /// stored. Required when `extract` is "pattern"; optional for "search".
    #[serde(default)]
    pub pattern: Option<String>,
    /// Max rows processed, in the tab's view order (sorted/filtered when the
    /// GUI carries one, else storage order). Default 50, hard cap 500.
    #[serde(default)]
    pub limit: Option<usize>,
    /// Per-cell character cap for `extract: "text"`. Default 500.
    #[serde(default)]
    pub cell_chars: Option<usize>,
    /// Also write `<target>_src` and `<target>_at` columns recording where
    /// each value came from and when it was fetched. Default false.
    #[serde(default)]
    pub provenance: bool,
    /// Ignore cached pages and fetch live. Default false.
    #[serde(default)]
    pub refresh: bool,
    /// For `extract: "search"`: how many hits to try. With a `pattern`, the
    /// first hit whose page yields a non-empty capture wins; without one the
    /// top hit is used. Default 1, hard cap 5.
    #[serde(default)]
    pub top_k: Option<usize>,
}

#[derive(Debug, Default, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Extract {
    /// The HTTP status code ("200") - "does the listing still exist".
    #[default]
    Status,
    /// Readable page text (HTML converted), capped per cell.
    Text,
    /// First capture group of the first `pattern` match in the page text.
    Pattern,
    /// The cell is a search query: fetch the top hit's page and extract -
    /// `pattern` when given, page text otherwise.
    Search,
}

/// Side-channel controls for a run the GUI owns: `cancel` stops dispatching
/// new pages (fetches in flight finish), `progress` counts processed rows so
/// the status bar can show "12/340". The plain `run` wrapper passes inert
/// defaults, so the chat path and tests are unchanged.
#[derive(Debug, Default)]
pub struct RunCtl {
    pub cancel: Arc<AtomicBool>,
    pub progress: Arc<AtomicUsize>,
    /// Set by the run once the target list is known, so a progress readout
    /// can show "12/340" instead of a bare spinner.
    pub total: Arc<AtomicUsize>,
}

/// What one page gave back, kept separate from the extracted value so
/// provenance can travel with it.
struct PageResult {
    final_url: String,
    status: u16,
    content_type: String,
    body: String,
    /// Epoch seconds when the page was actually fetched: the cache entry's
    /// age for a hit, `None` for a live fetch (stamped "now" by the caller).
    fetched_at: Option<i64>,
}

/// Why one target produced nothing. `Http` carries the page's status so the
/// worker can back off a rate-limiting host; `Other` is everything else.
enum ResolveFail {
    Http(u16),
    Other(String),
}

impl std::fmt::Display for ResolveFail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolveFail::Http(s) => write!(f, "HTTP {s}"),
            ResolveFail::Other(m) => write!(f, "{m}"),
        }
    }
}

/// Statuses worth a cooldown: the host is shedding load, so piling more
/// rows on it just turns the batch into a wall of failures.
fn is_backoff_status(s: u16) -> bool {
    matches!(s, 408 | 429 | 502 | 503 | 504)
}

/// Search hits tried per row: default 1, hard cap 5. More than five results
/// per row means the query, not the ranking, is the problem.
fn top_k_of(p: &Params) -> usize {
    p.top_k.unwrap_or(1).clamp(1, 5)
}

/// Hostname of a URL for the per-host semaphore. `http(s)://` only - anything
/// else never reaches the worker pool anyway.
fn host_of(url: &str) -> String {
    url.split("://")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .unwrap_or_default()
        .to_lowercase()
}

/// Order search candidates by likely usefulness instead of blind ranking:
/// hits whose title/snippet shares words with the query are tried first,
/// so a pattern usually captures on the first fetch instead of the third.
/// Pure re-ordering - ties keep the engine's own order.
fn rank_hits(query: &str, hits: &mut [web_search::Hit]) {
    let qwords: std::collections::HashSet<String> = query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= 3)
        .map(|w| w.to_lowercase())
        .collect();
    if qwords.is_empty() {
        return;
    }
    let score = |h: &web_search::Hit| -> usize {
        let text = format!("{} {}", h.title, h.snippet).to_lowercase();
        qwords.iter().filter(|w| text.contains(w.as_str())).count()
    };
    hits.sort_by_key(|h| std::cmp::Reverse(score(h)));
}

/// Resolve one target: fetch `input` directly, or when `is_search` treat it
/// as a query and fetch a result page. With a `pattern` the top hits are
/// tried in order and the first whose page yields a capture wins - the #1
/// result is not always the page holding the price. Without a pattern the
/// top hit answers. When `for_content` is set, a page answering 4xx/5xx is
/// not a source: the run moves on to the next candidate (a "this page is
/// gone" body is not a price). Errors carry a short reason.
fn resolve_target(
    input: &str,
    is_search: bool,
    for_content: bool,
    refresh: bool,
    regex: Option<&regex::Regex>,
    top_k: usize,
) -> Result<PageResult, ResolveFail> {
    let urls: Vec<String> = if is_search {
        let mut hits =
            web_search::search(input, top_k).map_err(|e| ResolveFail::Other(e.to_string()))?;
        if hits.is_empty() {
            return Err(ResolveFail::Other("no search result".to_string()));
        }
        rank_hits(input, &mut hits);
        hits.into_iter().map(|h| h.url).collect()
    } else {
        vec![input.to_string()]
    };
    let mut last_err = ResolveFail::Other("no fetchable search result".to_string());
    let mut fallback: Option<PageResult> = None;
    for url in urls {
        match web_fetch::fetch_cached_stamped(&url, refresh) {
            Ok((final_url, status, content_type, body, fetched_at)) => {
                let page = PageResult {
                    final_url,
                    status,
                    content_type,
                    body,
                    fetched_at,
                };
                if for_content && page.status >= 400 {
                    last_err = ResolveFail::Http(page.status);
                    continue;
                }
                // With a pattern, a page only wins when it actually yields
                // a capture - a top hit without the price is worse than the
                // next result that has one.
                match regex {
                    Some(re) => {
                        let text = if page.content_type.contains("html") {
                            web_fetch::html_to_text(&page.body)
                        } else {
                            page.body.clone()
                        };
                        if re.captures(&text).is_some() {
                            return Ok(page);
                        }
                        fallback = fallback.or(Some(page));
                    }
                    None => return Ok(page),
                }
            }
            Err(e) => last_err = ResolveFail::Other(e.to_string()),
        }
    }
    fallback.ok_or(last_err)
}

fn extract_value(
    extract: Extract,
    regex: Option<&regex::Regex>,
    page: &PageResult,
    cell_chars: usize,
) -> String {
    match extract {
        Extract::Status => page.status.to_string(),
        Extract::Text | Extract::Pattern | Extract::Search => {
            let text = if page.content_type.contains("html") {
                web_fetch::html_to_text(&page.body)
            } else {
                page.body.clone()
            };
            if let Some(re) = regex {
                re.captures(&text)
                    .and_then(|c| c.get(1))
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default()
            } else {
                text.chars().take(cell_chars).collect::<String>()
            }
        }
    }
}

/// Emit the column op for one output column: SetCells onto an existing
/// column, or AddColumn with NULLs where no row produced a value. Returns
/// the number of real (non-NULL) values emitted.
fn push_column_op(
    ops: &mut Vec<ResolvedOp>,
    table: &octa::data::DataTable,
    name: &str,
    pairs: &BTreeMap<usize, CellValue>,
) -> usize {
    if pairs.is_empty() {
        return 0;
    }
    let written = pairs.len();
    match table.columns.iter().position(|c| c.name == name) {
        Some(col) => ops.push(ResolvedOp::SetCells(
            pairs.iter().map(|(&r, v)| (r, col, v.clone())).collect(),
        )),
        None => {
            let mut values = vec![CellValue::Null; table.row_count()];
            for (&r, v) in pairs {
                if r < values.len() {
                    values[r] = v.clone();
                }
            }
            ops.push(ResolvedOp::AddColumn {
                name: name.to_string(),
                type_name: "VARCHAR".to_string(),
                values,
            });
        }
    }
    written
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    run_ctl(ctx, p, &RunCtl::default())
}

pub fn run_ctl(ctx: &ToolContext, p: &Params, ctl: &RunCtl) -> anyhow::Result<Value> {
    let queue = ctx.pending_tab_edits.as_ref().ok_or_else(|| {
        anyhow::anyhow!("enriching the live tab is only available in the in-GUI assistant")
    })?;
    if !ctx.allow_existing_writes {
        anyhow::bail!(
            "Modifying open data is turned off (Write protection). Turn it off in Settings to let \
             me write enriched values into the open tab."
        );
    }
    let needs_regex = matches!(p.extract, Extract::Pattern)
        || (matches!(p.extract, Extract::Search) && p.pattern.is_some());
    let regex = if needs_regex {
        let pat = p.pattern.as_deref().ok_or_else(|| {
            anyhow::anyhow!("extract \"pattern\" needs a `pattern` regex with a capture group")
        })?;
        let re =
            regex::Regex::new(pat).map_err(|e| anyhow::anyhow!("invalid `pattern` regex: {e}"))?;
        if re.captures_len() < 2 {
            anyhow::bail!("`pattern` regex has no capture group - wrap the part to keep in (...)");
        }
        Some(re)
    } else {
        None
    };

    let snap = ctx
        .snapshot_for_open_tab(&p.open_tab)
        .ok_or_else(|| anyhow::anyhow!("no open tab matches \"{}\"", p.open_tab))?;
    if snap.window.is_some() {
        anyhow::bail!(
            "\"{}\" is open in large-file mode, which is read-only - it streams from disk as the \
             user scrolls, so there is nothing in memory to enrich.",
            p.open_tab
        );
    }
    let table = &snap.table;
    let n = table.row_count();
    let url_col = match &p.url_column {
        ColRef::Index(i) => {
            if *i >= table.col_count() {
                anyhow::bail!(
                    "url_column {i} is out of range (the tab has {})",
                    table.col_count()
                );
            }
            *i
        }
        ColRef::Name(name) => table
            .columns
            .iter()
            .position(|c| &c.name == name)
            .ok_or_else(|| anyhow::anyhow!("no column named \"{name}\""))?,
    };
    let limit = p.limit.unwrap_or(DEFAULT_LIMIT).min(HARD_LIMIT);
    let cell_chars = p.cell_chars.unwrap_or(DEFAULT_CELL_CHARS);
    let is_search = matches!(p.extract, Extract::Search);

    // Collect the rows worth processing first: the scan stays cheap and
    // synchronous, the network work fans out afterwards. A URL column is
    // detected per cell; a query column just needs non-empty text. When the
    // snapshot carries the user's view order (sorted/filtered), "the first
    // N rows" means the first N *visible* rows and filtered-out rows are
    // skipped; indices stay storage indices so writes cannot drift.
    let view: Vec<usize> = snap.view_rows.clone().unwrap_or_else(|| (0..n).collect());
    let mut targets: Vec<(usize, String)> = Vec::new();
    let mut scanned = 0usize;
    for row in view {
        if row >= n {
            continue;
        }
        if targets.len() >= limit {
            break;
        }
        scanned = row + 1;
        let text = table
            .get(row, url_col)
            .map(|c| c.to_string())
            .unwrap_or_default();
        let usable = if is_search {
            !text.trim().is_empty()
        } else {
            detect_url(&text).is_some()
        };
        if usable {
            let input = if is_search {
                text.chars().take(MAX_QUERY_CHARS).collect()
            } else {
                detect_url(&text).unwrap_or("").to_string()
            };
            targets.push((row, input));
        }
    }
    ctl.total.store(targets.len(), Ordering::Relaxed);

    // Bounded-parallel work: worker threads share an index into `targets`,
    // results land by target position so row order is preserved regardless
    // of completion order. A per-host semaphore keeps a single-domain batch
    // polite, a per-host cooldown pauses dispatch onto a host that answered
    // 429/5xx instead of burning the remaining rows into failures, `cancel`
    // stops dispatch (in-flight pages finish), `progress` counts processed
    // rows for the GUI's status pill.
    let results: Vec<Mutex<Option<Result<PageResult, ResolveFail>>>> =
        (0..targets.len()).map(|_| Mutex::new(None)).collect();
    {
        let next = AtomicUsize::new(0);
        let host_slots = Mutex::new(std::collections::HashMap::<String, usize>::new());
        let host_blocked =
            Mutex::new(std::collections::HashMap::<String, std::time::Instant>::new());
        let host_free = Condvar::new();
        let for_content = !matches!(p.extract, Extract::Status);
        let workers = FETCH_WORKERS.min(targets.len());
        std::thread::scope(|scope| {
            for _ in 0..workers {
                scope.spawn(|| {
                    loop {
                        if ctl.cancel.load(Ordering::Relaxed) {
                            return;
                        }
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        if i >= targets.len() {
                            return;
                        }
                        let host = host_of(&targets[i].1);
                        // Hold at most PER_HOST_LIMIT slots per host, and
                        // none while the host is in cooldown. The cancel flag
                        // is re-checked while waiting so a held task can exit.
                        {
                            let mut slots = host_slots.lock().unwrap();
                            loop {
                                if ctl.cancel.load(Ordering::Relaxed) {
                                    return;
                                }
                                let blocked = host_blocked
                                    .lock()
                                    .unwrap()
                                    .get(&host)
                                    .map(|t| t.saturating_duration_since(std::time::Instant::now()))
                                    .unwrap_or_default();
                                if blocked.is_zero()
                                    && slots.get(&host).copied().unwrap_or(0) < PER_HOST_LIMIT
                                {
                                    break;
                                }
                                let (guard, _timeout) = host_free
                                    .wait_timeout(
                                        slots,
                                        blocked
                                            .min(std::time::Duration::from_millis(200))
                                            .max(std::time::Duration::from_millis(50)),
                                    )
                                    .unwrap();
                                slots = guard;
                            }
                            *slots.entry(host.clone()).or_default() += 1;
                        }
                        let out = resolve_target(
                            &targets[i].1,
                            is_search,
                            for_content,
                            p.refresh,
                            regex.as_ref(),
                            top_k_of(p),
                        );
                        // A rate-limited or buckling host earns a cooldown:
                        // further rows on it wait instead of failing fast.
                        if let Err(ResolveFail::Http(s)) = &out {
                            if is_backoff_status(*s) {
                                host_blocked.lock().unwrap().insert(
                                    host.clone(),
                                    std::time::Instant::now() + std::time::Duration::from_secs(20),
                                );
                            }
                        }
                        *results[i].lock().unwrap() = Some(out);
                        {
                            let mut slots = host_slots.lock().unwrap();
                            if let Some(c) = slots.get_mut(&host) {
                                *c = c.saturating_sub(1);
                            }
                        }
                        host_free.notify_all();
                        ctl.progress.fetch_add(1, Ordering::Relaxed);
                    }
                });
            }
        });
    }
    if let Some(cache) = super::fetch_cache::global() {
        cache.flush();
    }

    // Pair results back to rows: `results[i]` belongs to `targets[i].0`, so
    // failed, skipped and unfinished (cancelled) rows can never shift values
    // onto the wrong row - the pre-parallel version's positional re-walk had
    // exactly that hazard.
    let mut values: BTreeMap<usize, CellValue> = BTreeMap::new();
    let mut srcs: BTreeMap<usize, CellValue> = BTreeMap::new();
    let mut ats: BTreeMap<usize, CellValue> = BTreeMap::new();
    let mut failures: Vec<Value> = Vec::new();
    let (mut fetched, mut cached, mut cancelled) = (0usize, 0usize, 0usize);
    let run_stamp = chrono::Utc::now().to_rfc3339();

    for (i, slot) in results.iter().enumerate() {
        let row = targets[i].0;
        match slot.lock().unwrap().take() {
            Some(Ok(page)) => {
                fetched += 1;
                // A cache hit carries its entry's fetch time - the _at column
                // must say how old the answer really is, not stamp it "now".
                let fetched_at = page
                    .fetched_at
                    .and_then(|t| chrono::DateTime::from_timestamp(t, 0))
                    .map(|t| t.to_rfc3339())
                    .unwrap_or_else(|| run_stamp.clone());
                if page.fetched_at.is_some() {
                    cached += 1;
                }
                let value = extract_value(
                    if is_search {
                        // Search mode extracts pattern-or-text from the hit's
                        // page; "status" still means the page's HTTP status.
                        if p.pattern.is_some() {
                            Extract::Pattern
                        } else {
                            Extract::Text
                        }
                    } else {
                        p.extract
                    },
                    regex.as_ref(),
                    &page,
                    cell_chars,
                );
                values.insert(row, CellValue::String(value));
                srcs.insert(row, CellValue::String(page.final_url.clone()));
                ats.insert(row, CellValue::String(fetched_at.clone()));
            }
            Some(Err(e)) => failures.push(serde_json::json!({"row": row, "error": e.to_string()})),
            None => cancelled += 1,
        }
    }

    let mut ops = Vec::new();
    let mut cells_set = push_column_op(&mut ops, table, &p.target_column, &values);
    if p.provenance {
        cells_set += push_column_op(&mut ops, table, &format!("{}_src", p.target_column), &srcs);
        cells_set += push_column_op(&mut ops, table, &format!("{}_at", p.target_column), &ats);
    }

    if !ops.is_empty() {
        queue.lock().unwrap().push(PendingTabEdit {
            tab_handle: snap.handle.clone(),
            snapshot_rows: n,
            ops,
        });
    }

    let mut out = Map::new();
    out.insert("applied_to".to_string(), Value::String(snap.handle.clone()));
    out.insert("rows_scanned".to_string(), Value::from(scanned));
    out.insert("pages_fetched".to_string(), Value::from(fetched));
    out.insert("cache_hits".to_string(), Value::from(cached));
    out.insert("values_written".to_string(), Value::from(values.len()));
    out.insert(
        "target_column".to_string(),
        Value::String(p.target_column.clone()),
    );
    out.insert("provenance".to_string(), Value::from(p.provenance));
    out.insert("cells_set".to_string(), Value::from(cells_set));
    out.insert("failures".to_string(), Value::Array(failures));
    if cancelled > 0 {
        out.insert("cancelled_remaining".to_string(), Value::from(cancelled));
    }
    out.insert(
        "note".to_string(),
        Value::String(
            "Queued as pending edits on the live tab - visible in the edit audit trail. \
             The user must save to persist."
                .to_string(),
        ),
    );
    Ok(Value::Object(out))
}

#[cfg(test)]
#[path = "enrich_rows_tests.rs"]
mod tests;
