//! The collapsible, Excel-style editing bar under the menu row: quick actions
//! on top, name box + formula bar below.
//!
//! Nothing here is a second implementation. Every button writes the same
//! [`ToolbarAction`] flag the matching menu entry writes, so the menus, the
//! shortcuts and the ribbon cannot drift apart. The formula bar commits through
//! [`crate::ui::table_view::commit_cell_text`], the function an in-cell edit
//! ends in, so `=` formulas, time and binary cells behave identically.

use egui::{RichText, Ui};

use crate::data::{BinaryDisplayMode, CellValue, DataTable, MarkColor, MarkKey};
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

pub(super) fn loaded_cell_ref(text: &str, table: &DataTable) -> Option<(usize, usize)> {
    let (row, col) = parse_cell_ref(text)?;
    let local_row = row.checked_sub(table.row_offset)?;
    (local_row < table.row_count() && col < table.col_count()).then_some((local_row, col))
}

#[derive(Clone, PartialEq, Eq)]
struct FormulaTarget {
    editor: egui::Id,
    cell: (usize, usize),
    row_offset: usize,
    row_storage: usize,
    column_count: usize,
    column_name_storage: usize,
    value_type: std::mem::Discriminant<CellValue>,
    original: String,
}

#[derive(Clone)]
struct FormulaDraft {
    target: FormulaTarget,
    text: String,
}

fn formula_button(ui: &mut Ui, icon: Icon, label: &str, enabled: bool) -> egui::Response {
    let button = egui::Button::image(crate::ui::icons::icon_image(ui, icon)).min_size(
        egui::Vec2::splat(crate::ui::control_row::control_height(ui)),
    );
    let response = ui.add_enabled(enabled, button);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label));
    response.on_hover_text(label)
}

pub(super) fn draw_formula_editor(
    ui: &mut Ui,
    table: &DataTable,
    selected: Option<(usize, usize)>,
    can_edit: bool,
    editor: egui::Id,
    action: &mut ToolbarAction,
) -> (egui::Rect, egui::Rect) {
    // Only one formula draft for the window. Tab and cell changes drop it,
    // but focus can move to an action button without dropping the edit.
    let draft_id = egui::Id::new("dbviewy_ribbon_draft");
    let value = selected.and_then(|(row, col)| table.get(row, col));
    let usable = can_edit && value.is_some_and(|v| !matches!(v, CellValue::Binary(_)));
    let current = value
        .map(|v| v.display_with_binary_mode(BinaryDisplayMode::default()))
        .unwrap_or_default();
    let target = selected
        .zip(value)
        .filter(|_| usable)
        .and_then(|(cell, value)| {
            let column = table.columns.get(cell.1)?;
            Some(FormulaTarget {
                editor,
                cell,
                row_offset: table.row_offset,
                // Allocations move with their rows and column headings.
                // Changing either identity must discard the old draft even
                // if the coordinate and the visible value stay the same.
                row_storage: table.rows.get(cell.0)?.as_ptr() as usize,
                column_count: table.col_count(),
                column_name_storage: column.name.as_ptr() as usize,
                value_type: std::mem::discriminant(value),
                original: current.clone(),
            })
        });
    let saved = ui
        .data_mut(|d| d.remove_temp::<Option<FormulaDraft>>(draft_id))
        .flatten();
    let mut text = match saved {
        Some(draft) if Some(&draft.target) == target.as_ref() => draft.text,
        Some(draft) => {
            ui.memory_mut(|m| m.surrender_focus(draft.target.editor));
            current.clone()
        }
        None => current.clone(),
    };
    let focused = ui.memory(|m| m.has_focus(editor));
    let dirty = usable && text != current;
    let apply = formula_button(ui, Icon::Check, &t("common.apply"), dirty);
    let cancel = formula_button(
        ui,
        Icon::Close,
        &t("common.cancel"),
        usable && (dirty || focused),
    );
    ui.label(
        RichText::new("fx")
            .italics()
            .color(ui.visuals().weak_text_color()),
    );
    let edit = ui
        .add_enabled(
            usable,
            egui::TextEdit::singleline(&mut text)
                .id(editor)
                .desired_width(ui.available_width().max(40.0))
                .font(egui::TextStyle::Monospace),
        )
        .on_hover_text(t("ribbon.formula_hint"));
    let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    // Egui clears keyboard focus at the start of the Escape frame,
    // before drawing the former owner. Its lost-focus flag is still set.
    let owns_escape = focused
        || edit.has_focus()
        || edit.lost_focus()
        || (dirty && (apply.lost_focus() || cancel.lost_focus()));
    let escape =
        owns_escape && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    if escape || cancel.clicked() {
        edit.surrender_focus();
    } else if let Some(target) = target.filter(|_| text != current) {
        if enter || apply.clicked() {
            action.commit_cell_text = Some((target.cell.0, target.cell.1, text));
        } else {
            ui.data_mut(|d| d.insert_temp(draft_id, Some(FormulaDraft { target, text })));
        }
    }
    if enter {
        ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
    }
    (apply.rect, cancel.rect)
}

pub(super) fn draw(ui: &mut Ui, cx: ToolbarCtx<'_>, action: &mut ToolbarAction) {
    let ToolbarCtx {
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
        if icon_button(
            ui,
            can_undo && can_edit,
            Icon::Undo,
            t("edit_menu.undo_hint"),
        ) {
            action.undo = true;
        }
        if icon_button(
            ui,
            can_redo && can_edit,
            Icon::Redo,
            t("edit_menu.redo_hint"),
        ) {
            action.redo = true;
        }
        ui.separator();
        if icon_button(
            ui,
            has_data && selected_cell.is_some(),
            Icon::Copy,
            t("header.copy"),
        ) {
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
        crate::ui::table_zoom::ribbon_controls(ui, zoom_percent, action);
    });

    // Row 2: name box, apply / cancel, formula bar.
    let editor = ui.make_persistent_id("dbviewy_ribbon_formula");
    let name_id = ui.make_persistent_id("dbviewy_ribbon_namebox");
    crate::ui::control_row::control_line(ui, |ui| {
        // Name box: shows `B12`; typing another reference and Enter jumps there.
        let current_ref = selected_cell.and_then(|(r, c)| {
            let row = table.row_offset.checked_add(r)?.checked_add(1)?;
            Some(format!("{}{row}", column_letters(c)))
        });
        let name_focused = ui.memory(|m| m.has_focus(name_id));
        let mut name_buf: String = if name_focused {
            ui.data(|d| d.get_temp::<String>(name_id))
                .unwrap_or_default()
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
            action.goto_cell = loaded_cell_ref(&name_buf, table);
            ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
        }
        draw_formula_editor(ui, table, selected_cell, can_edit, editor, action);
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
                    .add(egui::Button::new(
                        RichText::new(color.label_t()).color(swatch),
                    ))
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
