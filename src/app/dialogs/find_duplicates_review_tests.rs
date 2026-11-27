use super::*;
use egui::accesskit::{Action, ActionRequest, NodeId, Role, TreeId, TreeUpdate};
use octa::data::cleanup::{CleanupKind, Severity, Suggestion};
use octa::data::{ColumnInfo, DataTable};

fn fixture() -> OctaApp {
    let mut app = OctaApp::new(
        vec![],
        Default::default(),
        octa::ui::settings::IconVariant::Rose,
    );
    let tab = &mut app.tabs[0];
    tab.table = DataTable {
        columns: [("Schlüssel", "Utf8"), ("ID", "Int64"), ("Notiz", "Utf8")]
            .map(|(name, ty)| ColumnInfo {
                name: name.into(),
                data_type: ty.into(),
            })
            .to_vec(),
        rows: ["A", "A", "A", "B", "B", "C"]
            .iter()
            .enumerate()
            .map(|(i, key)| {
                vec![
                    CellValue::String((*key).into()),
                    CellValue::Int(10 + i as i64),
                    CellValue::String(format!("Beleg {}\nNord\tOK", i + 1)),
                ]
            })
            .collect(),
        ..DataTable::empty()
    };
    tab.filtered_rows = vec![0, 2, 3, 5];
    tab.table.hidden_rows.insert(4);
    tab.filter_dirty = false;
    tab.table_state.selected_cell = Some((2, 2));
    app
}

fn ids(table: &DataTable) -> Vec<i64> {
    (0..table.row_count())
        .map(|row| match table.get(row, 1) {
            Some(CellValue::Int(n)) => *n,
            Some(CellValue::Float(n)) if n.fract() == 0.0 => *n as i64,
            other => panic!("invalid identifier at row {row}: {other:?}"),
        })
        .collect()
}

fn open(app: &mut OctaApp, keep: KeepWhich) -> DuplicateReview {
    assert!(app.open_duplicate_review(&[0], keep));
    app.duplicate_review.take().unwrap()
}

#[test]
fn preview_counts_hidden_rows_and_commits_exact_first_or_last_plan_as_one_undo() {
    use octa::formats::{FormatReader, excel_reader::ExcelReader};
    for (keep, want, hidden_count, retained) in [
        (KeepWhich::First, [4, 2, 1], 2, [10, 13, 15]),
        (KeepWhich::Last, [3, 1, 0], 1, [12, 14, 15]),
    ] {
        let mut app = fixture();
        let review = open(&mut app, keep);
        assert_eq!(ids(&app.tabs[0].table), [10, 11, 12, 13, 14, 15]);
        assert_eq!(review.rows, want);
        assert_eq!(review.hidden_count, hidden_count);
        assert_eq!(review.key_names, "Schlüssel");
        assert!(app.tabs[0].table.undo_stack.is_empty());
        assert!(app.commit_duplicate_review(&review));
        let tab = &mut app.tabs[0];
        assert_eq!(ids(&tab.table), retained);
        assert_eq!(tab.table.undo_stack.len(), 1);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("confirmed.xlsx");
        ExcelReader.write_file(&path, &tab.table).unwrap();
        assert_eq!(ids(&ExcelReader.read_file(&path).unwrap()), retained);
        assert!(tab.table.undo());
        assert_eq!(ids(&tab.table), [10, 11, 12, 13, 14, 15]);
    }
}

#[test]
fn approval_rejects_changed_non_key_cells_changed_survivors_visibility_and_other_tabs() {
    let mut changed = fixture();
    let review = open(&mut changed, KeepWhich::First);
    // A direct replacement of text of the same shape need not change DataStamp.
    changed.tabs[0].table.rows[1][2] = CellValue::String("geändert".into());
    assert!(review.current(&changed.tabs[0]));
    assert!(!changed.commit_duplicate_review(&review));
    assert_eq!(changed.tabs[0].table.row_count(), 6);

    let mut changed = fixture();
    let review = open(&mut changed, KeepWhich::First);
    changed.tabs[0].table.rows[0][0] = CellValue::String("new key".into());
    assert!(review.current(&changed.tabs[0]));
    assert!(!changed.commit_duplicate_review(&review));
    assert_eq!(changed.tabs[0].table.row_count(), 6);

    let mut changed = fixture();
    let review = open(&mut changed, KeepWhich::First);
    changed.tabs[0].filtered_rows = vec![0, 1, 3, 5];
    assert!(review.current(&changed.tabs[0]));
    assert!(!changed.commit_duplicate_review(&review));
    assert_eq!(changed.tabs[0].table.row_count(), 6);

    changed.open_new_table_tab(2, 4);
    assert!(!changed.commit_duplicate_review(&review));
    assert_eq!(changed.tabs[0].table.row_count(), 6);
    assert_eq!(changed.tabs[changed.active_tab].table.row_count(), 4);
}

#[test]
fn review_refuses_partial_read_only_and_unbounded_input_before_writing() {
    let mut app = fixture();
    app.tabs[0].table.total_rows = Some(10);
    assert!(!app.open_duplicate_review(&[0], KeepWhich::First));
    assert!(app.duplicate_review.is_none());
    app.tabs[0].table.total_rows = None;
    let review = open(&mut app, KeepWhich::First);
    app.readonly_mode = true;
    assert!(!app.commit_duplicate_review(&review));
    assert!(!app.open_duplicate_review(&[0], KeepWhich::First));
    app.readonly_mode = false;
    app.tabs[0].bg_can_load_more = true;
    assert!(!app.open_duplicate_review(&[0], KeepWhich::First));
    app.tabs[0].bg_can_load_more = false;
    app.tabs[0].table.rows[1][2] = CellValue::String("z".repeat(MAX_REVIEW_BYTES + 1));
    assert!(!app.open_duplicate_review(&[0], KeepWhich::First));
    assert_eq!(
        app.status_message.as_ref().unwrap().0,
        t("fill.value_large")
    );
    assert_eq!(app.tabs[0].table.row_count(), 6);
    assert!(app.tabs[0].table.undo_stack.is_empty());
}

