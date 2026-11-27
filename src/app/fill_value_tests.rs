use super::*;
use egui::accesskit::{Action, ActionRequest, NodeId, Role, TreeId, TreeUpdate};
use octa::data::{CellValue as C, DataTable, SearchResultMode};

fn fixture() -> OctaApp {
    let mut app = OctaApp::new(
        vec![],
        Default::default(),
        octa::ui::settings::IconVariant::Rose,
    );
    app.search_result_mode = SearchResultMode::Filter;
    app.tabs[0].table = DataTable {
        columns: [("ID", "Utf8"), ("Code", "Utf8"), ("Units", "Int64")]
            .map(|(name, ty)| ColumnInfo {
                name: name.into(),
                data_type: ty.into(),
            })
            .to_vec(),
        rows: vec![
            vec![C::String("visible A".into()), C::Null, C::Int(12)],
            vec![C::String("outside".into()), C::Null, C::Int(99)],
            vec![
                C::String("visible B".into()),
                C::String("".into()),
                C::Int(15),
            ],
            vec![
                C::String("visible C".into()),
                C::String("existing".into()),
                C::Int(18),
            ],
        ],
        ..DataTable::empty()
    };
    app.tabs[0].filtered_rows = vec![0, 1, 2, 3];
    app.tabs[0].filter_dirty = false;
    app.tabs[0].table_state.selected_cell = Some((0, 1));
    app
}

fn state(app: &OctaApp, text: &str) -> ValueFill {
    let mut state = ValueFill::start(&app.tabs[0], None).unwrap();
    state.input = text.to_owned();
    state.plan = state.prepare();
    state
}

#[test]
fn fill_from_blank_selection_survives_xlsx_and_csv_and_undoes_as_one_step() {
    use octa::formats::{FormatReader, csv_reader::CsvReader, excel_reader::ExcelReader};
    let mut app = fixture();
    let tab = &mut app.tabs[0];
    tab.search_text = "visible".into();
    tab.search_scope_col = Some(0);
    tab.hidden_columns.insert(2);
    tab.table_state.selected_cols = [1, 2].into();
    app.recompute_filter();
    app.select_visible_blanks();
    assert_eq!(
        app.tabs[0].table_state.selected_cells,
        [(0, 1), (2, 1)].into()
    );
    let original = app.tabs[0].table.rows.clone();
    let st = state(&app, "001");
    assert_eq!(st.plan.as_ref().unwrap().changed, 2);
    assert!(app.tabs[0].table.edits.is_empty());
    assert_eq!(app.apply_value_fill(&st), Ok(2));
    let tab = &mut app.tabs[0];
    assert_eq!(tab.table.undo_stack.len(), 1);
    let dir = tempfile::tempdir().unwrap();
    let xlsx = dir.path().join("filled.xlsx");
    let csv = dir.path().join("filled.csv");
    ExcelReader.write_file(&xlsx, &tab.table).unwrap();
    CsvReader.write_file(&csv, &tab.table).unwrap();
    let reopened = ExcelReader.read_file(&xlsx).unwrap();
    for row in 0..4 {
        let actual = tab.table.get(row, 1).unwrap();
        assert_eq!(
            actual.to_string(),
            match row {
                0 | 2 => "001",
                1 => "",
                _ => "existing",
            }
        );
        match row {
            0 | 2 => assert_eq!(reopened.get(row, 1), Some(&C::String("001".into()))),
            1 => assert_eq!(reopened.get(row, 1), Some(&C::Null)),
            _ => assert_eq!(reopened.get(row, 1), Some(&C::String("existing".into()))),
        }
        assert_eq!(tab.table.get(row, 2), Some(&original[row][2]));
    }
    let text = std::fs::read_to_string(csv).unwrap();
    assert!(
        text.lines().any(|line| line == "visible A,001,12"),
        "{text}"
    );
    assert!(text.lines().any(|line| line == "outside,,99"), "{text}");
    assert!(tab.table.undo());
    assert_eq!(tab.table.get(0, 1), Some(&C::Null));
    assert_eq!(tab.table.get(2, 1), Some(&C::String("".into())));
}

#[test]
fn mixed_columns_validate_atomically_even_beyond_the_three_preview_samples() {
    let mut app = fixture();
    let st = &mut app.tabs[0].table_state;
    st.selected_cells = [(0, 1), (1, 1), (2, 1), (3, 2)].into();
    let draft = state(&app, "SKU-001");
    assert_eq!(draft.plan.as_ref().unwrap_err(), &Failure::BadColumn(2));
    assert_eq!(app.apply_value_fill(&draft), Err(Failure::BadColumn(2)));
    assert!(app.tabs[0].table.edits.is_empty());
    let draft = state(&app, "001");
    assert_eq!(draft.plan.as_ref().unwrap().samples.len(), 3);
    assert_eq!(app.apply_value_fill(&draft), Ok(4));
    assert_eq!(app.tabs[0].table.get(3, 2), Some(&C::Int(1)));
    assert_eq!(app.tabs[0].table.get(0, 1), Some(&C::String("001".into())));
}

