use eframe::egui;
use octa::data::{CellValue, ViewMode};
use octa::formats::{FormatReader, excel_reader::ExcelReader};
use octa::ui::settings::{AppSettings, IconVariant};

use super::{Direction, OctaApp};

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
    assert!(tab.fill_block(Direction::Down).is_some());
    tab.table_state.selected_rows.clear();
    tab.table_state.selected_cells.extend([(0, 0), (1, 1)]);
    assert!(
        tab.fill_block(Direction::Down).is_some(),
        "two cells in different columns"
    );
    tab.table_state.selected_cells.insert((1, 0));
    assert!(tab.fill_block(Direction::Down).is_none());
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

    let position = |items: &[(String, egui::Pos2)], key| {
        let label = octa::i18n::t(key);
        items
            .iter()
            .find(|(name, _)| name == &label)
            .unwrap_or_else(|| panic!("{label:?} not in {items:?}"))
            .1
    };
    for (rows, cols, gap, end, command) in [
        (3, 1, (1, 0), (2, 0), "edit_menu.fill_down"),
        (1, 3, (0, 1), (0, 2), "edit_menu.fill_right"),
    ] {
        let mut app = app(rows, cols);
        let ctx = egui::Context::default();
        let tab = &mut app.tabs[app.active_tab];
        let source = CellValue::String("=literal text\nsecond line".into());
        tab.table.rows[0][0] = source.clone();
        tab.table_state.selected_cells.extend([(0, 0), end]);
        frame(&mut app, &ctx, vec![]);
        let items = frame(&mut app, &ctx, vec![]);
        click(&mut app, &ctx, position(&items, "menu.edit"));
        frame(&mut app, &ctx, vec![]);
        let items = frame(&mut app, &ctx, vec![]);
        click(&mut app, &ctx, position(&items, command));
        let tab = &app.tabs[app.active_tab];
        assert_eq!(
            tab.table.get(gap.0, gap.1),
            Some(&CellValue::Null),
            "{command}"
        );
        assert_eq!(tab.table.get(end.0, end.1), Some(&source), "{command}");
        assert_eq!(tab.table.undo_stack.len(), 1, "{command}");
    }
}

#[test]
fn fill_right_uses_each_rows_leftmost_visible_source_and_keeps_gaps() {
    use CellValue::{Duration, Int, Null, String as Text};
    let mut app = app(4, 6);
    let tab = &mut app.tabs[app.active_tab];
    let elapsed = Duration(27 * 3_600_000_000_000 + 500_000_000);
    tab.table.rows = vec![
        vec![
            Int(90),
            elapsed.clone(),
            Text("gap".into()),
            Int(60),
            Int(4),
            Int(5),
        ],
        vec![Int(91), Null, Int(1), Int(2), Text("clear".into()), Int(5)],
        vec![Int(92), Int(7), Int(2), Int(3), Int(4), Int(5)],
        vec![Int(93), Int(8), Int(2), Int(3), Int(4), Int(5)],
    ];
    tab.hidden_columns.extend([0, 3]);
    tab.table.hide_rows([2]);
    tab.filter_dirty = true;
    tab.table_state.selected_cells.extend([
        (0, 5),
        (0, 4),
        (0, 3),
        (0, 1),
        (0, 0),
        (1, 4),
        (1, 1),
        (1, 999),
        (2, 5),
        (2, 1),
        (3, 0),
        (3, 4),
        (999, 3),
    ]);
    let before = tab.table.rows.clone();
    let changes = [(0, 4, elapsed.clone()), (0, 5, elapsed), (1, 4, Null)];
    app.fill_right();
    let tab = &mut app.tabs[app.active_tab];
    assert_eq!(tab.table.edits.len(), 3);
    for (r, previous) in before.iter().enumerate() {
        for (c, value) in previous.iter().enumerate() {
            let expected = changes
                .iter()
                .find_map(|(a, b, v)| ((*a, *b) == (r, c)).then_some(v))
                .unwrap_or(value);
            assert_eq!(tab.table.get(r, c), Some(expected), "{r}:{c}");
        }
    }
    assert_eq!(tab.table.undo_stack.len(), 1);
    assert!(tab.table.undo());
    for (r, row) in before.iter().enumerate() {
        for (c, value) in row.iter().enumerate() {
            assert_eq!(tab.table.get(r, c), Some(value), "{r}:{c}");
        }
    }
    assert!(tab.table.redo());
    assert_eq!(tab.table.get(1, 4), Some(&Null));
    assert_eq!(tab.filtered_rows, [0, 1, 3]);
}

