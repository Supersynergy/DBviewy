//! The collapsible, Excel-style editing bar under the menu row: quick actions
//! on top, name box + formula bar below.
//!
//! Nothing here is a second implementation. Every button writes the same
//! [`ToolbarAction`] flag the matching menu entry writes, so the menus, the
//! shortcuts and the ribbon cannot drift apart. The formula bar commits through
//! [`crate::ui::table_view::commit_cell_text`], the function an in-cell edit
//! ends in, so `=` formulas, time and binary cells behave identically.

use egui::{RichText, Ui};

use crate::data::{BinaryDisplayMode, MarkColor, MarkKey};
use crate::i18n::t;
use crate::ui::icons::{Icon, icon_button};
use crate::ui::theme::ThemeColors;

use super::types::{ToolbarAction, ToolbarCtx};

/// Height the ribbon claims below the menu row: two 24 px rows, their spacing
/// and the separator line.
pub const RIBBON_HEIGHT: f32 = 58.0;

/// Spreadsheet column name for a 0-based index: 0 -> `A`, 26 -> `AA`.
pub(super) fn column_letters(mut col: usize) -> String {
    let mut out = Vec::new();
    loop {
        out.push(b'A' + (col % 26) as u8);
        if col < 26 {
            break;
        }
        col = col / 26 - 1;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// `B12` -> `(row 11, col 1)`, both 0-based. `None` for anything else.
pub(super) fn parse_cell_ref(text: &str) -> Option<(usize, usize)> {
    let text = text.trim();
    let split = text.find(|c: char| !c.is_ascii_alphabetic())?;
    let (letters, digits) = text.split_at(split);
    if letters.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let mut col = 0usize;
    for b in letters.bytes() {
        col = col
            .checked_mul(26)?
            .checked_add((b.to_ascii_uppercase() - b'A') as usize + 1)?;
    }
    let row: usize = digits.parse().ok()?;
    Some((row.checked_sub(1)?, col - 1))
}

pub(super) fn draw(ui: &mut Ui, cx: ToolbarCtx<'_>, action: &mut ToolbarAction) {
    let ToolbarCtx {
        colors,
        has_data,
        selected_cell,
        can_undo,
        can_redo,
        edits_blocked,
        table,
        zoom_percent,
        ..
    } = cx;
    let can_edit = has_data && !edits_blocked;

    ui.spacing_mut().interact_size.y = 24.0;
    ui.spacing_mut().item_spacing.x = 4.0;

    // Row 1: quick actions.
    ui.horizontal(|ui| {
        if icon_button(ui, can_undo && can_edit, Icon::Undo, t("edit_menu.undo_hint")) {
            action.undo = true;
        }
        if icon_button(ui, can_redo && can_edit, Icon::Redo, t("edit_menu.redo_hint")) {
            action.redo = true;
        }
        ui.separator();
        if icon_button(ui, has_data && selected_cell.is_some(), Icon::Copy, t("header.copy")) {
            action.copy_cells = true;
        }
        if icon_button(
            ui,
            can_edit && selected_cell.is_some(),
            Icon::Clipboard,
            t("header.paste"),
        ) {
            action.paste_cells = true;
        }
        ui.separator();
        if icon_button(ui, can_edit, Icon::Rows, t("edit_menu.insert_row_hint")) {
            action.add_row = true;
        }
        if icon_button(
            ui,
            can_edit && selected_cell.is_some(),
            Icon::Trash,
            t("edit_menu.delete_row_hint"),
        ) {
            action.delete_row = true;
        }
        if icon_button(ui, can_edit, Icon::Columns, t("toolbar.insert_column_hint")) {
            action.add_column = true;
        }
        if icon_button(
            ui,
            can_edit && selected_cell.is_some(),
            Icon::Trash,
            t("toolbar.delete_column_hint"),
        ) {
            action.delete_column = true;
        }
        ui.separator();
        let sort_col = selected_cell.map(|(_, c)| c);
        if icon_button(
            ui,
            has_data && sort_col.is_some(),
            Icon::ArrowUp,
            t("edit_menu.sort_asc_hint"),
        ) {
            action.sort_rows_asc_by = sort_col;
        }
        if icon_button(
            ui,
            has_data && sort_col.is_some(),
            Icon::ArrowDown,
            t("edit_menu.sort_desc_hint"),
        ) {
            action.sort_rows_desc_by = sort_col;
        }
        if icon_button(ui, has_data, Icon::Filter, t("status_bar.filter_hint")) {
            action.show_column_filter = Some(None);
        }
        if icon_button(ui, has_data, Icon::Search, t("search_menu.find_replace")) {
            action.toggle_replace_bar = true;
        }
        ui.separator();
        if icon_button(
            ui,
            can_edit && sort_col.is_some(),
            Icon::Hash,
            t("edit_menu.number_format_hint"),
        ) {
            action.open_column_format = true;
        }
        if icon_button(ui, has_data, Icon::Fit, t("edit_menu.fit_all_columns_hint")) {
            action.fit_all_columns = true;
        }
        mark_menu(ui, cx, can_edit, action);

        // Zoom sits at the right edge, where Excel keeps it.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icon_button(ui, true, Icon::ZoomIn, t("view_menu.zoom")) {
                action.zoom_in = true;
            }
            if ui
                .small_button(format!("{zoom_percent}%"))
                .on_hover_text(t("view_menu.zoom_reset_hint"))
                .clicked()
            {
                action.zoom_reset = true;
            }
            if icon_button(ui, true, Icon::ZoomOut, t("view_menu.zoom")) {
                action.zoom_out = true;
            }
        });
    });

    // Row 2: name box, apply / cancel, formula bar.
    ui.horizontal(|ui| {
        let formula_id = egui::Id::new("dbviewy_ribbon_formula");
        let name_id = egui::Id::new("dbviewy_ribbon_namebox");

        // Name box: shows `B12`; typing another reference and Enter jumps there.
        let current_ref = selected_cell.map(|(r, c)| format!("{}{}", column_letters(c), r + 1));
        let name_focused = ui.memory(|m| m.has_focus(name_id));
        let mut name_buf: String = if name_focused {
            ui.data(|d| d.get_temp::<String>(name_id)).unwrap_or_default()
        } else {
            current_ref.clone().unwrap_or_default()
        };
        let name = ui
            .add_enabled(
                has_data,
                egui::TextEdit::singleline(&mut name_buf)
                    .id(name_id)
                    .desired_width(64.0)
                    .font(egui::TextStyle::Monospace),
            )
            .on_hover_text(t("ribbon.cell_hint"));
        ui.data_mut(|d| d.insert_temp(name_id, name_buf.clone()));
        if name.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            if let Some((r, c)) = parse_cell_ref(&name_buf)
                && r < table.row_count()
                && c < table.col_count()
            {
                action.goto_cell = Some((r, c));
            }
        }

        // Formula bar text: the selected cell as the in-cell editor shows it.
        let current_text = selected_cell
            .and_then(|(r, c)| table.get(r, c))
            .map(|v| v.display_with_binary_mode(BinaryDisplayMode::default()))
            .unwrap_or_default();
        let is_binary = selected_cell
            .and_then(|(r, c)| table.get(r, c))
            .is_some_and(|v| matches!(v, crate::data::CellValue::Binary(_)));
        let usable = can_edit && selected_cell.is_some() && !is_binary;
        let focused = ui.memory(|m| m.has_focus(formula_id));
        let mut buf: String = if focused {
            ui.data(|d| d.get_temp::<String>(formula_id))
                .unwrap_or_else(|| current_text.clone())
        } else {
            current_text.clone()
        };

        let apply = ui
            .add_enabled(usable && buf != current_text, egui::Button::new("OK"))
            .on_hover_text(t("common.apply"))
            .clicked();
        let cancel = ui
            .add_enabled(usable && focused, egui::Button::new("x"))
            .on_hover_text(t("common.cancel"))
            .clicked();
        ui.label(RichText::new("fx").italics().color(colors.text_secondary));

        let edit = ui
            .add_enabled(
                usable,
                egui::TextEdit::singleline(&mut buf)
                    .id(formula_id)
                    .desired_width(ui.available_width().max(60.0))
                    .font(egui::TextStyle::Monospace),
            )
            .on_hover_text(t("ribbon.formula_hint"));
        ui.data_mut(|d| d.insert_temp(formula_id, buf.clone()));

        let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let escape = edit.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape));
        if (enter || apply)
            && usable
            && buf != current_text
            && let Some((r, c)) = selected_cell
        {
            action.commit_cell_text = Some((r, c, buf));
        }
        if escape || cancel {
            ui.data_mut(|d| d.remove::<String>(formula_id));
            edit.surrender_focus();
        }
    });
}