#[test]
fn type_checks_keep_text_integer_precision_date_and_decimal_storage() {
    assert_eq!(
        value_for_type(" =1+1 ", "Utf8"),
        Some(C::String(" =1+1 ".into()))
    );
    assert_eq!(
        value_for_type("9007199254740993", "Int64"),
        Some(C::Int(9007199254740993))
    );
    assert_eq!(value_for_type("256", "UInt8"), None);
    assert_eq!(value_for_type("3,25", "Float64"), Some(C::Float(3.25)));
    assert_eq!(value_for_type("NaN", "Float64"), None);
    assert_eq!(
        value_for_type("2024-02-29", "Date32"),
        Some(C::Date("2024-02-29".into()))
    );
    assert_eq!(value_for_type("2023-02-29", "Date32"), None);
    assert_eq!(value_for_type("false", "Boolean"), Some(C::Bool(false)));
    assert_eq!(value_for_type("not false", "Boolean"), None);
    assert_eq!(
        value_for_type("12:30:00.000001", "Time64(Microsecond)"),
        Some(C::Time(45_000_000_001_000))
    );
    assert_eq!(value_for_type("12:30:00.000001", "Time32(Second)"), None);
    let decimal = value_for_type("123456789012345678.02", "Decimal128(22, 2)").unwrap();
    assert_eq!(decimal.to_string(), "123456789012345678.02");
    assert_eq!(value_for_type("012", "Binary"), None);
}

#[test]
fn stale_filter_readonly_and_equal_stamp_mutation_fail_before_any_write() {
    let mut app = fixture();
    app.tabs[0].table_state.selected_cols = [1].into();
    let draft = state(&app, "filed");
    app.tabs[0].hidden_columns.insert(1);
    assert_eq!(app.apply_value_fill(&draft), Err(Failure::Stale));
    app.tabs[0].hidden_columns.clear();
    app.tabs[0].filtered_rows.remove(1);
    assert_eq!(app.apply_value_fill(&draft), Err(Failure::Stale));
    app.tabs[0].filtered_rows.insert(1, 1);
    app.readonly_mode = true;
    assert_eq!(app.apply_value_fill(&draft), Err(Failure::Stale));
    app.readonly_mode = false;
    app.tabs[0].table.rows[2][1] = C::String("new value".into());
    assert!(draft.source_matches(&app.tabs[0]));
    assert_eq!(app.apply_value_fill(&draft), Err(Failure::Stale));
    assert!(app.tabs[0].table.undo_stack.is_empty());
    assert!(app.tabs[0].table.edits.is_empty());
}

#[test]
fn selection_scope_never_falls_back_to_rows_or_partly_applies_large_inputs() {
    let mut app = fixture();
    let tab = &mut app.tabs[0];
    tab.table_state.selected_rows = [0].into();
    assert_eq!(visible_targets(tab), Err(Failure::Selection));
    tab.table_state.selected_rows.clear();
    tab.table_state.selected_cells = [(0, 1), (1, 1)].into();
    tab.table.hidden_rows.insert(1);
    assert_eq!(visible_targets(tab), Ok(vec![(0, 1)]));
    tab.table_state.selected_cells.clear();
    tab.table_state.selected_cell = Some((3, 1));
    assert_eq!(app.apply_value_fill(&state(&app, "")), Err(Failure::Empty));
    assert_eq!(
        app.apply_value_fill(&state(&app, "existing")),
        Err(Failure::NoChange)
    );
    assert_eq!(
        app.apply_value_fill(&state(&app, &"x".repeat(MAX_INPUT + 1))),
        Err(Failure::TooLarge)
    );
    assert!(app.tabs[0].table.undo_stack.is_empty());
    let tab = &mut app.tabs[0];
    tab.table.rows = vec![vec![C::Null; 3]; MAX_CELLS + 1];
    tab.filtered_rows = (0..tab.table.row_count()).collect();
    tab.table_state.selected_cols = [1].into();
    // One row is still hidden: exactly 50,000 visible cells fit.
    assert_eq!(visible_targets(tab).unwrap().len(), MAX_CELLS);
    tab.table.hidden_rows.clear();
    assert!(matches!(visible_targets(tab), Err(Failure::TooLarge)));
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
                egui::vec2(800.0, 600.0),
            )),
            events,
            ..Default::default()
        },
        |ui| {
            app.render_value_fill(ui.ctx());
        },
    );
    out.textures_delta.clear();
    fn collect(shape: &egui::Shape, text: &mut Vec<String>) {
        match shape {
            egui::Shape::Text(s) => text.push(s.galley.text().to_owned()),
            egui::Shape::Vec(xs) => xs.iter().for_each(|x| collect(x, text)),
            _ => (),
        }
    }
    let mut labels = Vec::new();
    for x in &out.shapes {
        collect(&x.shape, &mut labels);
    }
    (
        out.platform_output
            .accesskit_update
            .expect("AccessKit tree"),
        labels,
    )
}