#[test]
fn fill_right_on_column_headers_can_fill_one_visible_row() {
    use CellValue::{Bool, Int, Null};
    let mut app = app(3, 5);
    let tab = &mut app.tabs[app.active_tab];
    tab.filtered_rows = vec![1];
    tab.table.rows[0][4] = Int(99);
    tab.table.rows[1][0] = Int(100);
    tab.table.rows[1][1] = Bool(true);
    tab.table.rows[2][4] = Int(101);
    tab.hidden_columns.insert(0);
    tab.table_state.selected_cols.extend([4, 0, 1]);
    assert!(tab.fill_block(Direction::Right).is_none());
    assert!(tab.fill_block(Direction::Down).is_some());
    app.fill_right();
    let tab = &app.tabs[app.active_tab];
    assert_eq!(tab.table.get(1, 4), Some(&Bool(true)));
    assert_eq!(tab.table.get(1, 2), Some(&Null));
    assert_eq!(tab.table.get(1, 0), Some(&Int(100)));
    assert_eq!(tab.table.get(0, 4), Some(&Int(99)));
    assert_eq!(tab.table.get(2, 4), Some(&Int(101)));
    assert_eq!(tab.table.undo_stack.len(), 1);
}

#[test]
fn fill_right_does_not_expand_row_headers_or_edit_locked_views() {
    let mut app = app(3, 3);
    let tab = &mut app.tabs[app.active_tab];
    tab.table.rows[0][0] = CellValue::Int(42);
    tab.table_state.selected_rows.insert(0);
    assert_eq!(
        tab.fill_block(Direction::Right),
        Some("greyed.need_two_cells_in_row")
    );
    tab.table_state.selected_rows.clear();
    tab.table_state.selected_cells.extend([(0, 0), (1, 1)]);
    assert!(tab.fill_block(Direction::Right).is_some());
    tab.table_state.selected_cells.insert((0, 2));
    tab.table_state.editing_cell = Some((0, 0, "typing".into()));
    assert!(tab.fill_block(Direction::Right).is_some());
    app.fill_right();
    let tab = &mut app.tabs[app.active_tab];
    assert!(tab.table.edits.is_empty());
    tab.table_state.editing_cell = None;
    tab.view_mode = ViewMode::Raw;
    assert_eq!(
        tab.fill_block(Direction::Right),
        Some("greyed.need_table_view")
    );
    app.fill_right();
    app.tabs[app.active_tab].view_mode = ViewMode::Table;
    app.readonly_mode = true;
    app.fill_right();
    let table = &app.tabs[app.active_tab].table;
    assert!(table.edits.is_empty());
    assert!(table.undo_stack.is_empty());
}

#[test]
fn fill_right_preserves_typed_time_values_in_a_real_workbook() {
    let mut app = app(2, 3);
    let tab = &mut app.tabs[app.active_tab];
    let values = [
        CellValue::Time(22 * 3_600_000_000_000 + 45 * 60_000_000_000),
        CellValue::Duration(27 * 3_600_000_000_000 + 500_000_000),
    ];
    for (r, value) in values.iter().enumerate() {
        tab.table.rows[r][0] = value.clone();
    }
    tab.table_state.selected_cols.extend([0, 2]);
    app.fill_right();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fill-right.xlsx");
    ExcelReader
        .write_file(&path, &app.tabs[app.active_tab].table)
        .unwrap();
    use calamine::{Data, Reader, open_workbook_auto};
    let mut raw_book = open_workbook_auto(&path).unwrap();
    let raw = raw_book.worksheet_range_at(0).unwrap().unwrap();
    let result = ExcelReader.read_file(&path).unwrap();
    let table = &app.tabs[app.active_tab].table;
    for (r, value) in values.iter().enumerate() {
        let (nanos, is_duration) = match value {
            CellValue::Time(n) => (*n, false),
            CellValue::Duration(n) => (*n, true),
            _ => unreachable!(),
        };
        for c in [0, 2] {
            assert_eq!(table.get(r, c), Some(value), "typed fill at {r}:{c}");
            let Data::DateTime(serial) = raw[r + 1][c] else {
                panic!("expected an XLSX clock or elapsed format at {r}:{c}");
            };
            assert_eq!(serial.is_duration(), is_duration, "XLSX style at {r}:{c}");
            let exported_nanos =
                (serial.as_f64() * octa::data::times::NANOS_PER_DAY as f64).round() as i64;
            assert_eq!(exported_nanos, nanos, "XLSX value at {r}:{c}");
            // Reopening a column that mixes clocks and durations promotes
            // both to Duration. The stored cell styles above remain distinct.
            assert_eq!(result.get(r, c), Some(&CellValue::Duration(nanos)));
        }
        assert_eq!(result.get(r, 1), Some(&CellValue::Null));
    }
}
