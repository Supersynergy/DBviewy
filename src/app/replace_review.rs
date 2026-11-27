//! Review a find box's bulk table edit, then write exactly what was shown.

use std::collections::HashSet;
use std::time::Instant;

use eframe::egui;
use octa::data::{CellValue, ViewMode, search::RowMatcher};
use octa::i18n::t;
use octa::ui::control_row::control_line;
use octa::ui::menu_entry::disabled_tip;

use super::sql_panel::DataStamp;
use super::state::{OctaApp, TabState};
use super::tabs::TabId;
use crate::view_modes::find_box::{self, FindInput};

const EXAMPLES: usize = 3;

#[derive(Clone, PartialEq, Eq)]
struct Request {
    find: String,
    replacement: String,
    case: bool,
    word: bool,
    regex: bool,
}

impl Request {
    fn from(input: &FindInput) -> Self {
        Self {
            find: input.query.clone(),
            replacement: input.replace.clone(),
            case: input.case_sensitive,
            word: input.whole_word,
            regex: input.regex,
        }
    }

    fn matches(&self, input: &FindInput) -> bool {
        self.find == input.query
            && self.replacement == input.replace
            && self.case == input.case_sensitive
            && self.word == input.whole_word
            && self.regex == input.regex
    }
}

struct VisibleGrid {
    rows: Vec<usize>,
    cols: Vec<usize>,
}

impl VisibleGrid {
    fn rows(tab: &TabState) -> impl Iterator<Item = usize> + '_ {
        tab.filtered_rows
            .iter()
            .copied()
            .filter(|row| *row < tab.table.row_count() && !tab.table.hidden_rows.contains(row))
    }

    fn cols(tab: &TabState) -> impl Iterator<Item = usize> + '_ {
        (0..tab.table.col_count()).filter(|col| !tab.hidden_columns.contains(col))
    }

    fn from(tab: &TabState) -> Self {
        Self {
            rows: Self::rows(tab).collect(),
            cols: Self::cols(tab).collect(),
        }
    }

    fn matches(&self, tab: &TabState) -> bool {
        Self::rows(tab).eq(self.rows.iter().copied())
            && Self::cols(tab).eq(self.cols.iter().copied())
    }

    fn changes<'a>(
        &'a self,
        tab: &'a TabState,
        input: &'a FindInput,
    ) -> impl Iterator<Item = ((usize, usize), &'a CellValue, CellValue)> + 'a {
        let find = find_box::matcher(input);
        // Plain matching avoids a regex, but replacing case-insensitively
        // needs original UTF-8 positions. Compile once outside the cell scan.
        let writer = match &find {
            RowMatcher::Plain(text) => regex::Regex::new(&format!("(?i){}", regex::escape(text)))
                .map(RowMatcher::Literal)
                .unwrap_or(RowMatcher::Invalid),
            RowMatcher::Regex(re) => RowMatcher::Regex(re.clone()),
            RowMatcher::Literal(re) => RowMatcher::Literal(re.clone()),
            RowMatcher::Invalid => RowMatcher::Invalid,
        };
        self.rows
            .iter()
            .flat_map(|&row| self.cols.iter().map(move |&col| (row, col)))
            .filter_map(move |(row, col)| {
                let old = tab.table.get(row, col)?;
                let text = old.to_string();
                if !find.matches(&text) {
                    return None;
                }
                let changed_text = writer.replace_all(&text, &input.replace);
                // An unchanged display must not silently re-type a value.
                if changed_text == text {
                    return None;
                }
                let new = CellValue::parse_like(old, &changed_text);
                (!same_value(old, &new)).then_some(((row, col), old, new))
            })
    }
}

fn same_value(a: &CellValue, b: &CellValue) -> bool {
    match (a, b) {
        (CellValue::Float(a), CellValue::Float(b)) => a.to_bits() == b.to_bits(),
        _ => a == b,
    }
}

struct Edit {
    cell: (usize, usize),
    old: CellValue,
    new: CellValue,
}

struct Example {
    target: String,
    before: String,
    after: String,
}

pub(crate) struct ReplaceReview {
    tab: TabId,
    request: Request,
    stamp: DataStamp,
    grid: VisibleGrid,
    edits: Vec<Edit>,
    examples: Vec<Example>,
    changed_rows: usize,
    changed_cols: usize,
    stale: bool,
    focus_cancel: bool,
}

