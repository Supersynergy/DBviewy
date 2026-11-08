//! Enrich a column of the live tab from the web - the GUI door onto the
//! `enrich_rows` tool the chat assistant uses. Right-click a column header
//! -> "Enrich from web..." opens a small config dialog; Start runs it.
//!
//! The fetch runs on a worker thread (up to 500 pages is far too slow to
//! hold a frame), the results ride the same `pending_tab_edits` queue the
//! assistant's batches use, so every cell lands in the edit audit trail
//! and one Ctrl+Z undoes the run. `EnrichRun` exposes cancel/progress to
//! the status bar; `pending_status_msgs` carries the worker's final
//! report - the only channel a worker may use to reach the status bar.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize};

use crate::app::OctaApp;
use crate::mcp::tools::edit_open_tab::ColRef;
use crate::mcp::tools::enrich_rows::{Extract, Params, RunCtl};

/// Default row cap offered by the dialog - the tool's own hard cap.
pub(crate) const GUI_ENRICH_LIMIT: usize = 500;

/// What one column's cells mean to the enrich run - the dialog's pick list,
/// mapped onto the tool's `Extract` when the run starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GuiExtract {
    /// HTTP status of each row's URL ("is the listing still alive").
    Status,
    /// Readable page text of each row's URL.
    Text,
    /// First regex capture group from each row's page.
    Pattern,
    /// The cell is a search query: fetch the top hits' pages and mine them.
    Search,
}

/// Open "Enrich from web" dialog state for a tab. Field values seed once
/// when the dialog opens and are read back on Start.
#[derive(Debug)]
pub(crate) struct EnrichDialogState {
    /// Column the right-click landed on - the URL or query source.
    pub col_idx: usize,
    pub mode: GuiExtract,
    /// Target column name; seeded `<col>_live`.
    pub target_buf: String,
    /// Regex buffer (enabled for Pattern and Search modes).
    pub pattern_buf: String,
    /// Row cap buffer; seeded with the GUI limit.
    pub limit_buf: String,
    /// Search-mode hit depth buffer; seeded "3".
    pub top_k_buf: String,
    /// Per-cell character cap buffer for text extraction; seeded "500".
    pub cell_chars_buf: String,
    /// Write `<target>_src` / `<target>_at` provenance columns too.
    pub provenance: bool,
    /// Ignore the six-hour page cache.
    pub refresh: bool,
    /// Skip rows whose target cell is already filled (top-up runs).
    pub only_missing: bool,
    /// Last validation complaint shown inside the dialog.
    pub error: Option<String>,
}

/// A live enrich run the status bar can show and cancel. All fields are
/// worker-shared atomics: `progress` counts processed rows, `total` is
/// filled in once the run knows its target list, `finished` flips when the
/// worker exits (after posting its status message, so the tail is not lost).
#[derive(Debug)]
pub(crate) struct EnrichRun {
    pub cancel: Arc<AtomicBool>,
    pub progress: Arc<AtomicUsize>,
    pub total: Arc<AtomicUsize>,
    pub finished: Arc<AtomicBool>,
}

