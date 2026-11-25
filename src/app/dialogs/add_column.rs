//! "Insert Column" modal dialog: name + type + insert position + formula.
//! On confirm, the formula (if any) is evaluated for every existing row.

use std::fmt::Write;
use std::ops::Range;

use eframe::egui;
use egui::RichText;

use octa::data;
use octa::ui::settings::{
    DialogSize, center_on_first_show, dialog_header, remember_dialog_rect, size_dialog_window,
};

use super::super::init::COLUMN_TYPES;
use super::super::state::OctaApp;

pub(crate) fn render_add_column_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if !app.tabs[app.active_tab].show_add_column_dialog {
        return;
    }
    let mut should_add = false;
    let mut close = false;

    // The dialog has no dedicated state struct (its fields live on TabState),
    // so persist the window-size mode in egui temp memory keyed by the dialog
    // id instead of adding another TabState field.
    let dialog_id = egui::Id::new("octa_add_column_dialog");
    let size_key = dialog_id.with("octa_dlg_size");
    let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key).unwrap_or_default());
    let minimized = size == DialogSize::Minimized;

    let center = center_on_first_show(ctx, egui::vec2(360.0, 320.0));
    let window = egui::Window::new("octa_add_column")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true).default_width(360.0).default_pos(center)
    });

    let inner = window.show(ctx, |ui| {
        if dialog_header(
            ui,
            "add_column_header",
            RichText::new(octa::i18n::t("dialog.add_column_title")),
            &mut size,
        ) {
            close = true;
        }

        if minimized {
            return;
        }

        egui::Panel::bottom("add_column_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show(ui, |ui| {
                octa::ui::control_row::control_line(ui, |ui| {
                    if ui.button(octa::i18n::t("common.add")).clicked()
                        && !app.tabs[app.active_tab].new_col_name.is_empty()
                    {
                        should_add = true;
                    }
                    if ui.button(octa::i18n::t("common.cancel")).clicked() {
                        close = true;
                    }
                });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(octa::i18n::t("dialog.field_name"));
                ui.text_edit_singleline(&mut app.tabs[app.active_tab].new_col_name);
            });
            // Autofill: show existing column names that match what the user
            // has typed so far. Clicking one fills the Name field.
            let typed = app.tabs[app.active_tab].new_col_name.clone();
            if !typed.is_empty() {
                let lower = typed.to_lowercase();
                let matches: Vec<String> = app.tabs[app.active_tab]
                    .table
                    .columns
                    .iter()
                    .filter(|c| {
                        let n = c.name.to_lowercase();
                        n != lower && n.contains(&lower)
                    })
                    .take(8)
                    .map(|c| c.name.clone())
                    .collect();
                if !matches.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new(octa::i18n::t("dialog.autofill"))
                                .size(10.0)
                                .color(ui.visuals().weak_text_color()),
                        );
                        for name in matches {
                            if ui.small_button(&name).clicked() {
                                app.tabs[app.active_tab].new_col_name = name;
                            }
                        }
                    });
                }
            }
            octa::ui::control_row::control_line(ui, |ui| {
                ui.label(octa::i18n::t("dialog.field_type"));
                egui::ComboBox::from_id_salt("col_type_combo")
                    .selected_text(app.tabs[app.active_tab].new_col_type.as_str())
                    .show_ui(ui, |ui| {
                        for t in COLUMN_TYPES {
                            ui.selectable_value(
                                &mut app.tabs[app.active_tab].new_col_type,
                                t.to_string(),
                                *t,
                            );
                        }
                    });
            });
            ui.add_space(4.0);
            octa::ui::control_row::control_line(ui, |ui| {
                ui.label(octa::i18n::t("dialog.insert_at_position"));
                let col_count = app.tabs[app.active_tab].table.col_count();
                // Plain text input instead of DragValue - DragValue draws
                // hover spinner arrows that look out of place in this dialog
                // and the value range is small enough that typing it is
                // easier than dragging. Empty buffer falls back to
                // `col_count + 1` (append at end).
                let target_pos = app.tabs[app.active_tab].insert_col_at.unwrap_or(col_count) + 1;
                if app.tabs[app.active_tab].insert_col_at_text.is_empty() {
                    app.tabs[app.active_tab].insert_col_at_text = target_pos.to_string();
                }
                let parsed = app.tabs[app.active_tab]
                    .insert_col_at_text
                    .trim()
                    .parse::<usize>()
                    .ok();
                let valid = parsed.is_some_and(|v| (1..=col_count + 1).contains(&v));
                let buf_is_empty = app.tabs[app.active_tab].insert_col_at_text.is_empty();
                let buf = &mut app.tabs[app.active_tab].insert_col_at_text;
                let mut text_edit = egui::TextEdit::singleline(buf).desired_width(48.0);
                if !valid && !buf_is_empty {
                    text_edit = text_edit.text_color(egui::Color32::from_rgb(220, 80, 80));
                }
                if ui.add(text_edit).changed()
                    && let Some(v) = app.tabs[app.active_tab]
                        .insert_col_at_text
                        .trim()
                        .parse::<usize>()
                        .ok()
                        .filter(|v| (1..=col_count + 1).contains(v))
                {
                    app.tabs[app.active_tab].insert_col_at = Some(v - 1);
                }
                ui.label(format!("/ {}", col_count + 1));
            });
            octa::ui::control_row::control_line(ui, |ui| {
                ui.label(octa::i18n::t("dialog.field_formula"));
                ui.add(
                    egui::TextEdit::singleline(&mut app.tabs[app.active_tab].new_col_formula)
                        .hint_text(octa::i18n::t("dialog.formula_hint")),
                );
            });
            ui.label(
                RichText::new(octa::i18n::t("dialog.add_column_tip"))
                    .size(10.0)
                    .color(ui.visuals().weak_text_color()),
            );
        });
    });

    if let Some(inner) = inner {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    // Persist size for the next frame; reset to Normal once the dialog closes
    // so a leftover Minimized/Maximized doesn't carry into the next open.
    ctx.data_mut(|d| {
        d.insert_temp(
            size_key,
            if close || should_add {
                DialogSize::Normal
            } else {
                size
            },
        )
    });

    if should_add {
        add_formula_column(app);
    }
    if close {
        app.tabs[app.active_tab].show_add_column_dialog = false;
        app.tabs[app.active_tab].insert_col_at_text.clear();
    }
}

