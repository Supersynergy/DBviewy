use super::*;
use egui::accesskit::{Action, ActionRequest, NodeId, Role, TreeId, TreeUpdate};
use octa::data::ColumnInfo;
use octa::ui::settings::{AppSettings, IconVariant};

fn fixture(rows: usize, cols: usize) -> OctaApp {
    let mut app = OctaApp::new(vec![], AppSettings::default(), IconVariant::Rose);
    let tab = &mut app.tabs[0];
    tab.custom_tab_label = Some("Lab results".into());
    tab.table = DataTable {
        columns: (0..cols)
            .map(|c| ColumnInfo {
                name: format!("Sample {}", c + 1),
                data_type: "Int64".into(),
            })
            .collect(),
        rows: vec![vec![CellValue::Null; cols]; rows],
        ..DataTable::empty()
    };
    tab.filtered_rows = (0..rows).collect();
    tab.filter_dirty = false;
    tab.table_state.selected_cols.extend(0..cols);
    app
}

fn open(app: &mut OctaApp) -> FillSeriesState {
    app.open_fill_series();
    app.fill_series_dialog.take().expect("valid fill target")
}

#[test]
fn fills_only_displayed_rows_and_columns_in_their_order_with_one_undo() {
    let mut app = fixture(10, 5);
    let tab = &mut app.tabs[0];
    tab.hidden_columns.insert(1);
    tab.filtered_rows = vec![9, 7, 5, 3, 1, 0];
    tab.table.rows[9][0] = CellValue::Int(100);
    tab.table.rows[7][0] = CellValue::Int(104);
    tab.table.rows[8][0] = CellValue::Int(925);
    let rows = tab.table.rows.clone();
    let st = open(&mut app);
    assert_eq!(st.target_count, 24);
    let preview = st.preview(&app.tabs[0].table).unwrap();
    assert_eq!(
        preview.iter().map(|(c, _)| *c).collect::<Vec<_>>(),
        [0, 2, 3]
    );
    for (_, values) in preview {
        assert_eq!(values, ["100", "104", "108", "112", "116"]);
    }
    assert_eq!(apply_fill(&mut app, &st), Ok(22));
    let tab = &mut app.tabs[0];
    assert_eq!(tab.table.get(0, 0), Some(&CellValue::Int(120)));
    assert_eq!(tab.table.get(8, 0), Some(&CellValue::Int(925)));
    assert_eq!(tab.table.get(5, 1), Some(&CellValue::Null));
    assert_eq!(tab.table.get(9, 4), Some(&CellValue::Int(100)));
    assert_eq!(tab.table.undo_stack.len(), 1);
    assert!(tab.filter_dirty);
    assert!(tab.table.undo());
    assert_eq!(tab.table.rows, rows);
    for (i, row) in rows.iter().enumerate() {
        for (j, value) in row.iter().enumerate() {
            assert_eq!(tab.table.get(i, j), Some(value));
        }
    }
}

#[test]
fn pattern_guess_preserves_more_than_five_seeds_and_samples_each_column() {
    let mut app = fixture(10, 2);
    let tab = &mut app.tabs[0];
    for (r, seed) in ["amber", "birch", "cedar", "dune", "elm", "field", "grove"]
        .into_iter()
        .enumerate()
    {
        for (c, suffix) in ["east", "west"].into_iter().enumerate() {
            tab.table.rows[r][c] = CellValue::String(format!("{seed} {suffix}"));
        }
    }
    for col in &mut tab.table.columns {
        col.data_type = "Utf8".into();
    }
    let st = open(&mut app);
    assert_eq!(st.spec.kind, FillKind::Repeat);
    assert_eq!(st.spec.pattern, 7);
    let preview = st.preview(&app.tabs[0].table).unwrap();
    assert_eq!(preview[0].1[4], "elm east");
    assert_eq!(preview[1].1[4], "elm west");
    assert_eq!(apply_fill(&mut app, &st), Ok(6));
    assert_eq!(
        app.tabs[0].table.get(7, 0),
        Some(&CellValue::String("amber east".into()))
    );
    assert_eq!(
        app.tabs[0].table.get(9, 1),
        Some(&CellValue::String("cedar west".into()))
    );
}

#[test]
fn pending_series_cannot_write_to_a_different_tab_or_changed_table() {
    let mut app = fixture(4, 2);
    let st = open(&mut app);
    app.open_new_table_tab(2, 4);
    let other = app.active_tab;
    assert_ne!(app.tabs[other].id, st.tab);
    assert_eq!(apply_fill(&mut app, &st), Err(ApplyError::ChangedTarget));
    assert!(app.tabs[other].table.edits.is_empty());
    app.active_tab = 0;
    app.tabs[0].table.set(0, 0, CellValue::Int(50));
    assert_eq!(apply_fill(&mut app, &st), Err(ApplyError::ChangedTarget));
    assert_eq!(app.tabs[0].table.edits.len(), 1);
    assert_eq!(app.tabs[0].table.get(0, 0), Some(&CellValue::Int(50)));
}

