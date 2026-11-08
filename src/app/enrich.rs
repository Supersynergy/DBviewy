//! Enrich a URL column of the live tab from the web - the GUI door onto
//! the `enrich_rows` tool the chat assistant uses. Right-click a column
//! header -> "Enrich from web..." starts it.
//!
//! The fetch runs on a worker thread (up to 500 pages is far too slow to
//! hold a frame), the results ride the same `pending_tab_edits` queue the
//! assistant's batches use, so every cell lands in the edit audit trail
//! and one Ctrl+Z undoes the run. Progress reports ride
//! `pending_status_msgs` - the only channel a worker may use to reach the
//! status bar.

use crate::app::OctaApp;
use crate::mcp::tools::edit_open_tab::ColRef;
use crate::mcp::tools::enrich_rows::{Extract, Params};

/// How many rows a GUI-triggered run may fetch - the tool's own hard cap.
const GUI_ENRICH_LIMIT: usize = 500;

impl OctaApp {
    /// Right-click "Enrich from web...": fetch every row's URL on a worker
    /// and write the HTTP status into `<col>_live` as pending edits.
    ///
    /// Status extraction is the deliberate default here, not the page
    /// text: a right-click should answer "which links are alive?" without
    /// flooding the sheet; richer extracts stay a chat tool call away.
    pub(crate) fn start_enrich_column(&mut self, col_idx: usize) {
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
        let params = Params {
            open_tab: "@active".to_string(),
            url_column: ColRef::Name(col_name.clone()),
            target_column: format!("{col_name}_live"),
            extract: Extract::Status,
            pattern: None,
            limit: Some(GUI_ENRICH_LIMIT),
            cell_chars: None,
        };
        self.status_message = Some((
            format!("Enriching \"{col_name}\" from the web (up to {GUI_ENRICH_LIMIT} rows)..."),
            std::time::Instant::now(),
        ));
        std::thread::spawn(move || {
            let msg = match crate::mcp::tools::enrich_rows::run(&ctx, &params) {
                Ok(out) => {
                    let fetched = out["pages_fetched"].as_u64().unwrap_or(0);
                    let written = out["values_written"].as_u64().unwrap_or(0);
                    let failed = out["failures"].as_array().map(|a| a.len()).unwrap_or(0);
                    let target = out["target_column"].as_str().unwrap_or("column");
                    format!(
                        "Enrich done: {written} values into \"{target}\" \
                         ({fetched} fetched, {failed} failed) - review in the edit audit trail"
                    )
                }
                Err(e) => format!("Enrich failed: {e:#}"),
            };
            if let Ok(mut q) = msgs.lock() {
                q.push(msg);
            }
        });
    }
}
