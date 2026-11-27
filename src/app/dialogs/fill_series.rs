//! "Fill series" dialog: fill the selected cells with numbers, an ID's
//! counter, dates or a repeated pattern. The values come from
//! `octa::data::fill_series`; writing goes through `CellValue::parse_like`,
//! the paste path, so a column's type rules still apply, and the whole fill
//! is one undo step.

use std::time::Instant;

use eframe::egui;
use egui::RichText;

use octa::data::fill_series::{self as engine, DateUnit, FillError, FillKind, FillSpec};
use octa::data::{CellValue, DataTable, ViewMode};
use octa::i18n::t;
use octa::ui::control_row::{control_grid, control_row, control_text_edit};
use octa::ui::menu_entry::disabled_tip;
use octa::ui::settings::{
    DialogSize, center_on_first_show, dialog_header, remember_dialog_rect, size_dialog_window,
};

use crate::app::sql_panel::DataStamp;
use crate::app::state::{OctaApp, TabState};
use crate::app::tabs::TabId;

const PREVIEW_VALUES: usize = 5;
const PREVIEW_COLUMNS: usize = 3;

pub(crate) struct FillSeriesState {
    spec: FillSpec,
    tab: TabId,
    stamp: DataStamp,
    title: String,
    /// Column and rows, in display order, fixed when the dialog opened.
    targets: Vec<(usize, Vec<usize>)>,
    target_count: usize,
    stale: bool,
    /// The pattern length as typed; parsed into `spec.pattern`.
    pattern_buf: String,
    error: Option<String>,
    size: DialogSize,
}

impl OctaApp {
    /// Edit > Fill series..., the cell menu and the shortcut.
    pub(crate) fn open_fill_series(&mut self) {
        if self.is_readonly() {
            return;
        }
        if self
            .tabs
            .get(self.active_tab)
            .is_some_and(|tab| tab.filter_dirty)
        {
            self.recompute_filter();
        }
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        let targets = visible_targets(tab);
        let Some((col, rows)) = targets.first() else {
            self.status_message = Some((t("greyed.need_two_cells"), Instant::now()));
            return;
        };
        let spec = guess(&tab.table, *col, rows);
        let target_count = targets.iter().map(|(_, rows)| rows.len()).sum();
        self.fill_series_dialog = Some(FillSeriesState {
            pattern_buf: spec.pattern.to_string(),
            spec,
            tab: tab.id,
            stamp: DataStamp::of(&tab.table),
            title: tab.title_display(),
            targets,
            target_count,
            stale: false,
            error: None,
            size: DialogSize::default(),
        });
    }
}

fn visible_targets(tab: &TabState) -> Vec<(usize, Vec<usize>)> {
    if tab.view_mode != ViewMode::Table || tab.filter_dirty {
        return Vec::new();
    }
    let st = &tab.table_state;
    let mut targets = engine::targets(&st.selected_cols, &st.selected_cells, &tab.filtered_rows);
    targets.retain_mut(|(col, rows)| {
        if *col >= tab.table.col_count() || tab.hidden_columns.contains(col) {
            return false;
        }
        rows.retain(|&r| r < tab.table.row_count());
        rows.len() >= 2
    });
    targets
}

/// Guess from two cells and count the unbroken prefix by reference.
/// The only use of the remaining cells is the default repeat length.
fn guess(table: &DataTable, col: usize, rows: &[usize]) -> FillSpec {
    let first = &rows[..rows.len().min(2)];
    let mut spec = engine::guess(&own_cells(table, col, first));
    let leading = rows
        .iter()
        .take_while(|&&r| match table.get(r, col) {
            None | Some(CellValue::Null) => false,
            Some(CellValue::String(s)) => !s.trim().is_empty(),
            Some(_) => true,
        })
        .count();
    spec.pattern = if leading < rows.len() {
        leading.max(1)
    } else {
        1
    };
    spec
}