impl ReplaceReview {
    fn new(tab: &TabState, input: &FindInput) -> Self {
        let grid = VisibleGrid::from(tab);
        let mut rows = HashSet::new();
        let mut cols = HashSet::new();
        let edits: Vec<_> = grid
            .changes(tab, input)
            .map(|(cell, old, new)| {
                rows.insert(cell.0);
                cols.insert(cell.1);
                Edit {
                    cell,
                    old: old.clone(),
                    new,
                }
            })
            .collect();
        let examples = edits
            .iter()
            .take(EXAMPLES)
            .map(|edit| {
                let (r, c) = edit.cell;
                let name = tab.table.columns.get(c).map_or("", |col| col.name.as_str());
                let brief: String = name.chars().take(48).collect();
                let row = r.saturating_add(tab.table.row_offset).saturating_add(1);
                Example {
                    target: format!("{row} / {brief}"),
                    before: example(&edit.old),
                    after: example(&edit.new),
                }
            })
            .collect();
        Self {
            tab: tab.id,
            request: Request::from(input),
            stamp: DataStamp::of(&tab.table),
            grid,
            edits,
            examples,
            changed_rows: rows.len(),
            changed_cols: cols.len(),
            stale: false,
            focus_cancel: true,
        }
    }

    fn current_request<'a>(&self, tab: &'a TabState) -> Option<&'a FindInput> {
        if tab.id != self.tab || tab.view_mode != ViewMode::Table {
            return None;
        }
        tab.find_box_active()
            .filter(|input| input.replacing && self.request.matches(input))
    }

    fn still_matches(&self, tab: &TabState) -> bool {
        let Some(input) = self.current_request(tab) else {
            return false;
        };
        if self.stamp != DataStamp::of(&tab.table) || !self.grid.matches(tab) {
            return false;
        }
        // In-place changes can keep the same stamp. Compare every old/new
        // cell before writing; even a newly matching cell is a conflict.
        let mut found = self.grid.changes(tab, input);
        self.edits.iter().all(|edit| {
            found.next().is_some_and(|(cell, old, new)| {
                cell == edit.cell && same_value(old, &edit.old) && same_value(&new, &edit.new)
            })
        }) && found.next().is_none()
    }
}

fn example(value: &CellValue) -> String {
    if matches!(value, CellValue::Null) {
        return "-".into();
    }
    let owned;
    let text = if let CellValue::String(text) = value {
        text.as_str()
    } else {
        owned = value.to_string();
        &owned
    };
    let mut chars = text.chars();
    let prefix: String = chars
        .by_ref()
        .take(54)
        .flat_map(char::escape_debug)
        .collect();
    let more = if chars.next().is_some() { "..." } else { "" };
    if matches!(value, CellValue::String(_)) {
        format!("\"{prefix}{more}\"")
    } else {
        format!("{prefix}{more}")
    }
}

fn modal_id() -> egui::Id {
    egui::Id::new("table_replace_review")
}

fn restore_key() -> egui::Id {
    modal_id().with("restore_focus")
}

impl OctaApp {
    pub(super) fn open_replace_review(&mut self) {
        if self.is_readonly() || self.replace_review.is_some() {
            return;
        }
        if self.tabs[self.active_tab].filter_dirty {
            self.recompute_filter();
        }
        let tab = &self.tabs[self.active_tab];
        if tab.view_mode == ViewMode::Table
            && let Some(input) = tab.find_box_active().filter(|f| f.replacing)
        {
            self.replace_review = Some(ReplaceReview::new(tab, input));
        }
    }

    fn commit_replace_review(&mut self, review: &mut ReplaceReview) -> bool {
        if review.stale || review.edits.is_empty() || self.is_readonly() {
            return false;
        }
        if self.tabs[self.active_tab].filter_dirty {
            self.recompute_filter();
        }
        let tab = &mut self.tabs[self.active_tab];
        if !review.still_matches(tab) {
            return false;
        }
        let count = review.edits.len();
        let undo_from = tab.table.undo_stack.len();
        for Edit {
            cell: (r, c), new, ..
        } in review.edits.drain(..)
        {
            tab.table.set(r, c, new);
        }
        tab.table.coalesce_undo_since(undo_from);
        tab.table_state.invalidate_row_heights();
        tab.filter_dirty = true;
        self.status_message = Some((
            t("search.replaced_cells").replace("{n}", &count.to_string()),
            Instant::now(),
        ));
        true
    }

