//! Enrich a URL column of the live tab from the web - the GUI door onto
//! the `enrich_rows` tool the chat assistant uses. Right-click a column
//! header -> "Enrich from web" starts it.
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

/// How many rows a GUI-triggered run may fetch - the tool's own hard cap.
const GUI_ENRICH_LIMIT: usize = 500;

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
    /// Right-click "Enrich from web": fetch every row's URL on a worker and
    /// write the HTTP status into `<col>_live` as pending edits, with
    /// `<col>_live_src` / `<col>_live_at` recording where each value came
    /// from and when - the audit trail entry alone cannot answer that.
    ///
    /// Status extraction is the deliberate default here, not the page
    /// text: a right-click should answer "which links are alive?" without
    /// flooding the sheet; richer extracts stay a chat tool call away.
    pub(crate) fn start_enrich_column(&mut self, col_idx: usize) {
        if self.enrich_run.is_some() {
            self.status_message = Some((
                "An enrich run is already in progress.".to_string(),
                std::time::Instant::now(),
            ));
            return;
        }
        if self.settings.write_protection {
            self.status_message = Some((
                "Enrich refused: Write protection is on (Settings).".to_string(),
                std::time::Instant::now(),
            ));
            return;
        }
        if self.is_readonly() {
            self.status_message = Some((
                "Enrich refused: this tab is read-only.".to_string(),
                std::time::Instant::now(),
            ));
            return;
        }
        let tab = &self.tabs[self.active_tab];
        if tab.large.is_some() {
            self.status_message = Some((
                "Enrich refused: large-file tabs are read-only.".to_string(),
                std::time::Instant::now(),
            ));
            return;
        }
        let Some(col_name) = tab.table.columns.get(col_idx).map(|c| c.name.clone()) else {
            return;
        };
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
        let params = Params {
            open_tab: "@active".to_string(),
            url_column: ColRef::Name(col_name.clone()),
            target_column: format!("{col_name}_live"),
            extract: Extract::Status,
            pattern: None,
            limit: Some(GUI_ENRICH_LIMIT),
            cell_chars: None,
            provenance: true,
            refresh: false,
        };
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
