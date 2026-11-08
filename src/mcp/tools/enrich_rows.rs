//! Chat-only tool: `enrich_rows` - batch-fetch the URL each row points to and
//! write what came back into a column of the live GUI tab.
//!
//! This is the semi-automatic half of "compare rows against the live web":
//! each result lands as one pending edit op on the tab, so every enriched cell
//! shows up in the edit audit trail for the user to review, undo or accept -
//! nothing is silently written to disk. No MCP `handle` - this needs the GUI
//! write-back channel.

use serde::Deserialize;
use serde_json::{Map, Value};

use octa::data::CellValue;
use octa::data::links::detect_url;

use super::edit_open_tab::ColRef;
use super::{PendingTabEdit, ResolvedOp, ToolContext, web_fetch};

pub const DESCRIPTION: &str = "Enrich the LIVE open tab from the web: for every row, fetch the URL \
in `url_column` (the product/listing page a cell links to) and write the extracted value into \
`target_column` - created when missing, overwritten otherwise. `extract`: \"status\" = HTTP status \
code, \"text\" = page text (HTML converted, capped at `cell_chars`, default 500), \"pattern\" = \
first capture group of the first `pattern` regex match. `limit` caps rows processed (default 50, \
max 500); rows are taken in table order and rows without a URL are skipped. Every written cell \
lands in the edit audit trail for review - the user still saves to persist. Only works in the \
in-GUI assistant with Write protection off.";

