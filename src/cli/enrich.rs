//! `octa --enrich FILE --enrich-on COL [flags]`
//!
//! Batch-fetch a URL column (or search a text column) into a new column -
//! the CLI surface of the GUI's "Enrich from web" and the `enrich_rows`
//! chat tool. All three share the same fetch pipeline
//! ([`crate::mcp::tools::enrich_rows`]): bounded-parallel workers, per-host
//! limits and cooldowns, the six-hour page cache and provenance columns.
//!
//! The enriched table goes to stdout in whatever `--format` was requested;
//! a one-line summary (rows scanned, pages fetched, cache hits, failures)
//! goes to stderr.

use std::path::PathBuf;

use crate::mcp::tools::enrich_rows::{self, Extract, RunCtl};

use super::OutputFormat;
use super::output::write_table;
use super::progress::Progress;

#[derive(Debug)]
pub struct Options {
    pub on: String,
    pub target: Option<String>,
    pub extract: Option<String>,
    pub pattern: Option<String>,
    pub limit: Option<usize>,
    pub cell_chars: Option<usize>,
    pub top_k: Option<usize>,
    pub provenance: bool,
    pub refresh: bool,
    pub only_missing: bool,
}

/// Same column reference style as the chat tool: a number is an index,
/// anything else a name.
fn resolve_column(table: &octa::data::DataTable, spec: &str) -> anyhow::Result<usize> {
    if let Ok(i) = spec.parse::<usize>() {
        if i < table.col_count() {
            return Ok(i);
        }
        anyhow::bail!(
            "--enrich-on {i} is out of range (the table has {})",
            table.col_count()
        );
    }
    table
        .columns
        .iter()
        .position(|c| c.name == spec)
        .ok_or_else(|| anyhow::anyhow!("--enrich-on: no column named \"{spec}\""))
}

pub fn run(path: PathBuf, o: Options, format: OutputFormat) -> anyhow::Result<()> {
    let mut table = super::read_table(&path)?;

    let extract = match o.extract.as_deref().unwrap_or("status") {
        "status" => Extract::Status,
        "text" => Extract::Text,
        "pattern" => Extract::Pattern,
        "search" => Extract::Search,
        other => {
            anyhow::bail!("--enrich-extract must be status|text|pattern|search (got \"{other}\")")
        }
    };
    let needs_regex = matches!(extract, Extract::Pattern)
        || (matches!(extract, Extract::Search) && o.pattern.is_some());
    let regex = if needs_regex {
        let pat = o.pattern.as_deref().ok_or_else(|| {
            anyhow::anyhow!("--enrich-extract pattern needs --enrich-pattern with a capture group")
        })?;
        let re = regex::Regex::new(pat)
            .map_err(|e| anyhow::anyhow!("invalid --enrich-pattern regex: {e}"))?;
        if re.captures_len() < 2 {
            anyhow::bail!(
                "--enrich-pattern regex has no capture group - wrap the part to keep in (...)"
            );
        }
        Some(re)
    } else {
        None
    };

    let url_col = resolve_column(&table, &o.on)?;
    let target = o
        .target
        .clone()
        .unwrap_or_else(|| format!("{}_live", table.columns[url_col].name));
    let is_search = matches!(extract, Extract::Search);
    let limit = o.limit.unwrap_or(50).min(500);
    let cell_chars = o.cell_chars.unwrap_or(500);
    let top_k = o.top_k.unwrap_or(1).clamp(1, 5);
    let only_missing_col = if o.only_missing {
        table.columns.iter().position(|c| c.name == target)
    } else {
        None
    };

    let (targets, scanned) =
        enrich_rows::collect_targets(&table, url_col, is_search, limit, None, only_missing_col);

    let ctl = RunCtl::default();
    ctl.total
        .store(targets.len(), std::sync::atomic::Ordering::Relaxed);
    // Progress mirror: a watcher thread turns the worker pool's atomic
    // counter into the CLI's stderr progress line (silent when piped).
    let mut prog = Progress::start(Some(targets.len()));
    let (stop, done) = {
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        (stop, done)
    };
    let watcher = {
        let ctl = ctl.clone();
        let stop = stop.clone();
        let done = done.clone();
        std::thread::spawn(move || {
            while !done.load(std::sync::atomic::Ordering::Relaxed) {
                prog.item(
                    ctl.progress.load(std::sync::atomic::Ordering::Relaxed),
                    "rows",
                );
                std::thread::sleep(std::time::Duration::from_millis(250));
                if stop.load(std::sync::atomic::Ordering::Relaxed) {
                    break;
                }
            }
            prog
        })
    };

    let results = enrich_rows::run_fetch(
        &targets,
        is_search,
        !matches!(extract, Extract::Status),
        o.refresh,
        regex.as_ref(),
        top_k,
        &ctl,
    );
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    done.store(true, std::sync::atomic::Ordering::Relaxed);
    let mut prog = watcher
        .join()
        .map_err(|_| anyhow::anyhow!("progress watcher panicked"))?;
    prog.item(targets.len(), "rows");
    prog.finish();

    let paired = enrich_rows::pair_results(
        results,
        &targets,
        extract,
        o.pattern.is_some(),
        regex.as_ref(),
        cell_chars,
    );

    // Materialise the columns the same way the live tab would: the ops are
    // the shared ones, just applied in place instead of queued for review.
    let mut ops = Vec::new();
    let mut written = enrich_rows::push_column_op(&mut ops, &table, &target, &paired.values);
    if o.provenance {
        written +=
            enrich_rows::push_column_op(&mut ops, &table, &format!("{target}_src"), &paired.srcs);
        written +=
            enrich_rows::push_column_op(&mut ops, &table, &format!("{target}_at"), &paired.ats);
    }
    for op in &ops {
        op.apply_to(&mut table, octa::data::TextOrder::Letters);
    }

    write_table(&table, format)?;
    eprintln!(
        "enrich: {} rows scanned, {} pages fetched ({} from cache), {} values written to \"{}\", {} failures{}",
        scanned,
        paired.fetched,
        paired.cached,
        written,
        target,
        paired.failures.len(),
        if paired.cancelled > 0 {
            format!(", {} cancelled", paired.cancelled)
        } else {
            String::new()
        },
    );
    for (row, err) in paired.failures.iter().take(10) {
        eprintln!("  row {row}: {err}");
    }
    Ok(())
}