fn frame(app: &mut OctaApp, ctx: &egui::Context, events: Vec<egui::Event>) -> TreeUpdate {
    let mut out = ctx.run_ui(
        egui::RawInput {
            events,
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            ..Default::default()
        },
        |ui| {
            octa::ui::shortcuts::begin_frame();
            app.handle_shortcuts(ui.ctx());
            app.render_central_panel(ui);
            super::super::render_find_duplicates_dialog(app, ui.ctx());
        },
    );
    out.textures_delta.clear();
    out.platform_output.accesskit_update.unwrap()
}

fn key(key: egui::Key) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

fn button(tree: &TreeUpdate, label: &str) -> NodeId {
    tree.nodes
        .iter()
        .find(|(_, node)| node.role() == Role::Button && node.label() == Some(label))
        .map(|(id, _)| *id)
        .unwrap_or_else(|| panic!("missing button {label}"))
}

fn focus(id: NodeId) -> egui::Event {
    egui::Event::AccessKitActionRequest(ActionRequest {
        action: Action::Focus,
        target_node: id,
        target_tree: TreeId::ROOT,
        data: None,
    })
}

#[test]
fn keyboard_review_starts_at_cancel_restores_picker_focus_and_drops_only_on_delete() {
    use crate::app::state::FindDuplicatesMode;
    let mut app = fixture();
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    app.tabs[0].find_duplicates_key_cols.insert(0);
    app.tabs[0].find_duplicates_mode = FindDuplicatesMode::Drop(KeepWhich::First);
    assert!(app.open_duplicate_review(&[0], KeepWhich::First));
    app.return_duplicate_review_to_picker();
    frame(&mut app, &ctx, vec![]);
    let tree = frame(&mut app, &ctx, vec![]);
    assert_eq!(tree.focus, button(&tree, &t("common.cancel")));
    frame(
        &mut app,
        &ctx,
        vec![key(egui::Key::Delete), egui::Event::Paste("no".into())],
    );
    assert!(app.duplicate_review.is_some());
    assert!(app.tabs[0].table.edits.is_empty());
    frame(&mut app, &ctx, vec![key(egui::Key::Escape)]);
    assert!(app.duplicate_review.is_none());
    assert!(app.tabs[0].show_find_duplicates);
    assert_eq!(app.tabs[0].table.row_count(), 6);

    frame(&mut app, &ctx, vec![]);
    let picker = frame(&mut app, &ctx, vec![]);
    assert_eq!(picker.focus, button(&picker, &t("dialog.pv_preview")));
    frame(&mut app, &ctx, vec![key(egui::Key::Enter)]);
    assert!(app.duplicate_review.is_some());
    frame(&mut app, &ctx, vec![]);
    let tree = frame(&mut app, &ctx, vec![]);
    assert_eq!(tree.focus, button(&tree, &t("common.cancel")));
    let delete = button(&tree, &t("common.delete"));
    assert_eq!(frame(&mut app, &ctx, vec![focus(delete)]).focus, delete);
    frame(&mut app, &ctx, vec![key(egui::Key::Enter)]);
    assert!(app.duplicate_review.is_none());
    assert!(!app.tabs[0].show_find_duplicates);
    assert_eq!(ids(&app.tabs[0].table), [10, 13, 15]);
    assert!(!ctx.input(|input| input.key_pressed(egui::Key::Enter)));
}

#[test]
fn cleanup_duplicate_suggestion_opens_the_same_review_without_deleting() {
    let mut app = fixture();
    app.tabs[0].table.rows[1] = app.tabs[0].table.rows[0].clone();
    let suggestion = Suggestion {
        kind: CleanupKind::DuplicateRows,
        column: None,
        affected: 1,
        severity: Severity::High,
        detail: String::new(),
        examples: vec![],
    };
    assert!(!app.apply_cleanup(suggestion));
    let plan = app.duplicate_review.take().unwrap();
    assert_eq!(plan.rows, [1]);
    assert_eq!(plan.hidden_count, 1);
    assert!(plan.keys.is_empty());
    assert_eq!(app.tabs[0].table.row_count(), 6);
    assert!(app.tabs[0].table.undo_stack.is_empty());
}

#[test]
fn unavailable_removal_keeps_the_chosen_mode_without_substituting_highlight() {
    use crate::app::state::FindDuplicatesMode;
    let mut app = fixture();
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let tab = &mut app.tabs[0];
    tab.table.total_rows = Some(100);
    tab.show_find_duplicates = true;
    tab.find_duplicates_key_cols.insert(0);
    tab.find_duplicates_mode = FindDuplicatesMode::Drop(KeepWhich::Last);
    frame(&mut app, &ctx, vec![]);
    let tree = frame(&mut app, &ctx, vec![]);
    let preview = button(&tree, &t("dialog.pv_preview"));
    assert!(
        tree.nodes
            .iter()
            .any(|(id, node)| *id == preview && node.is_disabled())
    );
    assert_eq!(
        app.tabs[0].find_duplicates_mode,
        FindDuplicatesMode::Drop(KeepWhich::Last)
    );
    assert!(app.duplicate_review.is_none());
    assert!(app.tabs[0].table.marks.is_empty());
}