/// Find the decimal row spans once. The evaluator still validates the whole
/// expression. This template only rewrites a relative row, never its column,
/// an anchored row or an invalid row zero.
struct FormulaRows<'a> {
    source: &'a str,
    relative: Vec<(Range<usize>, usize)>,
}

impl<'a> FormulaRows<'a> {
    fn new(source: &'a str) -> Self {
        let mut relative = Vec::new();
        let bytes = source.as_bytes();
        let mut i = 0;
        while let Some(byte) = bytes.get(i) {
            if *byte == b'"' {
                i += 1;
                // A doubled quote belongs to the criterion text. Never shift
                // a cell-like identifier anywhere inside a quoted literal.
                while let Some(&quoted) = bytes.get(i) {
                    i += 1;
                    if quoted == b'"' {
                        if bytes.get(i) == Some(&b'"') {
                            i += 1;
                        } else {
                            break;
                        }
                    }
                }
                continue;
            }
            if !byte.is_ascii_alphabetic() {
                i += 1;
                continue;
            }
            while bytes.get(i).is_some_and(u8::is_ascii_alphabetic) {
                i += 1;
            }
            // A dollar sign here ends the match: the following digits don't
            // move, unlike those in e.g. `$A1` where only the column is fixed.
            let start = i;
            while bytes.get(i).is_some_and(u8::is_ascii_digit) {
                i += 1;
            }
            if start != i
                && let Ok(row @ 1..) = source[start..i].parse::<usize>()
            {
                relative.push((start..i, row));
            }
        }
        Self { source, relative }
    }

    fn at<'b>(&'b self, offset: usize, buffer: &'b mut String) -> Option<&'b str> {
        if offset == 0 || self.relative.is_empty() {
            return Some(self.source);
        }
        buffer.clear();
        let mut copied = 0;
        for (span, row) in &self.relative {
            buffer.push_str(&self.source[copied..span.start]);
            write!(buffer, "{}", row.checked_add(offset)?).ok()?;
            copied = span.end;
        }
        buffer.push_str(&self.source[copied..]);
        Some(buffer)
    }
}

/// Find an exact single-column SUM whose lower endpoint is fixed and whose
/// upper endpoint advances one row per result. Subexpressions, different
/// columns, and ranges that cross over the fixed endpoint use the evaluator.
fn running_sum_range(formula: &str) -> Option<(&str, usize)> {
    fn endpoint(s: &str) -> Option<(&str, usize, bool)> {
        let s = s.trim_ascii();
        let s = s.strip_prefix('$').unwrap_or(s);
        let letters = s.bytes().take_while(u8::is_ascii_alphabetic).count();
        let (col, row) = s.split_at(letters);
        let fixed = row.starts_with('$');
        let digits = row.strip_prefix('$').unwrap_or(row);
        if col.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let number = digits.parse::<usize>().ok()?;
        (number > 0).then_some((col, number, fixed))
    }

    let (name, range) = formula.trim().split_once('(')?;
    let name = name.trim_ascii_end();
    if !(name.eq_ignore_ascii_case("SUM") || name.eq_ignore_ascii_case("SUMME")) {
        return None;
    }
    let (a, b) = range.strip_suffix(')')?.split_once(':')?;
    let a = endpoint(a)?;
    let b = endpoint(b)?;
    let (fixed, advancing) = match (a.2, b.2) {
        (true, false) => (a, b),
        (false, true) => (b, a),
        _ => return None,
    };
    (fixed.0.eq_ignore_ascii_case(advancing.0) && fixed.1 <= advancing.1)
        .then_some((advancing.0, advancing.1))
}

