//! Ordered / Join renderer for the Compare view. Unlike the hash-based
//! `row_diff`, these route through the shared pure logic in
//! `octa::data::compare` (the same code the CLI `--diff` and the MCP
//! `diff_tables` tool use), so the GUI, CLI, and MCP report identical results.
//!
//! - **Ordered** - positional row-by-row cell comparison (`compare_ordered`):
//!   row N on the left versus row N on the right. Trailing rows on the longer
//!   side are reported as added / removed.
//! - **Join** - rows matched by user-picked key column(s) (`compare_join`),
//!   reporting which rows were added, removed, or changed and exactly which
//!   columns differ in each changed pair.
//!
//! Both borrow result cells on demand, using the same projection as the
//! CLI. The two summary columns stay ahead of the source columns.

use eframe::egui;
use egui::{RichText, ScrollArea};
use egui_extras::{Column, TableBuilder};

use octa::data::DataTable;
use octa::data::compare::{CompareOptions, CompareResult, compare_join, compare_ordered};
use octa::i18n::t;

use crate::app::state::TabState;
use crate::compare_output::CompareOutput;
use crate::ui;
use ui::theme::ThemeMode;

/// What the last comparison was computed from. The left table counts as
/// unchanged while its undo, redo and edit counts and its shape stay the
/// same, the key the timeline view uses too.
/// ponytail: an edit undone and replaced by another of the same size between
/// two frames keeps the counts; recomputing every frame is what this cache
/// exists to stop, and a content hash would cost as much as the comparison.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyDiffKey {
    join: bool,
    keys: Vec<String>,
    options: CompareOptions,
    // The right-hand table is not in the key: every site that changes it drops
    // `compare_cache` (`app/file_io/compare.rs`, `central_panel/views.rs`).
    left: Generation,
}

type Generation = (usize, usize, usize, usize, usize, usize);

/// A finished comparison. The result cells remain in the source tables.
pub struct KeyDiffCache {
    key: KeyDiffKey,
    /// egui pass that last used this cache.
    pass: u64,
    outcome: Result<CompareResult, String>,
}

fn generation(t: &DataTable) -> Generation {
    (
        t.undo_stack.len(),
        t.redo_stack.len(),
        t.edits.len(),
        t.row_count(),
        t.col_count(),
        // A replaced table is a new allocation, so this catches every
        // `tab.table = ..` site without each one dropping the cache.
        t.rows.as_ptr() as usize,
    )
}

/// A cache only counts if it was used on the previous pass: while this view is
/// not drawn the table may be freed and a new one reuse its address.
fn reusable(cache: &KeyDiffCache, key: &KeyDiffKey, pass: u64) -> bool {
    cache.key == *key && pass <= cache.pass.saturating_add(1)
}

pub fn render(ui: &mut egui::Ui, tab: &mut TabState, theme_mode: ThemeMode, join: bool) {
    let colors = ui::theme::ThemeColors::for_mode(theme_mode);
    if tab.compare_right_table.is_none() {
        ui.label(RichText::new(t("cmptol.no_table")).color(colors.text_muted));
        return;
    }
    let left_names: Vec<String> = tab.table.columns.iter().map(|c| c.name.clone()).collect();

    if join {
        // The key picker reuses `compare_columns_left` as the chosen keys.
        ui.collapsing(t("cmptol.key_columns"), |ui| {
            draw_key_picker(ui, &tab.table, &mut tab.compare_columns_left, &colors);
            ui.add_space(4.0);
            ui.label(
                RichText::new(t("cmptol.key_columns_hint"))
                    .color(colors.text_muted)
                    .size(11.0),
            );
        });
    }
    super::tolerance::tolerance_controls(
        ui,
        "compare_tolerance",
        &mut tab.compare_tolerance,
        &left_names,
    );
    ui.separator();

    let key_names: Vec<String> = tab
        .compare_columns_left
        .iter()
        .filter_map(|&i| left_names.get(i).cloned())
        .collect();
    if join && key_names.is_empty() {
        ui.label(RichText::new(t("cmptol.pick_keys")).color(colors.text_muted));
        return;
    }

    let options = tab.compare_tolerance.options(&left_names);
    let key = KeyDiffKey {
        join,
        keys: key_names.clone(),
        options: options.clone(),
        left: generation(&tab.table),
    };
    let pass = ui.ctx().cumulative_pass_nr();
    if !tab
        .compare_cache
        .as_ref()
        .is_some_and(|c| reusable(c, &key, pass))
    {
        let right = tab
            .compare_right_table
            .as_deref()
            .expect("checked at the top");
        let left = &tab.table;
        let outcome = if join {
            compare_join(left, right, &key_names, &options)
        } else {
            Ok(compare_ordered(left, right, &options))
        }
        .map_err(|e| format!("{e:#}"));
        tab.compare_cache = Some(KeyDiffCache { key, pass, outcome });
    }
    let Some(cache) = tab.compare_cache.as_mut() else {
        return;
    };
    cache.pass = pass;
    let result = match &cache.outcome {
        Ok(result) => result,
        Err(err) => {
            ui.label(RichText::new(err).color(colors.warning));
            return;
        }
    };

    // Summary line: the true totals up front.
    octa::ui::control_row::control_line(ui, |ui| {
        ui.label(
            RichText::new(
                t("cmptol.summary")
                    .replace("{changed}", &result.changed.len().to_string())
                    .replace("{unchanged}", &result.unchanged.to_string())
                    .replace("{left}", &result.only_in_a.len().to_string())
                    .replace("{right}", &result.only_in_b.len().to_string()),
            )
            .color(colors.text_secondary)
            .size(12.0),
        );
        if result.within_tolerance > 0 {
            ui.label(
                RichText::new(
                    t("cmptol.within").replace("{n}", &result.within_tolerance.to_string()),
                )
                .color(colors.text_muted)
                .size(12.0),
            );
        }
    });
    ui.add_space(6.0);
    if let Some(right) = tab.compare_right_table.as_deref() {
        let output = CompareOutput::new(&tab.table, right, result, join);
        draw_result_table(ui, &output, &colors);
    }
}