#[test]
fn pending_series_rechecks_selection_filter_order_and_readonly() {
    let mut app = fixture(4, 2);
    let st = open(&mut app);
    app.tabs[0].table_state.selected_cols.remove(&0);
    assert_eq!(apply_fill(&mut app, &st), Err(ApplyError::ChangedTarget));
    app.tabs[0].table_state.selected_cols.insert(0);
    app.tabs[0].hidden_columns.insert(1);
    assert_eq!(apply_fill(&mut app, &st), Err(ApplyError::ChangedTarget));
    app.tabs[0].hidden_columns.clear();
    app.tabs[0].filtered_rows.swap(0, 1);
    assert_eq!(apply_fill(&mut app, &st), Err(ApplyError::ChangedTarget));
    app.tabs[0].filtered_rows.swap(0, 1);
    app.readonly_mode = true;
    assert_eq!(apply_fill(&mut app, &st), Err(ApplyError::ChangedTarget));
    assert!(app.tabs[0].table.undo_stack.is_empty());
    app.open_fill_series();
    assert!(app.fill_series_dialog.is_none());
}

#[test]
fn overflow_beyond_the_bounded_sample_cannot_partly_write() {
    let mut app = fixture(7, 2);
    let mut st = open(&mut app);
    st.spec.start = "0".into();
    st.spec.step = "3e307".into();
    assert_eq!(st.preview(&app.tabs[0].table).unwrap()[0].1.len(), 5);
    assert_eq!(
        apply_fill(&mut app, &st),
        Err(ApplyError::Series(FillError::TooFar))
    );
    assert!(app.tabs[0].table.edits.is_empty());
    assert!(app.tabs[0].table.undo_stack.is_empty());
}

fn frame(
    app: &mut OctaApp,
    ctx: &egui::Context,
    events: Vec<egui::Event>,
) -> (TreeUpdate, Vec<String>) {
    let mut out = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(530., 530.),
            )),
            events,
            ..Default::default()
        },
        |ui| render_fill_series_dialog(app, ui.ctx()),
    );
    out.textures_delta.clear();
    let tree = out.platform_output.accesskit_update.take().unwrap();
    let paint = out
        .shapes
        .iter()
        .filter_map(|item| match &item.shape {
            egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
            _ => None,
        })
        .collect();
    (tree, paint)
}

fn focus(target_node: NodeId) -> egui::Event {
    egui::Event::AccessKitActionRequest(ActionRequest {
        action: Action::Focus,
        target_tree: TreeId::ROOT,
        target_node,
        data: None,
    })
}

fn key(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

#[test]
fn dialog_previews_the_typed_step_then_keyboard_fill_writes_once() {
    let mut app = fixture(4, 2);
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    app.open_fill_series();
    frame(&mut app, &ctx, vec![]);
    let (tree, paint) = frame(&mut app, &ctx, vec![]);
    assert!(
        paint.iter().any(|v| v.contains("Lab results")
            && v.contains(&app.fill_series_dialog.as_ref().unwrap().scope_label())),
        "{paint:?}"
    );
    assert!(
        paint.iter().any(|v| v == "Sample 2: 1, 2, 3, 4"),
        "{paint:?}"
    );
    let step_label = t("fill.step");
    let (step, _) = tree
        .nodes
        .iter()
        .find(|(_, node)| {
            node.role() == Role::TextInput
                && node.labelled_by().iter().any(|label_id| {
                    tree.nodes.iter().any(|(id, n)| {
                        id == label_id && n.value().or(n.label()) == Some(&step_label)
                    })
                })
        })
        .expect("text field labelled Step");
    let (focused, _) = frame(&mut app, &ctx, vec![focus(*step)]);
    assert_eq!(focused.focus, *step);
    let (_, paint) = frame(
        &mut app,
        &ctx,
        vec![
            key(egui::Key::A, egui::Modifiers::COMMAND),
            egui::Event::Text("3".into()),
        ],
    );
    assert!(
        paint.iter().any(|v| v == "Sample 2: 1, 4, 7, 10"),
        "{paint:?}"
    );
    assert!(app.tabs[0].table.edits.is_empty());

    let (tree, _) = frame(&mut app, &ctx, vec![]);
    let fill = t("fill.fill");
    let (apply, apply_node) = tree
        .nodes
        .iter()
        .find(|(_, node)| node.role() == Role::Button && node.label() == Some(&fill))
        .expect("Fill button");
    assert!(!apply_node.is_disabled());
    let (focused, _) = frame(&mut app, &ctx, vec![focus(*apply)]);
    assert_eq!(focused.focus, *apply);
    frame(
        &mut app,
        &ctx,
        vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
    );
    assert!(!ctx.input(|i| i.key_pressed(egui::Key::Enter)));
    assert!(app.fill_series_dialog.is_none());
    assert_eq!(app.tabs[0].table.get(3, 1), Some(&CellValue::Int(10)));
    assert_eq!(app.tabs[0].table.undo_stack.len(), 1);
}