impl FillSeriesState {
    fn source_is_current(&self, tab: &TabState) -> bool {
        !self.stale
            && tab.id == self.tab
            && tab.view_mode == ViewMode::Table
            && !tab.filter_dirty
            && DataStamp::of(&tab.table) == self.stamp
    }

    fn scope_label(&self) -> String {
        t("fill.scope")
            .replace("{n}", &self.target_count.to_string())
            .replace("{cols}", &self.targets.len().to_string())
    }

    fn preview(&self, table: &DataTable) -> Result<Vec<(usize, Vec<String>)>, FillError> {
        self.targets
            .iter()
            .take(PREVIEW_COLUMNS)
            .map(|(col, rows)| {
                let rows = &rows[..rows.len().min(PREVIEW_VALUES)];
                let values = series_text(&self.spec, table, *col, rows)?;
                Ok((
                    *col,
                    rows.iter()
                        .zip(values)
                        .map(|(&r, text)| {
                            let value = destination_value(table, r, *col, text).to_string();
                            sample_text(&value)
                        })
                        .collect(),
                ))
            })
            .collect()
    }
}

/// A column's current cells at `rows`, in that order.
fn own_cells(table: &DataTable, col: usize, rows: &[usize]) -> Vec<CellValue> {
    rows.iter()
        .map(|&r| table.get(r, col).cloned().unwrap_or(CellValue::Null))
        .collect()
}

fn series_text(
    spec: &FillSpec,
    table: &DataTable,
    col: usize,
    rows: &[usize],
) -> Result<Vec<String>, FillError> {
    // Only Repeat reads existing values, and never beyond the pattern.
    let own = if spec.kind == FillKind::Repeat {
        own_cells(table, col, &rows[..rows.len().min(spec.pattern)])
    } else {
        Vec::new()
    };
    engine::values(spec, &own, rows.len())
}

fn destination_value(table: &DataTable, row: usize, col: usize, text: String) -> CellValue {
    match table.get(row, col) {
        Some(CellValue::Null) | None => {
            octa::data::convert_value(&CellValue::String(text), &table.columns[col].data_type)
        }
        Some(existing) => CellValue::parse_like(existing, &text),
    }
}

fn sample_text(text: &str) -> String {
    const MAX_CHARS: usize = 48;
    let mut chars = text.chars();
    let mut sample: String = chars
        .by_ref()
        .take(MAX_CHARS)
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect();
    if chars.next().is_some() {
        sample.push_str("...");
    }
    if sample.is_empty() {
        String::from("\"\"")
    } else {
        sample
    }
}

/// The error in words. A Numbers step may have decimals, so its complaint
/// differs from the whole-number one Counter and Dates share.
fn error_text(spec: &FillSpec, e: FillError) -> String {
    if spec.kind == FillKind::Numbers && e == FillError::BadStep {
        t("fill.err_step_number")
    } else {
        t(e.key())
    }
}

#[derive(Debug, PartialEq, Eq)]
enum ApplyError {
    ChangedTarget,
    Series(FillError),
}

/// Write the series into every target column as one undo step.
fn apply_fill(app: &mut OctaApp, st: &FillSeriesState) -> Result<usize, ApplyError> {
    let writable = !app.is_readonly();
    let Some(tab) = app.tabs.get_mut(app.active_tab) else {
        return Err(ApplyError::ChangedTarget);
    };
    // Recheck the exact visible rows and their order once, at confirmation.
    // A new search, sort, column hide or selection must never retarget Fill.
    if !writable || !st.source_is_current(tab) || visible_targets(tab) != st.targets {
        return Err(ApplyError::ChangedTarget);
    }
    // Generate every column before writing, so an invalid series writes nothing.
    let mut plan = Vec::with_capacity(st.targets.len());
    for (col, rows) in &st.targets {
        let values = series_text(&st.spec, &tab.table, *col, rows).map_err(ApplyError::Series)?;
        plan.push((*col, rows, values));
    }
    let start = tab.table.undo_stack.len();
    let mut n = 0;
    for (col, rows, values) in plan {
        for (&r, text) in rows.iter().zip(values) {
            let new = destination_value(&tab.table, r, col, text);
            if tab.table.get(r, col).unwrap_or(&CellValue::Null) != &new {
                tab.table.set(r, col, new);
                n += 1;
            }
        }
    }
    if n != 0 {
        tab.table.coalesce_undo_since(start);
        tab.table_state.invalidate_row_heights();
        tab.filter_dirty = true;
    }
    Ok(n)
}

