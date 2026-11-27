//! Confirm exactly the duplicate rows shown for the current complete table.

use std::sync::atomic::Ordering;
use std::time::Instant;

use eframe::egui;
use octa::data::dedupe::{KeepWhich, dedupe_dropped_indices};
use octa::data::{CellValue, ViewMode};
use octa::i18n::t;
use octa::ui::control_row::control_line;
use octa::ui::menu_entry::disabled_tip;

use crate::app::sql_panel::DataStamp;
use crate::app::state::{OctaApp, TabState};
use crate::app::tabs::TabId;

const MAX_KEY_CELLS: usize = 1_000_000;
const MAX_KEY_BYTES: usize = 32 * 1024 * 1024;
const MAX_REVIEW_CELLS: usize = 100_000;
const MAX_REVIEW_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
enum Failure {
    ReadOnly,
    Partial,
    Stale,
    TooLarge,
    NoMatches,
}

impl Failure {
    fn message(&self) -> String {
        t(match self {
            Self::ReadOnly => "transform.readonly",
            Self::Partial => "loadall.title",
            Self::Stale => "fill.value_stale",
            Self::TooLarge => "fill.value_large",
            Self::NoMatches => "dialog.fd_no_dups",
        })
    }
}

fn incomplete(tab: &TabState) -> bool {
    tab.large.is_some()
        || tab.server_view.is_some()
        || tab.table.row_offset > 0
        || tab.source_has_more()
        || tab.bg_can_load_more
        || tab.loading_all
        || tab.bg_row_buffer.is_some() && !tab.bg_loading_done.load(Ordering::Relaxed)
}

pub(super) fn unavailable(tab: &TabState, readonly: bool) -> Option<String> {
    if readonly {
        Some(Failure::ReadOnly.message())
    } else if incomplete(tab) {
        Some(Failure::Partial.message())
    } else {
        None
    }
}

fn payload(cell: &CellValue) -> usize {
    match cell {
        CellValue::String(s)
        | CellValue::Nested(s)
        | CellValue::Date(s)
        | CellValue::DateTime(s) => s.len(),
        CellValue::Binary(b) => b.len().saturating_mul(3),
        _ => 0,
    }
}

fn plan_indices(tab: &TabState, keys: &[usize], keep: KeepWhich) -> Result<Vec<usize>, Failure> {
    let table = &tab.table;
    let cols = if keys.is_empty() {
        (0..table.col_count()).collect::<Vec<_>>()
    } else if keys.iter().all(|&c| c < table.col_count()) {
        keys.to_vec()
    } else {
        return Err(Failure::Stale);
    };
    if cols.is_empty() {
        return Err(Failure::NoMatches);
    }
    if table.row_count().saturating_mul(cols.len()) > MAX_KEY_CELLS {
        return Err(Failure::TooLarge);
    }
    let mut bytes = 0usize;
    for row in 0..table.row_count() {
        for &col in &cols {
            if let Some(cell) = table.get(row, col) {
                bytes = bytes.saturating_add(payload(cell));
                if bytes > MAX_KEY_BYTES {
                    return Err(Failure::TooLarge);
                }
            }
        }
    }
    let rows = dedupe_dropped_indices(table, keys, keep);
    if rows.is_empty() {
        Err(Failure::NoMatches)
    } else {
        Ok(rows)
    }
}

fn hidden(tab: &TabState, row: usize) -> bool {
    tab.table.hidden_rows.contains(&row) || tab.filtered_rows.binary_search(&row).is_err()
}

fn same_cell(a: Option<&CellValue>, b: Option<&CellValue>) -> bool {
    match (a, b) {
        (Some(CellValue::Float(a)), Some(CellValue::Float(b))) => a.to_bits() == b.to_bits(),
        _ => a == b,
    }
}

struct Target {
    hidden: bool,
    cells: Vec<Option<CellValue>>,
}

pub(crate) struct DuplicateReview {
    tab: TabId,
    title: String,
    stamp: DataStamp,
    keys: Vec<usize>,
    key_names: String,
    keep: KeepWhich,
    rows: Vec<usize>,
    targets: Vec<Target>,
    hidden_count: usize,
    stale: bool,
    focus_cancel: bool,
    back_to_picker: bool,
}