/// Colour the selection, like Excel's fill colour. Same marks as **Edit ->
/// Mark** and the right-click menu.
fn mark_menu(ui: &mut Ui, cx: ToolbarCtx<'_>, can_edit: bool, action: &mut ToolbarAction) {
    let ToolbarCtx {
        selected_cell,
        selected_rows,
        selected_cols,
        selected_cells,
        table,
        ..
    } = cx;
    let keys: Vec<MarkKey> = if !selected_rows.is_empty() {
        let mut rs: Vec<usize> = selected_rows.iter().copied().collect();
        rs.sort();
        rs.into_iter().map(MarkKey::Row).collect()
    } else if !selected_cols.is_empty() {
        let mut cs: Vec<usize> = selected_cols.iter().copied().collect();
        cs.sort();
        cs.into_iter().map(MarkKey::Column).collect()
    } else if !selected_cells.is_empty() {
        let mut cs: Vec<(usize, usize)> = selected_cells.iter().copied().collect();
        cs.sort();
        cs.into_iter().map(|(r, c)| MarkKey::Cell(r, c)).collect()
    } else if let Some((r, c)) = selected_cell {
        vec![MarkKey::Cell(r, c)]
    } else {
        Vec::new()
    };
    let any_marked = keys.iter().any(|k| table.marks.contains_key(k));
    ui.add_enabled_ui(can_edit && !keys.is_empty(), |ui| {
        ui.menu_button(t("edit_menu.mark"), |ui| {
            for &color in MarkColor::ALL {
                let swatch = ThemeColors::mark_swatch(color);
                if ui
                    .add(egui::Button::new(RichText::new(color.label_t()).color(swatch)))
                    .clicked()
                {
                    for k in &keys {
                        action.set_marks.push((k.clone(), color));
                    }
                    ui.close();
                }
            }
            ui.separator();
            if ui
                .add_enabled(any_marked, egui::Button::new(t("edit_menu.clear")))
                .clicked()
            {
                for k in &keys {
                    action.clear_marks.push(k.clone());
                }
                ui.close();
            }
        })
        .response
        .on_hover_text(t("edit_menu.mark_hint"));
    });
}
