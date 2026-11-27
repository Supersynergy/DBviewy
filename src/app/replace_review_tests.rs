use super::*;
use octa::data::{ColumnInfo, DataTable};

fn s(value: &str) -> CellValue {
    CellValue::String(value.into())
}

fn grid() -> OctaApp {
    let mut app = OctaApp::new(
        vec![],
        octa::ui::settings::AppSettings::default(),
        octa::ui::settings::IconVariant::Rose,
    );
    let tab = &mut app.tabs[0];
    tab.table = DataTable {
        columns: ["Item", "Notes", "Private code"]
            .into_iter()
            .map(|name| ColumnInfo {
                name: name.into(),
                data_type: "Utf8".into(),
            })
            .collect(),
        rows: vec![
            vec![s("A1"), s("a1 then A1"), s("A1")],
            vec![s("A1"), s("North"), s("A1")],
            vec![s("A1"), s("West"), s("A1")],
            vec![s("A1"), s("Central"), s("A1")],
        ],
        row_offset: 100,
        ..DataTable::empty()
    };
    tab.table_state.selected_cell = Some((0, 0));
    tab.filtered_rows = vec![0, 1, 2];
    tab.table.hidden_rows.insert(2);
    tab.hidden_columns.insert(2);
    tab.filter_dirty = false;
    tab.find_box = Some(FindInput {
        query: "A1".into(),
        replace: "B2".into(),
        replacing: true,
        ..Default::default()
    });
    app
}

fn review(app: &mut OctaApp) -> ReplaceReview {
    app.replace_all_matches();
    app.replace_review.take().expect("table opens a review")
}

#[test]
fn review_visible_rows_does_not_write_before_confirmation_and_undos_as_group() {
    let mut app = grid();
    let mut plan = review(&mut app);
    let table = &app.tabs[0].table;
    assert!(table.edits.is_empty() && table.undo_stack.is_empty());
    assert_eq!(
        (plan.edits.len(), plan.changed_rows, plan.changed_cols),
        (3, 2, 2)
    );
    assert_eq!(plan.examples[0].target, "101 / Item");
    assert_eq!(plan.examples[1].before, "\"a1 then A1\"");
    assert_eq!(plan.examples[1].after, "\"B2 then B2\"");
    assert!(app.commit_replace_review(&mut plan));
    let tab = &mut app.tabs[0];
    assert_eq!(tab.table.get(0, 1), Some(&s("B2 then B2")));
    for (r, c) in [(0, 2), (2, 0), (3, 0)] {
        assert_eq!(tab.table.get(r, c), Some(&s("A1")));
    }
    assert_eq!(tab.table.undo_stack.len(), 1);
    assert!(tab.filter_dirty);
    assert_eq!(tab.table_state.selected_cell, Some((0, 0)));
    assert!(tab.table.undo());
    assert_eq!(tab.table.get(0, 0), Some(&s("A1")));
    assert_eq!(tab.table.get(0, 1), Some(&s("a1 then A1")));
    assert_eq!(tab.table.get(1, 0), Some(&s("A1")));
}

#[test]
fn review_regex_groups_and_literal_dollars_match_bulk_replace() {
    let mut app = grid();
    {
        let tab = &mut app.tabs[0];
        tab.find_box.as_mut().unwrap().replace = "$1".into();
    }
    let mut plan = review(&mut app);
    assert_eq!(plan.edits[1].new, s("$1 then $1"));
    assert!(app.commit_replace_review(&mut plan));
    assert_eq!(app.tabs[0].table.get(0, 0), Some(&s("$1")));
    let tab = &mut app.tabs[0];
    assert!(tab.table.undo());
    tab.filter_dirty = false;
    // The undo writes through the edit overlay, which takes precedence over
    // the backing rows. Set the next typed fixture through that same layer.
    tab.table.set(1, 0, CellValue::Int(120));
    let input = tab.find_box.as_mut().unwrap();
    input.regex = true;
    input.query = "^(1)(20)$".into();
    input.replace = "$2".into();
    let mut plan = review(&mut app);
    assert_eq!(plan.edits.len(), 1);
    assert_eq!(plan.edits[0].old, CellValue::Int(120));
    assert_eq!(plan.edits[0].new, CellValue::Int(20));
    assert!(app.commit_replace_review(&mut plan));
    let table = &mut app.tabs[0].table;
    assert_eq!(table.get(1, 0), Some(&CellValue::Int(20)));
    assert!(table.undo());
    assert_eq!(table.get(1, 0), Some(&CellValue::Int(120)));
}