impl OctaApp {
    /// Guard shared by the dialog opener and the run starter: an enrich run
    /// only makes sense where writes can actually land.
    pub(crate) fn enrich_block_reason(&self) -> Option<&'static str> {
        if self.enrich_run.is_some() {
            Some("An enrich run is already in progress.")
        } else if self.settings.write_protection {
            Some("Enrich refused: Write protection is on (Settings).")
        } else if self.is_readonly() {
            Some("Enrich refused: this tab is read-only.")
        } else if self.tabs[self.active_tab].large.is_some() {
            Some("Enrich refused: large-file tabs are read-only.")
        } else {
            None
        }
    }

    /// Right-click "Enrich from web...": seed the dialog for this column or
    /// report why it cannot run.
    pub(crate) fn open_enrich_dialog(&mut self, col_idx: usize) {
        if let Some(reason) = self.enrich_block_reason() {
            self.status_message = Some((reason.to_string(), std::time::Instant::now()));
            return;
        }
        let tab = &mut self.tabs[self.active_tab];
        let Some(col_name) = tab.table.columns.get(col_idx).map(|c| c.name.clone()) else {
            return;
        };
        tab.enrich_dialog = Some(EnrichDialogState {
            col_idx,
            mode: GuiExtract::Status,
            target_buf: format!("{col_name}_live"),
            pattern_buf: String::new(),
            limit_buf: GUI_ENRICH_LIMIT.to_string(),
            top_k_buf: "3".to_string(),
            cell_chars_buf: "500".to_string(),
            provenance: true,
            refresh: false,
            only_missing: false,
            error: None,
        });
    }

    /// Start button: turn the dialog fields into tool `Params` (validating
    /// them first) and hand the run to a worker thread.
    pub(crate) fn start_enrich(&mut self) {
        let tab = &mut self.tabs[self.active_tab];
        let Some(dlg) = tab.enrich_dialog.take() else {
            return;
        };
        let Some(col_name) = tab.table.columns.get(dlg.col_idx).map(|c| c.name.clone()) else {
            return;
        };
        let params = match params_from(&dlg, &col_name) {
            Ok(p) => p,
            Err(e) => {
                tab.enrich_dialog = Some(EnrichDialogState {
                    error: Some(e),
                    ..dlg
                });
                return;
            }
        };
        self.launch_enrich(params);
    }

    /// Spawn an enrich run on a worker thread: the status bar gets its
    /// `EnrichRun` pill, the worker posts its summary into
    /// `pending_status_msgs`. Called by the dialog's Start button and by
    /// the `enrich` RPC - same run, same pill, same audit-trail queue.
    pub(crate) fn launch_enrich(&mut self, params: Params) {
        let ctx = self.build_tool_context(true);
        let msgs = self.pending_status_msgs.clone();
        let ctl = RunCtl::default();
        let run = EnrichRun {
            cancel: ctl.cancel.clone(),
            progress: ctl.progress.clone(),
            total: ctl.total.clone(),
            finished: Arc::new(AtomicBool::new(false)),
        };
        let finished = run.finished.clone();
        self.enrich_run = Some(run);
        std::thread::spawn(move || {
            let msg = match crate::mcp::tools::enrich_rows::run_ctl(&ctx, &params, &ctl) {
                Ok(out) => {
                    let fetched = out["pages_fetched"].as_u64().unwrap_or(0);
                    let cached = out["cache_hits"].as_u64().unwrap_or(0);
                    let written = out["values_written"].as_u64().unwrap_or(0);
                    let failed = out["failures"].as_array().map(|a| a.len()).unwrap_or(0);
                    let cancelled = out["cancelled_remaining"].as_u64().unwrap_or(0);
                    let target = out["target_column"].as_str().unwrap_or("column");
                    let mut s = format!(
                        "Enrich done: {written} values into \"{target}\" \
                         ({fetched} fetched, {cached} cached, {failed} failed)"
                    );
                    if cancelled > 0 {
                        s.push_str(&format!(", {cancelled} cancelled"));
                    }
                    s.push_str(" - review in the edit audit trail");
                    s
                }
                Err(e) => format!("Enrich failed: {e:#}"),
            };
            if let Ok(mut q) = msgs.lock() {
                q.push(msg);
            }
            finished.store(true, std::sync::atomic::Ordering::Relaxed);
        });
    }

    /// The Cancel button in the status bar: stop dispatching new pages.
    /// In-flight fetches finish and their values still queue - a partial
    /// batch is reviewable like a full one.
    pub(crate) fn cancel_enrich(&mut self) {
        if let Some(run) = &self.enrich_run {
            run.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }
}

/// Turn dialog fields into tool `Params`, or the complaint the dialog shows
/// inline. Kept free of `OctaApp` so every validation path is unit-testable
/// without a GUI.
pub(crate) fn params_from(dlg: &EnrichDialogState, col_name: &str) -> Result<Params, String> {
    let target = dlg.target_buf.trim();
    if target.is_empty() {
        return Err(octa::i18n::t("enrich_dialog.err_target").to_string());
    }
    let limit = dlg
        .limit_buf
        .trim()
        .parse::<usize>()
        .ok()
        .filter(|&n| n > 0)
        .unwrap_or(GUI_ENRICH_LIMIT)
        .min(GUI_ENRICH_LIMIT);
    let extract = match dlg.mode {
        GuiExtract::Status => Extract::Status,
        GuiExtract::Text => Extract::Text,
        GuiExtract::Pattern => Extract::Pattern,
        GuiExtract::Search => Extract::Search,
    };
    let pattern = dlg.pattern_buf.trim();
    // A pattern is required for Pattern mode; the tool validates the regex
    // itself, but catch the empty case here so the user never leaves the
    // dialog.
    if matches!(dlg.mode, GuiExtract::Pattern) && pattern.is_empty() {
        return Err(octa::i18n::t("enrich_dialog.err_pattern").to_string());
    }
    if matches!(dlg.mode, GuiExtract::Pattern | GuiExtract::Search) && !pattern.is_empty() {
        if let Err(e) = regex::Regex::new(pattern) {
            return Err(format!("{e}"));
        }
    }
    Ok(Params {
        open_tab: "@active".to_string(),
        url_column: ColRef::Name(col_name.to_string()),
        target_column: target.to_string(),
        extract,
        pattern: if pattern.is_empty() {
            None
        } else {
            Some(pattern.to_string())
        },
        limit: Some(limit),
        cell_chars: dlg
            .cell_chars_buf
            .trim()
            .parse::<usize>()
            .ok()
            .filter(|&n| n > 0),
        provenance: dlg.provenance,
        refresh: dlg.refresh,
        only_missing: dlg.only_missing,
        top_k: dlg
            .top_k_buf
            .trim()
            .parse::<usize>()
            .ok()
            .filter(|&n| n > 0),
    })
}

#[cfg(test)]
#[path = "enrich_tests.rs"]
mod tests;