type FormulaResults = Vec<(usize, f64)>;

fn fill_running_sum(formula: &str, table: &data::DataTable) -> Option<FormulaResults> {
    let (column, first_end) = running_sum_range(formula)?;
    let rows = table.row_count();
    if rows == 0 {
        return Some(Vec::new());
    }

    // The normal evaluator validates the first range and sums it in source
    // order. Then read just the newly included cells. SUM(cell) deliberately
    // keeps the same coercion as the full range (numeric text, blanks and
    // non-numeric types), without using differences of rounded prefix sums.
    let mut sum = data::evaluate_formula(formula, table)?;
    let fill_rows = rows.min(usize::MAX - first_end + 1);
    let mut results = Vec::with_capacity(fill_rows);
    results.push((0, sum));
    let mut next = first_end.min(rows);
    let mut term = format!("SUM({column}");
    let prefix = term.len();

    for offset in 1..fill_rows {
        let end = (first_end + offset).min(rows);
        for source in next..end {
            term.truncate(prefix);
            write!(term, "{})", source + 1).ok()?;
            sum += data::evaluate_formula(&term, table)?;
        }
        next = end;
        results.push((offset, sum));
    }
    Some(results)
}

fn fill_formula_template(
    formula: &str,
    table: &data::DataTable,
) -> (FormulaResults, Option<data::FormulaBadCell>) {
    let template = FormulaRows::new(formula);
    if template.relative.is_empty() && table.row_count() > 0 {
        // An anchored range and constant criterion cannot change from row
        // to row. Evaluate the whole template once, not once per result.
        let outcome = data::evaluate_formula_with_diagnostics(formula, table);
        let results = match outcome.value {
            Some(value) => (0..table.row_count()).map(|row| (row, value)).collect(),
            None => Vec::new(),
        };
        return (results, outcome.bad_cell);
    }
    let mut shifted = String::with_capacity(formula.len());
    let mut results = Vec::new();
    let mut first_bad = None;
    for row in 0..table.row_count() {
        let Some(formula) = template.at(row, &mut shifted) else {
            continue;
        };
        let outcome = data::evaluate_formula_with_diagnostics(formula, table);
        if first_bad.is_none() {
            first_bad = outcome.bad_cell;
        }
        if let Some(value) = outcome.value {
            results.push((row, value));
        }
    }
    (results, first_bad)
}

/// Add the formula column the dialog describes, as ONE undo step.
fn add_formula_column(app: &mut OctaApp) {
    if app.is_readonly() {
        return;
    }
    let tab = &mut app.tabs[app.active_tab];
    let start = tab.table.undo_stack.len();
    let col_count = tab.table.col_count();
    let idx = tab.insert_col_at.unwrap_or(col_count).min(col_count);
    let mut results = Vec::new();
    if let Some(formula) = tab.new_col_formula.trim().strip_prefix('=') {
        let row_count = tab.table.row_count();
        // Read the pre-insertion table, including existing edits. A formula
        // must see the same original columns regardless of output position.
        let first_bad;
        (results, first_bad) = match fill_running_sum(formula, &tab.table) {
            Some(values) => (values, None),
            None => fill_formula_template(formula, &tab.table),
        };
        let skipped = row_count - results.len();
        if skipped > 0 {
            let mut banner = format!(
                "{} {}/{} {}.",
                octa::i18n::t("dialog.formula_skipped"),
                skipped,
                row_count,
                octa::i18n::t("dialog.formula_rows"),
            );
            if let Some(bad) = first_bad {
                let name = tab
                    .table
                    .columns
                    .get(bad.col)
                    .map(|c| c.name.as_str())
                    .unwrap_or("?");
                let first_ref = octa::i18n::t("dialog.formula_first_ref");
                let row = octa::i18n::t("dialog.formula_row");
                let number = bad.row + 1;
                let content = bad.content;
                banner.push_str(&format!(
                    " {first_ref} \"{name}\", {row} {number} = {content:?}"
                ));
            }
            tab.parse_error_banner = Some(banner);
        }
    }
    tab.table
        .insert_column(idx, tab.new_col_name.clone(), tab.new_col_type.clone());
    for (row, result) in results {
        let value = if result.fract() == 0.0 && result.abs() < i64::MAX as f64 {
            data::CellValue::Int(result as i64)
        } else {
            data::CellValue::Float(result)
        };
        tab.table.set(row, idx, value);
    }
    // The column and every formula value come out with one Ctrl+Z.
    tab.table.coalesce_undo_since(start);
    if let Some((row, _)) = tab.table_state.selected_cell {
        tab.table_state.selected_cell = Some((row, idx));
    }
    tab.table_state.widths_initialized = false;
    tab.filter_dirty = true;
    tab.show_add_column_dialog = false;
    tab.insert_col_at_text.clear();
}

#[cfg(test)]
#[path = "add_column_tests.rs"]
mod tests;