impl DuplicateReview {
    fn start(tab: &TabState, keys: &[usize], keep: KeepWhich) -> Result<Self, Failure> {
        if incomplete(tab) {
            return Err(Failure::Partial);
        }
        if tab.view_mode != ViewMode::Table || tab.filter_dirty {
            return Err(Failure::Stale);
        }
        let rows = plan_indices(tab, keys, keep)?;
        let width = tab.table.col_count();
        if rows.len().saturating_mul(width) > MAX_REVIEW_CELLS {
            return Err(Failure::TooLarge);
        }
        let mut targets = Vec::with_capacity(rows.len());
        let mut bytes = 0usize;
        for &row in &rows {
            let mut cells = Vec::with_capacity(width);
            for col in 0..width {
                let value = tab.table.get(row, col);
                if let Some(value) = value {
                    bytes = bytes.saturating_add(payload(value));
                    if bytes > MAX_REVIEW_BYTES {
                        return Err(Failure::TooLarge);
                    }
                }
                cells.push(value.cloned());
            }
            targets.push(Target {
                hidden: hidden(tab, row),
                cells,
            });
        }
        let hidden_count = targets.iter().filter(|r| r.hidden).count();
        let key_names = if keys.is_empty() {
            t("dialog.sel_all")
        } else {
            let mut names = keys
                .iter()
                .take(6)
                .map(|&c| excerpt(&tab.table.columns[c].name))
                .collect::<Vec<_>>();
            if keys.len() > names.len() {
                names.push(format!("+{}", keys.len() - names.len()));
            }
            names.join(", ")
        };
        Ok(Self {
            tab: tab.id,
            title: excerpt(&tab.title_display()),
            stamp: DataStamp::of(&tab.table),
            keys: keys.to_vec(),
            key_names,
            keep,
            rows,
            targets,
            hidden_count,
            stale: false,
            focus_cancel: true,
            back_to_picker: false,
        })
    }

    fn current(&self, tab: &TabState) -> bool {
        !self.stale
            && tab.id == self.tab
            && !tab.filter_dirty
            && tab.view_mode == ViewMode::Table
            && self.stamp == DataStamp::of(&tab.table)
    }

    fn matches(&self, tab: &TabState) -> bool {
        self.current(tab)
            && self.rows.iter().zip(&self.targets).all(|(&row, target)| {
                target.hidden == hidden(tab, row)
                    && target
                        .cells
                        .iter()
                        .enumerate()
                        .all(|(col, value)| same_cell(tab.table.get(row, col), value.as_ref()))
            })
            && plan_indices(tab, &self.keys, self.keep).is_ok_and(|rows| rows == self.rows)
    }

    fn back_to_picker(&mut self) {
        self.back_to_picker = true;
    }
}

fn excerpt(s: &str) -> String {
    let mut chars = s.chars();
    let mut out = chars.by_ref().take(56).collect::<String>();
    if chars.next().is_some() {
        out.push_str("...");
    }
    out
}

fn sample(value: Option<&CellValue>) -> String {
    match value {
        None | Some(CellValue::Null) => t("dialog.cf_empty"),
        Some(CellValue::String(s) | CellValue::Nested(s)) => format!("{:?}", excerpt(s)),
        Some(CellValue::Binary(b)) => format!("{:?}", &b[..b.len().min(8)]),
        Some(value) => excerpt(&value.to_string()),
    }
}

pub(super) fn preview_key() -> egui::Id {
    egui::Id::new("dbviewy_duplicate_review_back")
}

impl OctaApp {
    pub(crate) fn open_duplicate_review(&mut self, keys: &[usize], keep: KeepWhich) -> bool {
        if self.duplicate_review.is_some()
            || self.value_fill.is_some()
            || self.paste_special.is_some()
            || self.replace_review.is_some()
        {
            return false;
        }
        if let Some(why) = unavailable(&self.tabs[self.active_tab], self.is_readonly()) {
            self.status_message = Some((why, Instant::now()));
            return false;
        }
        if self.tabs[self.active_tab].filter_dirty {
            self.recompute_filter();
        }
        match DuplicateReview::start(&self.tabs[self.active_tab], keys, keep) {
            Ok(review) => {
                self.duplicate_review = Some(review);
                true
            }
            Err(e) => {
                self.status_message = Some((e.message(), Instant::now()));
                false
            }
        }
    }

    pub(super) fn return_duplicate_review_to_picker(&mut self) {
        if let Some(review) = &mut self.duplicate_review {
            review.back_to_picker();
        }
    }