#[test]
fn review_same_visible_value_keeps_numeric_type_and_has_no_apply() {
    let mut app = grid();
    let tab = &mut app.tabs[0];
    tab.table.rows[0][0] = CellValue::Float(5.0);
    let input = tab.find_box.as_mut().unwrap();
    input.query = "5".into();
    input.replace = "5".into();
    let mut plan = review(&mut app);
    assert!(plan.edits.is_empty());
    assert_eq!((plan.changed_cols, plan.changed_rows), (0, 0));
    assert!(!app.commit_replace_review(&mut plan));
    assert_eq!(app.tabs[0].table.get(0, 0), Some(&CellValue::Float(5.0)));
    assert!(app.tabs[0].table.undo_stack.is_empty());
}

#[test]
fn review_refuses_new_hidden_columns_and_active_filters_without_partial_write() {
    let mut app = grid();
    let mut plan = review(&mut app);
    app.tabs[0].hidden_columns.insert(1);
    assert!(!app.commit_replace_review(&mut plan));
    assert!(app.tabs[0].table.edits.is_empty());
    app.tabs[0].hidden_columns.remove(&1);
    // Real filter path, not just a flag: restrict the visible rows to West.
    app.search_result_mode = octa::data::SearchResultMode::Filter;
    app.tabs[0].search_text = "West".into();
    app.tabs[0].filter_dirty = true;
    assert!(!app.commit_replace_review(&mut plan));
    assert!(app.tabs[0].filtered_rows.is_empty());
    assert!(app.tabs[0].table.undo_stack.is_empty());
}

#[test]
fn review_refuses_in_place_source_mutations_and_new_matches_with_same_stamp() {
    let mut app = grid();
    let mut plan = review(&mut app);
    let stamp = DataStamp::of(&app.tabs[0].table);
    app.tabs[0].table.rows[1][1] = s("A1");
    assert!(stamp == DataStamp::of(&app.tabs[0].table));
    assert!(!app.commit_replace_review(&mut plan), "newly matching cell");
    app.tabs[0].table.rows[1][1] = s("North");
    app.tabs[0].table.rows[0][0] = s("a1");
    assert!(stamp == DataStamp::of(&app.tabs[0].table));
    assert!(
        !app.commit_replace_review(&mut plan),
        "same result, different old value"
    );
    assert!(app.tabs[0].table.edits.is_empty());
}

#[test]
fn review_refuses_other_tab_changed_find_and_read_only_at_confirmation() {
    let mut app = grid();
    let mut plan = review(&mut app);
    app.readonly_mode = true;
    assert!(!app.commit_replace_review(&mut plan));
    app.readonly_mode = false;
    app.tabs[0].find_box.as_mut().unwrap().case_sensitive = true;
    assert!(!app.commit_replace_review(&mut plan));
    app.tabs[0].find_box.as_mut().unwrap().case_sensitive = false;
    app.open_new_table_tab(3, 4);
    assert_ne!(app.active_tab, 0);
    assert!(!app.commit_replace_review(&mut plan));
    assert!(app.tabs.iter().all(|t| t.table.edits.is_empty()));
}

fn frame(app: &mut OctaApp, ctx: &egui::Context, events: Vec<egui::Event>) -> egui::FullOutput {
    let mut output = ctx.run_ui(
        egui::RawInput {
            events,
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(380.0, 620.0),
            )),
            ..Default::default()
        },
        |ui| {
            octa::ui::shortcuts::begin_frame();
            app.handle_shortcuts(ui.ctx());
            app.render_central_panel(ui);
            app.render_replace_review(ui.ctx());
        },
    );
    output.textures_delta.clear();
    output
}

