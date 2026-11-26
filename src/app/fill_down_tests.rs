use eframe::egui;
use octa::data::{CellValue, ViewMode};
use octa::formats::{FormatReader, excel_reader::ExcelReader};
use octa::ui::settings::{AppSettings, IconVariant};

use super::OctaApp;

fn app(rows: usize, cols: usize) -> OctaApp {
    let mut app = OctaApp::new(vec![], AppSettings::default(), IconVariant::Rose);
    app.open_new_table_tab(cols, rows);
    app.recompute_filter();
    app
}

#[test]
fn copies_typed_sources_only_to_visible_selected_cells_and_undoes_once() {
    use CellValue::{Duration, Int, Null, String as Text, Time};
    let mut app = app(6, 4);
    let tab = &mut app.tabs[app.active_tab];
    let start = Time(6 * 3_600_000_000_000);
    let hours = Duration(27 * 3_600_000_000_000 + 500_000_000);
    tab.table.rows = vec![
        vec![Int(0), Int(0), Int(0), Int(0)],
        vec![start.clone(), hours.clone(), Null, Text("folded".into())],
        vec![Int(2), Int(2), Text("filtered".into()), Int(2)],
        vec![Int(3), Int(3), Text("gap".into()), Int(3)],
        vec![Int(4), hours.clone(), Text("clear me".into()), Int(4)],
        vec![Int(5), Int(5), Text("keep me".into()), Int(5)],
    ];
    tab.table.hide_rows([2]);
    tab.filter_dirty = true;
    tab.hidden_columns.insert(3);
    tab.table_state.selected_cells.extend([
        (1, 0),
        (2, 0),
        (4, 0),
        (5, 0),
        (1, 1),
        (2, 1),
        (4, 1),
        (1, 2),
        (2, 2),
        (4, 2),
        (1, 3),
        (4, 3),
        (999, 0),
        (1, 999),
    ]);
    let before = tab.table.rows.clone();
    app.fill_down();
    let tab = &mut app.tabs[app.active_tab];
    let table = &mut tab.table;
    assert_eq!(table.edits.len(), 3);
    assert_eq!(table.get(4, 0), Some(&start));
    assert_eq!(table.get(5, 0), Some(&start));
    assert_eq!(table.get(4, 1), Some(&hours));
    assert_eq!(table.get(4, 2), Some(&Null));
    for (r, c) in [(2, 0), (2, 1), (2, 2), (3, 0), (4, 3), (5, 2)] {
        assert_eq!(table.get(r, c), Some(&before[r][c]));
    }
    assert_eq!(table.undo_stack.len(), 1);
    table.undo();
    for (r, row) in before.iter().enumerate() {
        for (c, value) in row.iter().enumerate() {
            assert_eq!(table.get(r, c), Some(value));
        }
    }
    table.redo();
    assert_eq!(table.get(4, 2), Some(&Null));
    assert_eq!(table.get(5, 0), Some(&start));
    assert_eq!(tab.filtered_rows, vec![0, 1, 3, 4, 5]);
}

#[test]
fn column_headers_fill_in_display_order_and_leave_filtered_rows_untouched() {
    let mut app = app(5, 2);
    let tab = &mut app.tabs[app.active_tab];
    for (r, row) in tab.table.rows.iter_mut().enumerate() {
        row[1] = CellValue::Int(r as i64);
    }
    tab.filtered_rows = vec![4, 0, 2];
    tab.table_state.selected_cols.insert(1);
    app.fill_down();
    let table = &app.tabs[app.active_tab].table;
    let numbers = (0..5)
        .map(|r| table.get(r, 1).unwrap().clone())
        .collect::<Vec<_>>();
    assert_eq!(numbers, [4, 1, 4, 3, 4].map(CellValue::Int));
    assert_eq!(table.undo_stack.len(), 1);
}