/// Checkbox list of the left table's columns, used to pick join keys.
fn draw_key_picker(
    ui: &mut egui::Ui,
    table: &DataTable,
    selected: &mut Vec<usize>,
    colors: &ui::theme::ThemeColors,
) {
    let total = table.col_count();
    if total == 0 {
        ui.label(
            RichText::new(t("cmptol.no_columns"))
                .color(colors.text_muted)
                .size(11.0),
        );
        return;
    }
    for (col_idx, column) in table.columns.iter().enumerate() {
        let name = &column.name;
        let mut picked = selected.contains(&col_idx);
        if ui.checkbox(&mut picked, name).changed() {
            if picked {
                if !selected.contains(&col_idx) {
                    selected.push(col_idx);
                }
            } else {
                selected.retain(|c| *c != col_idx);
            }
        }
    }
}

fn draw_result_table(ui: &mut egui::Ui, rows: &CompareOutput<'_>, colors: &ui::theme::ThemeColors) {
    if rows.row_count() == 0 {
        ui.label(RichText::new(t("cmptol.no_differences")).color(colors.success));
        return;
    }
    let grid = draw_result_grid(ui, rows, colors);
    let area = ui.interact(
        grid.inner_rect,
        ui.make_persistent_id("compare_key_diff_menu"),
        egui::Sense::click(),
    );
    let mut copy = area.hovered() && octa::ui::text_selection::copy_pressed(ui.ctx());
    area.context_menu(|ui| {
        if ui.button(t("compare.copy_table")).clicked() {
            copy = true;
            ui.close();
        }
    });
    if copy {
        match rows.to_tsv() {
            Ok(text) => ui.ctx().copy_text(text),
            Err(err) => {
                tracing::error!("Could not copy comparison: {err:#}");
            }
        }
    }
}

/// Only viewport rows are laid out. The horizontal scroll owns both header
/// and body while TableBuilder scrolls only the body vertically.
fn draw_result_grid(
    ui: &mut egui::Ui,
    rows: &CompareOutput<'_>,
    colors: &ui::theme::ThemeColors,
) -> egui::scroll_area::ScrollAreaOutput<()> {
    let height = octa::ui::control_row::control_height(ui).max(20.0);
    ScrollArea::horizontal()
        .id_salt("compare_key_diff_horizontal")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let mut table = TableBuilder::new(ui)
                .id_salt("compare_key_diff_table")
                .striped(true)
                .resizable(true)
                .auto_shrink([false, false])
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center));
            for column in 0..rows.col_count() {
                let width = rows.header(column).chars().count().clamp(15, 31) as f32 * 8.0;
                table = table.column(Column::initial(width).at_least(64.0).clip(true));
            }
            table
                .header(height, |mut header| {
                    for c in 0..rows.col_count() {
                        header.col(|ui| {
                            result_label(ui, rows.header(c), colors.text_secondary, true);
                        });
                    }
                })
                .body(|body| {
                    body.rows(height, rows.row_count(), |mut slot| {
                        let Some(row) = rows.row(slot.index()) else {
                            return;
                        };
                        for c in 0..rows.col_count() {
                            slot.col(|ui| {
                                let text = row.text(c);
                                let color = match (c, row.status()) {
                                    (0, "only_in_a" | "changed_a") => colors.warning,
                                    (0, "only_in_b" | "changed_b") => colors.accent,
                                    _ => colors.text_primary,
                                };
                                result_label(ui, &text, color, c == 0);
                            });
                        }
                    });
                })
        })
        .inner
}

fn result_label(ui: &mut egui::Ui, text: &str, color: egui::Color32, strong: bool) {
    // A newline would make a Label taller than a virtual row. Keep the
    // unmodified value for the tooltip and clipboard instead of the preview.
    let preview = if text.contains(['\t', '\r', '\n']) {
        std::borrow::Cow::Owned(text.replace(['\t', '\r', '\n'], " "))
    } else {
        std::borrow::Cow::Borrowed(text)
    };
    let mut label = RichText::new(preview.as_ref()).color(color);
    if strong {
        label = label.strong();
    }
    ui.add(egui::Label::new(label).truncate().selectable(false))
        .on_hover_ui(|ui| {
            ui.label(text);
        });
}

#[cfg(test)]
#[path = "key_diff_tests.rs"]
mod tests;