/// Upper bounds so a runaway table cannot turn into an all-night crawler.
const HARD_LIMIT: usize = 500;
const DEFAULT_LIMIT: usize = 50;
const DEFAULT_CELL_CHARS: usize = 500;
/// Concurrent page fetches. Enough overlap that one slow page stops costing
/// a whole batch; low enough to stay polite to one host.
const FETCH_WORKERS: usize = 8;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Which open tab to enrich: a handle like `#2`, `@active`, or the tab name.
    pub open_tab: String,
    /// Column holding the URL to fetch per row (index or name). Cells without
    /// an http(s) URL are skipped.
    pub url_column: ColRef,
    /// Column the extracted value is written into (created when missing).
    pub target_column: String,
    /// What to pull from each page: "status", "text", or "pattern".
    #[serde(default)]
    pub extract: Extract,
    /// Regex with at least one capture group; the first group's first match is
    /// stored. Required when `extract` is "pattern".
    #[serde(default)]
    pub pattern: Option<String>,
    /// Max rows processed, in table order. Default 50, hard cap 500.
    #[serde(default)]
    pub limit: Option<usize>,
    /// Per-cell character cap for `extract: "text"`. Default 500.
    #[serde(default)]
    pub cell_chars: Option<usize>,
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
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let queue = ctx.pending_tab_edits.as_ref().ok_or_else(|| {
        anyhow::anyhow!("enriching the live tab is only available in the in-GUI assistant")
    })?;
    if !ctx.allow_existing_writes {
        anyhow::bail!(
            "Modifying open data is turned off (Write protection). Turn it off in Settings to let \
             me write enriched values into the open tab."
        );
    }
    let regex = if matches!(p.extract, Extract::Pattern) {
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
    let target_idx = table.columns.iter().position(|c| c.name == p.target_column);

    // Collect the rows worth fetching first: the scan stays cheap and
    // synchronous, the downloads fan out afterwards.
    let mut targets: Vec<(usize, String)> = Vec::new();
    let mut scanned = 0usize;
    for row in 0..n {
        if targets.len() >= limit {
            break;
        }
        scanned = row + 1;
        let text = table
            .get(row, url_col)
            .map(|c| c.to_string())
            .unwrap_or_default();
        if let Some(url) = detect_url(&text) {
            targets.push((row, url.to_string()));
        }
    }

    // Bounded-parallel fetch: a handful of worker threads share an index
    // into `targets`, results land by target position so row order is
    // preserved regardless of completion order. Serial used to mean one
    // slow page stalled every row behind it.
    let results: Vec<std::sync::Mutex<Option<Result<(u16, String, String), String>>>> = (0
        ..targets.len())
        .map(|_| std::sync::Mutex::new(None))
        .collect();
    {
        let next = std::sync::atomic::AtomicUsize::new(0);
        let workers = FETCH_WORKERS.min(targets.len());
        std::thread::scope(|scope| {
            for _ in 0..workers {
                scope.spawn(|| {
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        if i >= targets.len() {
                            return;
                        }
                        let out = web_fetch::fetch(&targets[i].1)
                            .map(|(_, status, ct, body)| (status, ct, body))
                            .map_err(|e| e.to_string());
                        *results[i].lock().unwrap() = Some(out);
                    }
                });
            }
        });
    }

    let mut values: Vec<CellValue> = Vec::new();
    let mut failures: Vec<Value> = Vec::new();
    let mut fetched = 0usize;

    for (i, slot) in results.iter().enumerate() {
        let row = targets[i].0;
        match slot.lock().unwrap().take() {
            Some(Ok((status, content_type, body))) => {
                fetched += 1;
                let value = match p.extract {
                    Extract::Status => status.to_string(),
                    Extract::Text => {
                        let text = if content_type.contains("html") {
                            web_fetch::html_to_text(&body)
                        } else {
                            body
                        };
                        text.chars().take(cell_chars).collect::<String>()
                    }
                    Extract::Pattern => {
                        let re = regex.as_ref().expect("compiled above");
                        let text = if content_type.contains("html") {
                            web_fetch::html_to_text(&body)
                        } else {
                            body
                        };
                        re.captures(&text)
                            .and_then(|c| c.get(1))
                            .map(|m| m.as_str().to_string())
                            .unwrap_or_default()
                    }
                };
                values.push(CellValue::String(value));
            }
            Some(Err(e)) => {
                failures.push(serde_json::json!({"row": row, "error": e}));
            }
            None => {}
        }
    }

    // Empty rows are intentionally left out of `values` only when the target
    // column already exists (SetCells). For a new column the value vec must
    // have one slot per row, so skipped/failed rows land as NULL.
    let mut ops = Vec::new();
    let written = values.len();
    let mut cells_set = 0usize;
    match target_idx {
        Some(col) => {
            // Re-walk the same row set to pair values with their row indices:
            // `values` is positional over *fetched* rows, not over `0..scanned`.
            let mut vi = 0usize;
            let mut cells = Vec::with_capacity(written);
            for row in 0..scanned {
                let text = table
                    .get(row, url_col)
                    .map(|c| c.to_string())
                    .unwrap_or_default();
                if detect_url(&text).is_none() {
                    continue;
                }
                if failures
                    .iter()
                    .any(|f| f["row"].as_u64() == Some(row as u64))
                {
                    continue;
                }
                if vi < values.len() {
                    cells.push((row, col, values[vi].clone()));
                    vi += 1;
                }
            }
            cells_set = cells.len();
            ops.push(ResolvedOp::SetCells(cells));
        }
        None => {
            // Fill unfetched positions with NULL so the column has n slots.
            let mut col_values = vec![CellValue::Null; scanned.min(n)];
            let mut vi = 0usize;
            for row in 0..scanned {
                let text = table
                    .get(row, url_col)
                    .map(|c| c.to_string())
                    .unwrap_or_default();
                if detect_url(&text).is_none()
                    || failures
                        .iter()
                        .any(|f| f["row"].as_u64() == Some(row as u64))
                {
                    continue;
                }
                if vi < values.len() {
                    col_values[row] = values[vi].clone();
                    vi += 1;
                }
            }
            ops.push(ResolvedOp::AddColumn {
                name: p.target_column.clone(),
                type_name: "VARCHAR".to_string(),
                values: col_values,
            });
        }
    }

    queue.lock().unwrap().push(PendingTabEdit {
        tab_handle: snap.handle.clone(),
        snapshot_rows: n,
        ops,
    });

    let mut out = Map::new();
    out.insert("applied_to".to_string(), Value::String(snap.handle.clone()));
    out.insert("rows_scanned".to_string(), Value::from(scanned));
    out.insert("pages_fetched".to_string(), Value::from(fetched));
    out.insert("values_written".to_string(), Value::from(written));
    out.insert(
        "target_column".to_string(),
        Value::String(p.target_column.clone()),
    );
    out.insert(
        "column_created".to_string(),
        Value::from(target_idx.is_none()),
    );
    out.insert("cells_set".to_string(), Value::from(cells_set));
    out.insert("failures".to_string(), Value::Array(failures));
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