/// A text field that is greyed with a reason instead of hidden.
fn field(
    ui: &mut egui::Ui,
    label: egui::Id,
    text: &mut String,
    enabled: bool,
    hint: &str,
    why: &str,
) {
    let r = ui
        .add_enabled_ui(enabled, |ui| {
            control_text_edit(ui, 160.0, egui::TextEdit::singleline(text).id_salt(hint))
                .labelled_by(label)
        })
        .inner;
    if enabled {
        r.on_hover_text(t(hint));
    } else {
        r.on_disabled_hover_text(disabled_tip(&t(hint), &t(why)));
    }
}

pub(crate) fn render_fill_series_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(mut st) = app.fill_series_dialog.take() else {
        return;
    };
    let Some(tab) = app.tabs.get(app.active_tab) else {
        return;
    };

    st.stale |= !st.source_is_current(tab);
    let writable = !app.is_readonly();

    let mut close = false;
    let mut apply = false;
    let dialog_id = egui::Id::new("octa_fill_series_dialog");
    let size_key = dialog_id.with("octa_dlg_size");
    let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key).unwrap_or(st.size));
    let minimized = size == DialogSize::Minimized;

    let center = center_on_first_show(ctx, egui::vec2(440.0, 340.0));
    let window = egui::Window::new("octa_fill_series")
        .id(dialog_id)
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(440.0)
            .default_height(340.0)
            .min_width(340.0)
            .default_pos(center)
    });

    let inner = window.show(ctx, |ui| {
        if dialog_header(
            ui,
            "fill_series_header",
            RichText::new(t("fill.title")),
            &mut size,
        ) {
            close = true;
        }
        if minimized {
            return;
        }

        let scope = format!("{}  |  {}", st.title, st.scope_label());
        ui.add(egui::Label::new(RichText::new(&scope).strong()).truncate())
            .on_hover_text(&scope);
        ui.separator();

        let old_spec = st.spec.clone();
        let body_height = (ui.available_height() - 46.0).clamp(94.0, 390.0);
        let preview_error = egui::ScrollArea::vertical()
            .id_salt("fill_series_body")
            .max_height(body_height)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                let kind = st.spec.kind;
                let repeat = kind == FillKind::Repeat;
                control_grid(ui, "fill_series_grid", |ui| {
                    let kind_label = ui.label(t("fill.kind")).on_hover_text(t("fill.kind_hint"));
                    egui::ComboBox::from_id_salt("fill_series_kind")
                        .selected_text(t(kind.key()))
                        .show_ui(ui, |ui| {
                            for k in FillKind::ALL {
                                ui.selectable_value(&mut st.spec.kind, k, t(k.key()));
                            }
                        })
                        .response
                        .labelled_by(kind_label.id)
                        .on_hover_text(t("fill.kind_hint"));
                    ui.end_row();

                    let start_label = ui
                        .label(t("fill.start"))
                        .on_hover_text(t("fill.start_hint"));
                    field(
                        ui,
                        start_label.id,
                        &mut st.spec.start,
                        !repeat,
                        "fill.start_hint",
                        "fill.start_repeat_why",
                    );
                    ui.end_row();

                    let step_label = ui.label(t("fill.step")).on_hover_text(t("fill.step_hint"));
                    field(
                        ui,
                        step_label.id,
                        &mut st.spec.step,
                        !repeat,
                        "fill.step_hint",
                        "fill.start_repeat_why",
                    );
                    ui.end_row();

                    let unit_label = ui.label(t("fill.unit")).on_hover_text(t("fill.unit_hint"));
                    let dates = kind == FillKind::Dates;
                    let r = ui
                        .add_enabled_ui(dates, |ui| {
                            egui::ComboBox::from_id_salt("fill_series_unit")
                                .selected_text(t(st.spec.unit.key()))
                                .show_ui(ui, |ui| {
                                    for u in DateUnit::ALL {
                                        ui.selectable_value(&mut st.spec.unit, u, t(u.key()));
                                    }
                                })
                                .response
                        })
                        .inner
                        .labelled_by(unit_label.id);
                    if dates {
                        r.on_hover_text(t("fill.unit_hint"));
                    } else {
                        r.on_disabled_hover_text(disabled_tip(
                            &t("fill.unit_hint"),
                            &t("fill.unit_why"),
                        ));
                    }
                    ui.end_row();

                    let pattern_label = ui
                        .label(t("fill.pattern"))
                        .on_hover_text(t("fill.pattern_hint"));
                    field(
                        ui,
                        pattern_label.id,
                        &mut st.pattern_buf,
                        repeat,
                        "fill.pattern_hint",
                        "fill.pattern_why",
                    );
                    ui.end_row();
                });
                // An unreadable length means "no pattern", which the preview says.
                st.spec.pattern = st.pattern_buf.trim().parse().unwrap_or(0);

                if st.spec != old_spec {
                    st.error = None;
                }
                ui.add_space(8.0);
                if st.stale || !writable {
                    return Some(t(if st.stale {
                        "fill.stale"
                    } else {
                        "context_menu.header_row_readonly"
                    }));
                }
                ui.label(RichText::new(t("fill.preview")).strong());
                let error = match st.preview(&tab.table) {
                    Ok(previews) => {
                        for (col, samples) in previews {
                            let label =
                                format!("{}: {}", tab.table.columns[col].name, samples.join(", "));
                            ui.add(egui::Label::new(RichText::new(&label).monospace()).truncate())
                                .on_hover_text(label);
                        }
                        None
                    }
                    Err(e) => Some(error_text(&st.spec, e)),
                };
                if let Some(e) = error.as_ref().or(st.error.as_ref()) {
                    octa::ui::message::selectable_message(ui, ui.visuals().error_fg_color, e);
                }
                error
            })
            .inner
            .or_else(|| st.error.clone());
        if (st.stale || !writable)
            && let Some(reason) = &preview_error
        {
            ui.weak(reason);
        }
        ui.separator();
        control_row(ui, |ui| {
            let button = ui.add_enabled(preview_error.is_none(), egui::Button::new(t("fill.fill")));
            apply = button.clicked();
            if let Some(reason) = &preview_error {
                button.on_disabled_hover_text(disabled_tip(&t("fill.fill_hint"), reason));
            } else {
                button.on_hover_text(t("fill.fill_hint"));
            }
            if ui
                .button(t("common.cancel"))
                .on_hover_text(t("fill.cancel_hint"))
                .clicked()
            {
                close = true;
            }
        });
    });

    if let Some(inner) = inner {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    st.size = size;
    ctx.data_mut(|d| d.insert_temp(size_key, size));

    if apply && !close {
        // A focused button accepts Enter or Space. Do not let that same event
        // edit a cell when the grid is rendered later in this frame.
        ctx.input_mut(|input| {
            input.events.retain(|event| {
                !matches!(
                    event,
                    egui::Event::Key {
                        key: egui::Key::Enter | egui::Key::Space,
                        ..
                    }
                )
            });
        });
        match apply_fill(app, &st) {
            Ok(n) => {
                app.status_message = Some((
                    t("fill.done").replace("{n}", &n.to_string()),
                    Instant::now(),
                ));
                return;
            }
            Err(ApplyError::ChangedTarget) => st.stale = true,
            Err(ApplyError::Series(e)) => st.error = Some(error_text(&st.spec, e)),
        }
    }
    if !close {
        app.fill_series_dialog = Some(st);
    }
}

#[cfg(test)]
#[path = "fill_series_tests.rs"]
mod tests;
