//! "Insert Column" modal dialog: name + type + insert position + formula.
//! On confirm, the formula (if any) is evaluated for every existing row.

use std::collections::HashMap;
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

// Used by the library evaluator as well. Keep the library's formula API
// unchanged while ensuring E3 in 1E3 isn't mistaken for a cell to shift.
#[path = "../../data/formula_lex.rs"]
mod formula_lex;

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
            if byte.is_ascii_digit() || *byte == b'.' {
                let Some(end) = formula_lex::number_end(bytes, i) else {
                    break; // preserve malformed literals for evaluator rejection
                };
                i = end;
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

    /// Memoize only a whole lookup call whose first argument is itself
    /// the sole moving reference. The other arguments cannot vary by row.
    /// Row numbers are one-based to keep the same overflow check as `at`.
    fn repeated_lookup_key(&self) -> Option<(usize, usize)> {
        let [(_, row)] = self.relative.as_slice() else {
            return None;
        };
        let (function, args) = self.source.split_once('(')?;
        if !["XLOOKUP", "XVERWEIS", "XMATCH", "XVERGLEICH"]
            .iter()
            .any(|name| name.eq_ignore_ascii_case(function.trim_ascii()))
        {
            return None;
        }
        let body = args.trim_ascii_end().strip_suffix(')')?;
        let (arg, _) = body.split_once([',', ';'])?;
        let arg = arg.trim_ascii();
        let cell = arg.strip_prefix('$').unwrap_or(arg);
        let letters = cell.bytes().take_while(u8::is_ascii_alphabetic).count();
        let (col, digits) = cell.split_at(letters);
        if letters == 0
            || !digits.bytes().all(|b| b.is_ascii_digit())
            || digits.parse::<usize>().ok()? != *row
        {
            return None;
        }
        let col = col
            .bytes()
            .try_fold(0usize, |col, letter| {
                col.checked_mul(26)?
                    .checked_add(usize::from(letter.to_ascii_uppercase() - b'A' + 1))
            })?
            .checked_sub(1)?;

        // Do not apply early reuse to an outer expression or an extra call
        // after the lookup. Doubled quotes toggle twice without visiting
        // any unquoted characters inside the literal.
        let mut depth = 0usize;
        let mut quoted = false;
        for b in body.bytes() {
            match b {
                b'"' => quoted = !quoted,
                b'(' if !quoted => depth += 1,
                b')' if !quoted => depth = depth.checked_sub(1)?,
                _ => {}
            }
        }
        (depth == 0 && !quoted).then_some((*row, col))
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
    fill_shifted_template(
        &template,
        table,
        template.repeated_lookup_key(),
        data::evaluate_formula_with_diagnostics,
    )
}

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
enum LookupMemoKey<'a> {
    Blank,
    Boolean(bool),
    Integer(i64),
    Float(u64),
    Text(&'a str),
}

impl<'a> LookupMemoKey<'a> {
    fn from_cell(cell: Option<&'a data::CellValue>) -> Option<Self> {
        use data::CellValue;
        match cell {
            None | Some(CellValue::Null) => Some(Self::Blank),
            Some(CellValue::Bool(value)) => Some(Self::Boolean(*value)),
            Some(CellValue::Int(value)) => Some(Self::Integer(*value)),
            Some(CellValue::Float(value)) if value.is_finite() => {
                Some(Self::Float(value.to_bits()))
            }
            Some(
                CellValue::String(value) | CellValue::Date(value) | CellValue::DateTime(value),
            ) if value.len() <= 256 => Some(Self::Text(value)),
            _ => None,
        }
    }
}

fn fill_shifted_template(
    template: &FormulaRows<'_>,
    table: &data::DataTable,
    lookup: Option<(usize, usize)>,
    mut evaluate: impl FnMut(&str, &data::DataTable) -> data::FormulaOutcome,
) -> (FormulaResults, Option<data::FormulaBadCell>) {
    // Keep this per fill against the immutable pre-insertion table. Borrow
    // short keys rather than copying them; unique-key workloads remain
    // memory-bounded and fall back to the normal evaluator.
    const MAX_MEMO_KEYS: usize = 4096;
    let mut memo = HashMap::<LookupMemoKey<'_>, Option<f64>>::new();
    let mut shifted = String::with_capacity(template.source.len());
    let mut results = Vec::new();
    let mut first_bad = None;
    for row in 0..table.row_count() {
        let key = lookup.and_then(|(base, col)| {
            LookupMemoKey::from_cell(table.get(base.checked_add(row)? - 1, col))
        });
        if let Some(value) = key.and_then(|key| memo.get(&key)) {
            if let Some(value) = value {
                results.push((row, *value));
            }
            continue;
        }
        let Some(formula) = template.at(row, &mut shifted) else {
            continue;
        };
        let outcome = evaluate(formula, table);
        if first_bad.is_none() {
            first_bad = outcome.bad_cell;
        }
        if let Some(value) = outcome.value {
            results.push((row, value));
        }
        // A cached failure was already evaluated at an earlier output
        // row, so it cannot change the first diagnostic in this fill.
        if memo.len() < MAX_MEMO_KEYS
            && let Some(key) = key
        {
            memo.insert(key, outcome.value);
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
