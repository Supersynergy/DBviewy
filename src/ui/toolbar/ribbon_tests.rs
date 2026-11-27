use egui::{Event, Id, Key, Modifiers, PointerButton, Pos2, Rect, Vec2};

use crate::data::{CellValue, ColumnInfo, DataTable};

use super::ribbon::{column_letters, draw_formula_editor, loaded_cell_ref, parse_cell_ref};
use super::types::ToolbarAction;

#[test]
fn column_letters_follow_spreadsheet_naming() {
    assert_eq!(column_letters(0), "A");
    assert_eq!(column_letters(25), "Z");
    assert_eq!(column_letters(26), "AA");
    assert_eq!(column_letters(51), "AZ");
    assert_eq!(column_letters(52), "BA");
    assert_eq!(column_letters(701), "ZZ");
    assert_eq!(column_letters(702), "AAA");
}

#[test]
fn cell_reference_round_trips_with_column_letters() {
    assert_eq!(parse_cell_ref("A1"), Some((0, 0)));
    assert_eq!(parse_cell_ref(" b12 "), Some((11, 1)));
    for col in [0, 1, 25, 26, 51, 701, 702, 16383] {
        let text = format!("{}7", column_letters(col));
        assert_eq!(parse_cell_ref(&text), Some((6, col)), "{text}");
    }
}

#[test]
fn malformed_cell_references_are_rejected() {
    for bad in ["", "12", "A", "A0", "A-1", "1A", "A1B", "A 1", "$A$1"] {
        assert_eq!(parse_cell_ref(bad), None, "{bad:?}");
    }
}

fn grid() -> DataTable {
    DataTable {
        columns: ["units", "price"]
            .map(|name| ColumnInfo {
                name: name.into(),
                data_type: "Int64".into(),
            })
            .to_vec(),
        rows: vec![vec![CellValue::Int(10); 2]; 2],
        ..DataTable::empty()
    }
}

#[test]
fn name_box_maps_only_the_visible_source_row_numbers() {
    let mut table = grid();
    table.row_offset = 100;
    assert_eq!(loaded_cell_ref("A101", &table), Some((0, 0)));
    assert_eq!(loaded_cell_ref(" b102 ", &table), Some((1, 1)));
    for address in ["A100", "B103", "C101", "A1"] {
        assert_eq!(loaded_cell_ref(address, &table), None, "{address}");
    }
}

fn key(key: Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

fn editor_frame(
    ctx: &egui::Context,
    table: &DataTable,
    target: (usize, usize),
    editable: bool,
    editor: Id,
    events: Vec<Event>,
) -> (ToolbarAction, (Rect, Rect)) {
    let mut action = ToolbarAction::default();
    let mut buttons = (Rect::NOTHING, Rect::NOTHING);
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 300.0))),
            events,
            ..Default::default()
        },
        |ui| {
            crate::ui::control_row::control_line(ui, |ui| {
                buttons =
                    draw_formula_editor(ui, table, Some(target), editable, editor, &mut action);
            });
        },
    );
    output.textures_delta.clear();
    (action, buttons)
}

fn draft(ctx: &egui::Context, table: &DataTable, editor: Id) -> (Rect, Rect) {
    editor_frame(ctx, table, (0, 0), true, editor, vec![]);
    ctx.memory_mut(|m| m.request_focus(editor));
    let (action, buttons) = editor_frame(
        ctx,
        table,
        (0, 0),
        true,
        editor,
        vec![
            key(Key::A, Modifiers::COMMAND),
            Event::Paste("=A1*2".into()),
        ],
    );
    assert!(action.commit_cell_text.is_none());
    buttons
}