    pub(super) fn commit_duplicate_review(&mut self, review: &DuplicateReview) -> bool {
        let tab = &self.tabs[self.active_tab];
        if self.is_readonly() || incomplete(tab) || !review.matches(tab) {
            return false;
        }
        super::apply_duplicate_rows(self, &review.keys, review.keep, &review.rows);
        self.cleanup_panel.scanned = false;
        true
    }

    pub(super) fn render_duplicate_review(&mut self, ctx: &egui::Context) {
        let Some(mut review) = self.duplicate_review.take() else {
            return;
        };
        let tab = &self.tabs[self.active_tab];
        review.stale |= !review.current(tab);
        let reason = if review.stale {
            Some(Failure::Stale.message())
        } else {
            unavailable(tab, self.is_readonly())
        };
        let response =
            egui::Modal::new(egui::Id::new("dbviewy_duplicate_review")).show(ctx, |ui| {
                let viewport = ctx.content_rect();
                ui.set_width((viewport.width() - 40.0).clamp(190.0, 500.0));
                ui.heading(t("dedupe.title"));
                ui.label(&review.title);
                ui.separator();
                ui.strong(format!("{} {}", review.rows.len(), t("status_bar.rows")));
                ui.label(format!(
                    "{}: {}",
                    t("status_bar.hidden_rows"),
                    review.hidden_count
                ));
                ui.label(format!(
                    "{}: {}",
                    t("dialog.fd_key_columns"),
                    review.key_names
                ));
                ui.label(format!(
                    "{} {}",
                    t("dedupe.keep_label"),
                    t(match review.keep {
                        KeepWhich::First => "dedupe.keep_first",
                        KeepWhich::Last => "dedupe.keep_last",
                    })
                ));
                if let Some(reason) = &reason {
                    ui.label(reason);
                }
                let shown_cols = if review.keys.is_empty() {
                    (0..tab.table.col_count().min(3)).collect::<Vec<_>>()
                } else {
                    review.keys.iter().take(3).copied().collect()
                };
                ui.separator();
                ui.weak(t("dialog.pv_preview"));
                egui::ScrollArea::vertical()
                    .id_salt("duplicate_sample_rows")
                    .max_height((viewport.height() - 235.0).clamp(44.0, 165.0))
                    .show(ui, |ui| {
                        for (&row, target) in review.rows.iter().zip(&review.targets).rev().take(4)
                        {
                            ui.strong(format!("{} {}", t("dialog.formula_row"), row + 1));
                            let line = shown_cols
                                .iter()
                                .map(|&col| sample(target.cells.get(col).and_then(Option::as_ref)))
                                .collect::<Vec<_>>()
                                .join(" | ");
                            ui.add(egui::Label::new(egui::RichText::new(line).monospace()).wrap());
                        }
                    });
                ui.separator();
                control_line(ui, |ui| {
                    let cancel = ui.button(t("common.cancel"));
                    if review.focus_cancel {
                        cancel.request_focus();
                        review.focus_cancel = false;
                    }
                    let delete =
                        ui.add_enabled(reason.is_none(), egui::Button::new(t("common.delete")));
                    let delete = if let Some(reason) = &reason {
                        delete.on_disabled_hover_text(disabled_tip(&t("dedupe.menu_hint"), reason))
                    } else {
                        delete.on_hover_text(t("dedupe.menu_hint"))
                    };
                    (cancel.clicked(), delete.clicked())
                })
                .inner
            });
        let (cancel, delete) = response.inner;
        let closed = response.should_close() || cancel;
        let applied = !closed && delete && self.commit_duplicate_review(&review);
        if closed || applied {
            ctx.input_mut(|i| {
                i.events.retain(|event| {
                    !matches!(
                        event,
                        egui::Event::Key {
                            key: egui::Key::Enter | egui::Key::Space | egui::Key::Escape,
                            ..
                        }
                    )
                });
            });
            ctx.memory_mut(|m| m.stop_text_input());
            let tab = &mut self.tabs[self.active_tab];
            if closed && review.back_to_picker && tab.id == review.tab {
                tab.show_find_duplicates = true;
                ctx.data_mut(|d| d.insert_temp(preview_key(), true));
            }
            ctx.request_repaint();
        } else {
            review.stale |= delete;
            self.duplicate_review = Some(review);
        }
    }
}

#[cfg(test)]
#[path = "find_duplicates_review_tests.rs"]
mod tests;