    fn restore_replace_review_focus(&self, ctx: &egui::Context) {
        let Some(id) = ctx.data(|d| d.get_temp::<TabId>(restore_key())) else {
            return;
        };
        let modal = ctx.memory(|m| m.top_modal_layer());
        if modal == Some(egui::Modal::default_area(modal_id()).layer()) {
            ctx.request_repaint();
            return;
        }
        ctx.data_mut(|d| d.remove::<TabId>(restore_key()));
        if modal.is_none()
            && self.tabs[self.active_tab].id == id
            && self.tabs[self.active_tab].find_box_active().is_some()
        {
            let field = find_box::replace_field_id(egui::Id::new(("tab_find_box", id)));
            ctx.memory_mut(|m| {
                if m.focused().is_none() {
                    m.request_focus(field);
                }
            });
        }
    }

    pub(crate) fn render_replace_review(&mut self, ctx: &egui::Context) {
        self.restore_replace_review_focus(ctx);
        let Some(mut review) = self.replace_review.take() else {
            return;
        };
        let tab = &self.tabs[self.active_tab];
        if tab.id != review.tab {
            return;
        }
        let why_not = if self.is_readonly() {
            Some(t("context_menu.header_row_readonly"))
        } else if review.stale || review.current_request(tab).is_none() {
            Some(t("search.replace_review_stale"))
        } else if review.edits.is_empty() {
            Some(t("search.replace_review_empty"))
        } else {
            None
        };
        let response = egui::Modal::new(modal_id()).show(ctx, |ui| {
            let viewport = ctx.content_rect();
            ui.set_width((viewport.width() - 40.0).clamp(180.0, 496.0));
            let title = t("search.replace_review_open");
            ui.heading(title.trim_end_matches("..."));
            ui.label(t("search.replace_review_hint"));
            ui.separator();
            let scope = t("search.replace_review_scope")
                .replace("{cells}", &review.edits.len().to_string())
                .replace("{rows}", &review.changed_rows.to_string())
                .replace("{cols}", &review.changed_cols.to_string());
            ui.strong(scope);
            if let Some(reason) = &why_not {
                ui.label(reason);
            }
            egui::ScrollArea::vertical()
                .id_salt("replace_examples")
                .max_height((viewport.height() - 200.0).clamp(56.0, 270.0))
                .show(ui, |ui| {
                    let before = t("datadrift.side_a");
                    let after = t("datadrift.side_b");
                    for row in &review.examples {
                        ui.weak(&row.target);
                        ui.monospace(format!("{before}: {}", row.before));
                        ui.monospace(format!("{after}: {}", row.after));
                        ui.add_space(4.0);
                    }
                });
            ui.separator();
            control_line(ui, |ui| {
                let cancel = ui
                    .button(t("common.cancel"))
                    .on_hover_text(t("fill.cancel_hint"));
                if review.focus_cancel {
                    cancel.request_focus();
                    review.focus_cancel = false;
                }
                let hint = t("search.replace_all_hint");
                let apply = ui.add_enabled(why_not.is_none(), egui::Button::new(t("common.apply")));
                let apply = if let Some(why) = &why_not {
                    apply.on_disabled_hover_text(disabled_tip(&hint, why))
                } else {
                    apply.on_hover_text(hint)
                };
                (cancel.clicked(), apply.clicked())
            })
            .inner
        });
        let (cancel, apply) = response.inner;
        let closed = response.should_close() || cancel;
        let accepted = !closed && apply && self.commit_replace_review(&mut review);
        if closed || accepted {
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
            ctx.data_mut(|d| d.insert_temp(restore_key(), review.tab));
            ctx.request_repaint();
        } else {
            review.stale |= apply;
            self.replace_review = Some(review);
        }
    }
}

#[cfg(test)]
#[path = "replace_review_tests.rs"]
mod tests;