fn key(key: egui::Key) -> egui::Event {
    key_with(key, egui::Modifiers::NONE)
}

fn key_with(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

fn focus(target: NodeId) -> egui::Event {
    egui::Event::AccessKitActionRequest(ActionRequest {
        action: Action::Focus,
        target_node: target,
        target_tree: TreeId::ROOT,
        data: None,
    })
}

#[test]
fn keyboard_requires_focused_confirmation_and_consumes_the_commit_key() {
    let mut app = fixture();
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    app.tabs[0].table_state.selected_cells = [(0, 1), (2, 1)].into();
    app.open_value_fill(&ctx);
    frame(&mut app, &ctx, vec![]);
    let (tree, _) = frame(&mut app, &ctx, vec![]);
    let input = tree
        .nodes
        .iter()
        .find(|(_, n)| n.role() == Role::MultilineTextInput)
        .unwrap()
        .0;
    assert_eq!(tree.focus, input);
    let (_, labels) = frame(&mut app, &ctx, vec![egui::Event::Text("filed".into())]);
    assert!(
        labels
            .iter()
            .any(|s| s.contains(&t("fill.value_will_change").replace("{count}", "2"))),
        "{labels:?}"
    );
    frame(&mut app, &ctx, vec![key(egui::Key::Enter)]);
    assert!(app.value_fill.is_some() && app.tabs[0].table.edits.is_empty());
    let (tree, _) = frame(&mut app, &ctx, vec![]);
    let (apply, n) = tree
        .nodes
        .iter()
        .find(|(_, n)| n.role() == Role::Button && n.label() == Some(&t("fill.fill")))
        .unwrap();
    assert!(!n.is_disabled());
    assert_eq!(frame(&mut app, &ctx, vec![focus(*apply)]).0.focus, *apply);
    frame(&mut app, &ctx, vec![key(egui::Key::Enter)]);
    assert!(app.value_fill.is_none());
    assert!(!ctx.input(|i| i.key_pressed(egui::Key::Enter)));
    assert_eq!(app.tabs[0].table.undo_stack.len(), 1);
    assert_eq!(
        app.tabs[0].table.get(2, 1),
        Some(&C::String("filed".into()))
    );
}

#[test]
fn escape_discards_the_preview_without_editing() {
    let mut app = fixture();
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    app.open_value_fill(&ctx);
    frame(&mut app, &ctx, vec![]);
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::Text("cancel this".into())],
    );
    frame(&mut app, &ctx, vec![key(egui::Key::Escape)]);
    assert!(app.value_fill.is_none());
    assert!(!ctx.input(|i| i.key_pressed(egui::Key::Escape)));
    assert!(app.tabs[0].table.undo_stack.is_empty());
}