fn position(output: &egui::FullOutput, label: &str) -> egui::Pos2 {
    output
        .shapes
        .iter()
        .find_map(|item| match &item.shape {
            egui::Shape::Text(text)
                if text.galley.text() == label
                    && item.clip_rect.intersects(text.visual_bounding_rect()) =>
            {
                Some(text.visual_bounding_rect().center())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing visible label {label:?}"))
}

fn click(app: &mut OctaApp, ctx: &egui::Context, pos: egui::Pos2) {
    frame(app, ctx, vec![egui::Event::PointerMoved(pos)]);
    for pressed in [true, false] {
        frame(
            app,
            ctx,
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }],
        );
    }
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

#[test]
fn review_keyboard_escape_and_apply_return_focus_without_leaking_editor_keys() {
    use egui::accesskit::{Action, ActionRequest, Role, TreeId};
    let mut app = grid();
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    app.tabs[0].search_nav.match_count = 3;
    let base = egui::Id::new(("tab_find_box", app.tabs[0].id));
    let replace_field = find_box::replace_field_id(base);
    let start = |app: &mut OctaApp| {
        for _ in 0..3 {
            frame(app, &ctx, vec![]);
        }
        let drawn = frame(app, &ctx, vec![]);
        let trigger = position(&drawn, &t("search.replace_review_open"));
        click(app, &ctx, trigger);
        assert!(app.replace_review.is_some());
        assert!(app.tabs[0].table.edits.is_empty());
    };
    start(&mut app);
    frame(&mut app, &ctx, vec![]);
    // A character typed in the modal does not start editing the table.
    frame(&mut app, &ctx, vec![egui::Event::Text("Z".into())]);
    assert!(app.tabs[0].table_state.editing_cell.is_none());
    frame(&mut app, &ctx, vec![key(egui::Key::Escape)]);
    for _ in 0..4 {
        frame(&mut app, &ctx, vec![]);
    }
    assert!(app.replace_review.is_none());
    assert!(app.tabs[0].table.undo_stack.is_empty());
    assert!(ctx.memory(|m| m.has_focus(replace_field)));

    ctx.disable_accesskit();
    ctx.enable_accesskit();
    start(&mut app);
    let frames = (0..3)
        .map(|_| frame(&mut app, &ctx, vec![]))
        .collect::<Vec<_>>();
    let label = t("common.apply");
    let apply = frames
        .iter()
        .rev()
        .filter_map(|f| f.platform_output.accesskit_update.as_ref())
        .flat_map(|tree| &tree.nodes)
        .find(|(_, node)| {
            node.role() == Role::Button
                && node.label() == Some(label.as_str())
                && !node.is_disabled()
                && node.supports_action(Action::Focus)
        })
        .map(|(id, _)| *id)
        .expect("named focusable Apply button");
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::AccessKitActionRequest(ActionRequest {
            action: Action::Focus,
            target_tree: TreeId::ROOT,
            target_node: apply,
            data: None,
        })],
    );
    frame(&mut app, &ctx, vec![key(egui::Key::Enter)]);
    for _ in 0..4 {
        frame(&mut app, &ctx, vec![]);
    }
    assert!(app.replace_review.is_none());
    assert_eq!(app.tabs[0].table.get(0, 1), Some(&s("B2 then B2")));
    assert_eq!(app.tabs[0].table.undo_stack.len(), 1);
    assert!(app.tabs[0].table_state.editing_cell.is_none());
    assert!(ctx.memory(|m| m.has_focus(replace_field)));
}

#[test]
fn raw_text_replace_all_still_replaces_without_creating_a_grid_review() {
    let mut app = grid();
    let tab = &mut app.tabs[0];
    tab.view_mode = ViewMode::Raw;
    tab.raw_content = Some("A1\nA1".into());
    app.replace_all_matches();
    let tab = &app.tabs[0];
    assert_eq!(tab.raw_content.as_deref(), Some("B2\nB2"));
    assert!(tab.raw_content_modified);
    assert!(tab.table.undo_stack.is_empty() && app.replace_review.is_none());
}