#[test]
fn rejects_unavailable_scopes_and_leaves_undo_empty_for_identical_values() {
    let mut app = app(3, 2);
    let tab = &mut app.tabs[app.active_tab];
    tab.table_state.selected_rows.extend([0, 1]);
    assert!(tab.fill_down_block().is_some());
    tab.table_state.selected_rows.clear();
    tab.table_state.selected_cells.extend([(0, 0), (1, 1)]);
    assert!(
        tab.fill_down_block().is_some(),
        "two cells in different columns"
    );
    tab.table_state.selected_cells.insert((1, 0));
    assert!(tab.fill_down_block().is_none());
    app.fill_down();
    let tab = &mut app.tabs[app.active_tab];
    assert!(!tab.filter_dirty);
    assert!(tab.table.edits.is_empty());
    assert!(tab.table.undo_stack.is_empty());
    tab.table.rows[0][0] = CellValue::Int(42);
    tab.view_mode = ViewMode::Raw;
    app.fill_down();
    let tab = &mut app.tabs[app.active_tab];
    assert!(tab.table.edits.is_empty());
    tab.view_mode = ViewMode::Table;
    app.readonly_mode = true;
    app.fill_down();
    assert!(app.tabs[app.active_tab].table.edits.is_empty());
}

#[test]
fn filled_clocks_and_elapsed_values_survive_export_to_real_xlsx() {
    let mut app = app(2, 2);
    let tab = &mut app.tabs[app.active_tab];
    let clock = CellValue::Time(22 * 3_600_000_000_000 + 45 * 60_000_000_000);
    let elapsed = CellValue::Duration(27 * 3_600_000_000_000 + 500_000_000);
    tab.table.rows[0] = vec![clock.clone(), elapsed.clone()];
    tab.table_state.selected_cols.extend([0, 1]);
    app.fill_down();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fill-down.xlsx");
    ExcelReader
        .write_file(&path, &app.tabs[app.active_tab].table)
        .unwrap();
    let output = ExcelReader.read_file(&path).unwrap();
    for row in 0..2 {
        assert_eq!(output.get(row, 0), Some(&clock));
        assert_eq!(output.get(row, 1), Some(&elapsed));
    }
}

#[test]
fn edit_menu_dispatches_the_fill_without_needing_a_clipboard() {
    fn frame(
        app: &mut OctaApp,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> Vec<(String, egui::Pos2)> {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1300.0, 900.0),
            )),
            events,
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| app.render_toolbar(ui));
        out.textures_delta.clear();
        fn read(shape: &egui::Shape, o: &mut Vec<(String, egui::Pos2)>) {
            match shape {
                egui::Shape::Text(t) => o.push((
                    t.galley.text().to_owned(),
                    t.visual_bounding_rect().center(),
                )),
                egui::Shape::Vec(s) => s.iter().for_each(|s| read(s, o)),
                _ => {}
            }
        }
        let mut items = Vec::new();
        for s in &out.shapes {
            read(&s.shape, &mut items);
        }
        items
    }

    fn click(app: &mut OctaApp, ctx: &egui::Context, at: egui::Pos2) {
        frame(app, ctx, vec![egui::Event::PointerMoved(at)]);
        for pressed in [true, false] {
            frame(
                app,
                ctx,
                vec![egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
        }
    }

    let mut app = app(3, 1);
    let ctx = egui::Context::default();
    let tab = &mut app.tabs[app.active_tab];
    let source = CellValue::String("=literal text\nsecond line".into());
    tab.table.rows[0][0] = source.clone();
    tab.table_state.selected_cells.extend([(0, 0), (2, 0)]);
    let position = |items: &[(String, egui::Pos2)], key| {
        let label = octa::i18n::t(key);
        items
            .iter()
            .find(|(name, _)| name == &label)
            .unwrap_or_else(|| panic!("{label:?} not in {items:?}"))
            .1
    };
    frame(&mut app, &ctx, vec![]);
    let items = frame(&mut app, &ctx, vec![]);
    click(&mut app, &ctx, position(&items, "menu.edit"));
    frame(&mut app, &ctx, vec![]);
    let items = frame(&mut app, &ctx, vec![]);
    click(&mut app, &ctx, position(&items, "edit_menu.fill_down"));
    let tab = &app.tabs[app.active_tab];
    assert_eq!(tab.table.get(1, 0), Some(&CellValue::Null));
    assert_eq!(tab.table.get(2, 0), Some(&source));
    assert_eq!(tab.table.undo_stack.len(), 1);
}