#[test]
fn pasted_multiline_value_is_previewed_written_and_reopened_without_flattening() {
    use octa::formats::{FormatReader, csv_reader::CsvReader, excel_reader::ExcelReader};

    let mut app = fixture();
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let tab = &mut app.tabs[0];
    tab.search_text = "visible".into();
    tab.search_scope_col = Some(0);
    tab.table_state.selected_cells = [(0, 1), (1, 1), (2, 1)].into();
    app.recompute_filter();
    app.open_value_fill(&ctx);
    frame(&mut app, &ctx, vec![]);
    let literal = "Status: bereit\nQuelle\tWest";
    let (tree, labels) = frame(&mut app, &ctx, vec![egui::Event::Paste(literal.into())]);
    assert_eq!(app.value_fill.as_ref().unwrap().input, literal);
    assert!(
        labels
            .iter()
            .any(|s| s.contains(r"Status: bereit\nQuelle\tWest")),
        "{labels:?}"
    );
    assert!(app.tabs[0].table.edits.is_empty());
    let (apply, button) = tree
        .nodes
        .iter()
        .find(|(_, n)| n.role() == Role::Button && n.label() == Some(&t("fill.fill")))
        .unwrap();
    assert!(!button.is_disabled());
    frame(&mut app, &ctx, vec![focus(*apply)]);
    frame(&mut app, &ctx, vec![key(egui::Key::Enter)]);
    assert!(app.value_fill.is_none());
    let table = &mut app.tabs[0].table;
    assert_eq!(table.undo_stack.len(), 1);
    let dir = tempfile::tempdir().unwrap();
    let xlsx = dir.path().join("notes.xlsx");
    let csv = dir.path().join("notes.csv");
    ExcelReader.write_file(&xlsx, table).unwrap();
    CsvReader.write_file(&csv, table).unwrap();
    let sheet = ExcelReader.read_file(&xlsx).unwrap();
    let mut reader = csv::Reader::from_path(csv).unwrap();
    let csv_rows = reader.records().collect::<Result<Vec<_>, _>>().unwrap();
    assert_eq!(csv_rows.len(), 4);
    let text = C::String(literal.into());
    for row in [0, 2] {
        assert_eq!(table.get(row, 1), Some(&text));
        assert_eq!(sheet.get(row, 1), Some(&text));
        assert_eq!(csv_rows[row].get(1), Some(literal));
    }
    assert_eq!(table.get(1, 1), Some(&C::Null));
    assert_eq!(sheet.get(1, 1), Some(&C::Null));
    assert_eq!(csv_rows[1].get(1), Some(""));
    assert!(table.undo());
    assert_eq!(table.get(0, 1), Some(&C::Null));
    assert_eq!(table.get(2, 1), Some(&C::String(String::new())));
}

#[test]
fn byte_limit_keeps_the_whole_valid_value_and_disarms_the_old_preview_after_rejection() {
    let mut app = fixture();
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    app.open_value_fill(&ctx);
    frame(&mut app, &ctx, vec![]);
    let exact = "ä".repeat(MAX_INPUT / 2);
    frame(&mut app, &ctx, vec![egui::Event::Paste(exact.clone())]);
    let state = app.value_fill.as_ref().unwrap();
    assert_eq!(state.input, exact);
    assert_eq!(state.plan.as_ref().unwrap().changed, 1);

    for attempted in [
        egui::Event::Text("é".into()), // the result of appending would be too large
        egui::Event::Paste("w".repeat(MAX_INPUT + 1)),
    ] {
        let (tree, labels) = frame(&mut app, &ctx, vec![attempted]);
        let state = app.value_fill.as_ref().unwrap();
        assert_eq!(state.input, exact);
        assert_eq!(state.plan.as_ref().unwrap_err(), &Failure::TooLarge);
        assert!(labels.iter().any(|s| s.contains(&t("fill.value_large"))));
        assert!(tree.nodes.iter().any(|(_, n)| {
            n.role() == Role::Button && n.label() == Some(&t("fill.fill")) && n.is_disabled()
        }));
        let state = app.value_fill.take().unwrap();
        assert_eq!(app.apply_value_fill(&state), Err(Failure::TooLarge));
        app.value_fill = Some(state);
        assert!(app.tabs[0].table.edits.is_empty());
    }

    frame(
        &mut app,
        &ctx,
        vec![
            key_with(egui::Key::A, egui::Modifiers::COMMAND),
            egui::Event::Paste("short\r\nnote\t!".into()),
        ],
    );
    let state = app.value_fill.as_ref().unwrap();
    assert_eq!(state.input, "short\r\nnote\t!");
    assert_eq!(state.plan.as_ref().unwrap().changed, 1);
    assert!(app.tabs[0].table.edits.is_empty());
}

#[test]
fn shift_enter_adds_a_line_and_tab_moves_focus_without_confirming() {
    let mut app = fixture();
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    app.open_value_fill(&ctx);
    frame(&mut app, &ctx, vec![]);
    let (tree, _) = frame(&mut app, &ctx, vec![]);
    let field = tree.focus;
    let (tree, _) = frame(
        &mut app,
        &ctx,
        vec![
            egui::Event::Text("Alpha".into()),
            key_with(egui::Key::Enter, egui::Modifiers::SHIFT),
            egui::Event::Text("Beta".into()),
            key(egui::Key::Enter),
        ],
    );
    assert_eq!(app.value_fill.as_ref().unwrap().input, "Alpha\nBeta");
    assert_eq!(tree.focus, field);
    let (tree, _) = frame(&mut app, &ctx, vec![key(egui::Key::Tab)]);
    assert_ne!(tree.focus, field);
    assert!(tree.nodes.iter().any(|(id, n)| {
        *id == tree.focus && n.role() == Role::Button && n.label() == Some(&t("common.cancel"))
    }));
    assert_eq!(app.value_fill.as_ref().unwrap().input, "Alpha\nBeta");
    assert!(app.tabs[0].table.undo_stack.is_empty());
}