#[test]
fn formula_draft_survives_pointer_down_and_commits_on_apply_release() {
    let ctx = egui::Context::default();
    let table = grid();
    let editor = Id::new("tab 1");
    let point = draft(&ctx, &table, editor).0.center();
    let (moved, _) = editor_frame(
        &ctx,
        &table,
        (0, 0),
        true,
        editor,
        vec![Event::PointerMoved(point)],
    );
    assert!(moved.commit_cell_text.is_none());
    let pointer = |pressed| Event::PointerButton {
        pos: point,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    let (down, _) = editor_frame(&ctx, &table, (0, 0), true, editor, vec![pointer(true)]);
    assert!(down.commit_cell_text.is_none());
    let (up, _) = editor_frame(&ctx, &table, (0, 0), true, editor, vec![pointer(false)]);
    assert_eq!(up.commit_cell_text, Some((0, 0, "=A1*2".into())));
    let (after, _) = editor_frame(&ctx, &table, (0, 0), true, editor, vec![]);
    assert!(after.commit_cell_text.is_none());
}

#[test]
fn formula_enter_applies_once_and_does_not_reach_the_grid() {
    let ctx = egui::Context::default();
    let table = grid();
    let editor = Id::new("tab 1");
    draft(&ctx, &table, editor);
    let (action, _) = editor_frame(
        &ctx,
        &table,
        (0, 0),
        true,
        editor,
        vec![key(Key::Enter, Modifiers::NONE)],
    );
    assert_eq!(action.commit_cell_text, Some((0, 0, "=A1*2".into())));
    assert!(!ctx.input(|i| i.key_pressed(Key::Enter)));
}

#[test]
fn formula_escape_discards_draft_and_returns_focus() {
    let ctx = egui::Context::default();
    let table = grid();
    let editor = Id::new("tab 1");
    draft(&ctx, &table, editor);
    let (action, _) = editor_frame(
        &ctx,
        &table,
        (0, 0),
        true,
        editor,
        vec![key(Key::Escape, Modifiers::NONE)],
    );
    assert!(action.commit_cell_text.is_none());
    assert!(!ctx.memory(|m| m.has_focus(editor)));
    ctx.memory_mut(|m| m.request_focus(editor));
    let (action, _) = editor_frame(
        &ctx,
        &table,
        (0, 0),
        true,
        editor,
        vec![key(Key::Enter, Modifiers::NONE)],
    );
    assert!(action.commit_cell_text.is_none());
}

#[test]
fn ribbon_draft_cannot_cross_cell_tab_reload_or_readonly_boundaries() {
    let editor = Id::new("tab 1");
    for (scope, cell, editable, change_base) in [
        (editor, (0, 1), true, false),
        (Id::new("tab 2"), (0, 0), true, false),
        (editor, (0, 0), true, true),
        (editor, (0, 0), false, false),
    ] {
        let ctx = egui::Context::default();
        let mut table = grid();
        draft(&ctx, &table, editor);
        if change_base {
            table.set(0, 0, CellValue::Int(50));
        }
        let (action, _) = editor_frame(
            &ctx,
            &table,
            cell,
            editable,
            scope,
            vec![key(Key::Enter, Modifiers::NONE)],
        );
        assert!(action.commit_cell_text.is_none(), "{scope:?}, {cell:?}");
        ctx.memory_mut(|m| m.request_focus(editor));
        let (later, _) = editor_frame(
            &ctx,
            &table,
            (0, 0),
            true,
            editor,
            vec![key(Key::Enter, Modifiers::NONE)],
        );
        assert!(later.commit_cell_text.is_none(), "discarded draft returned");
    }
}

#[test]
fn formula_draft_cannot_move_to_an_equal_valued_column_after_reorder() {
    let ctx = egui::Context::default();
    let mut table = grid();
    let editor = Id::new("tab 1");
    draft(&ctx, &table, editor);
    table.columns.swap(0, 1);
    for row in &mut table.rows {
        row.swap(0, 1);
    }
    assert_eq!(table.get(0, 0), Some(&CellValue::Int(10)));
    let (action, _) = editor_frame(
        &ctx,
        &table,
        (0, 0),
        true,
        editor,
        vec![key(Key::Enter, Modifiers::NONE)],
    );
    assert!(action.commit_cell_text.is_none());
}
