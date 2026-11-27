use octa::ui::settings::{AppSettings, IconVariant};

use crate::app::state::OctaApp;

fn app(zoom_percent: u32) -> OctaApp {
    OctaApp::new(
        vec![],
        AppSettings {
            zoom_percent,
            ..AppSettings::default()
        },
        IconVariant::Rose,
    )
}

#[test]
fn octa_opens_at_the_zoom_it_was_left_at() {
    assert_eq!(app(150).zoom_percent, 150);
    assert_eq!(app(100).zoom_percent, 100);
    assert_eq!(app(9000).zoom_percent, 500, "a hand-edited settings.toml");
}

// `set_zoom` is `remember_zoom` plus a save, which no test may run: a save here
// writes the developer's real settings.toml. What it would write is checked below.
#[test]
fn a_new_zoom_is_what_the_next_start_reads() {
    let mut app = app(100);
    app.remember_zoom(130);
    assert_eq!(app.zoom_percent, 130);
    let written = toml::to_string(&app.settings).unwrap();
    let next: AppSettings = toml::from_str(&written).unwrap();
    assert_eq!(next.zoom_percent(), 130);

    app.remember_zoom(9000);
    assert_eq!(app.zoom_percent, 500);
    assert_eq!(app.settings.zoom_percent(), 500);
}

fn cell(app: &OctaApp, r: usize, c: usize) -> String {
    app.tabs[app.active_tab]
        .table
        .get(r, c)
        .map(|v| v.to_string())
        .unwrap_or_default()
}

fn shape(app: &OctaApp) -> (usize, usize) {
    let t = &app.tabs[app.active_tab].table;
    (t.row_count(), t.col_count())
}

/// File > New table on a fresh start used to leave the blank tab Octa starts
/// with beside the new one: two "Untitled" tabs.
#[test]
fn a_new_table_takes_the_blank_tab_octa_starts_with() {
    let mut app = app(100);
    assert_eq!(app.tabs.len(), 1);
    app.open_new_table_tab(3, 1);
    assert_eq!(app.tabs.len(), 1, "no stray Untitled beside it");
    assert_eq!(shape(&app), (1, 3));
    // The first one holds a table now, so a second goes beside it.
    app.open_new_table_tab(2, 2);
    assert_eq!(app.tabs.len(), 2);
    assert_eq!(app.active_tab, 1);
    assert_eq!(shape(&app), (2, 2));
}

#[test]
fn a_paste_bigger_than_the_table_grows_it_and_undoes_in_one_step() {
    let mut app = app(100);
    app.open_new_table_tab(3, 1);
    app.paste_text_into_table("a\tb\tc\td\te\n1\t2\t3\t4\t5\n6\t7\t8\t9\t10\n11\t12\t13\t14\t15\n");
    assert_eq!(shape(&app), (4, 5));
    let names: Vec<_> = app.tabs[0]
        .table
        .columns
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(names, ["col1", "col2", "col3", "col4", "col5"]);
    assert_eq!(cell(&app, 0, 4), "e");
    assert_eq!(cell(&app, 3, 4), "15");

    assert!(app.tabs[0].table.undo());
    assert_eq!(shape(&app), (1, 3), "one Ctrl+Z takes the whole paste back");
    assert_eq!(cell(&app, 0, 0), "");
}

#[test]
fn a_paste_smaller_than_the_table_leaves_the_rest_empty() {
    let mut app = app(100);
    app.open_new_table_tab(4, 3);
    app.paste_text_into_table("x\ty\n");
    assert_eq!(shape(&app), (3, 4));
    assert_eq!(
        (cell(&app, 0, 0), cell(&app, 0, 1)),
        ("x".into(), "y".into())
    );
    assert_eq!(
        (cell(&app, 0, 2), cell(&app, 1, 0)),
        (String::new(), String::new())
    );
}

/// Rows added at the end of what is loaded would land in the middle of the
/// table, and new columns on a database tab would be a schema change.
#[test]
fn a_paste_stops_at_the_edge_while_rows_are_still_coming_or_on_a_database_tab() {
    let block = "a\tb\tc\n1\t2\t3\n";
    let mut loading = app(100);
    loading.open_new_table_tab(2, 1);
    loading.tabs[0].bg_can_load_more = true;
    loading.paste_text_into_table(block);
    assert_eq!(shape(&loading), (1, 2));
    assert_eq!(cell(&loading, 0, 1), "b");

    let mut partial = app(100);
    partial.open_new_table_tab(2, 1);
    partial.tabs[0].table.total_rows = Some(50);
    partial.paste_text_into_table(block);
    assert_eq!(shape(&partial), (1, 2));

    let mut database = app(100);
    database.open_new_table_tab(2, 1);
    database.tabs[0].db_origin = Some(crate::app::state::DbOrigin {
        conn_id: "c".into(),
        catalog: None,
        schema: "s".into(),
        table: "t".into(),
        identity: None,
    });
    database.paste_text_into_table(block);
    assert_eq!(shape(&database), (1, 2));
}

fn clipboard_text(action: impl FnOnce(&eframe::egui::Context)) -> Option<String> {
    use eframe::egui::{Context, OutputCommand, RawInput};
    let mut action = Some(action);
    let ctx = Context::default();
    let mut out = ctx.run_ui(RawInput::default(), |ui| {
        if let Some(action) = action.take() {
            action(ui.ctx());
        }
    });
    out.textures_delta.clear();
    out.platform_output.commands.into_iter().find_map(|cmd| {
        if let OutputCommand::CopyText(text) = cmd {
            Some(text)
        } else {
            None
        }
    })
}

fn visible_clipboard_app() -> OctaApp {
    use octa::data::{CellValue, SearchResultMode};
    let mut app = app(100);
    app.open_new_table_tab(3, 5);
    app.search_result_mode = SearchResultMode::Filter;
    let tab = &mut app.tabs[app.active_tab];
    tab.table.rows = [
        ("VISIBLE-A", "PRIVATE-COLUMN-A", 10),
        ("FILTERED-ROW", "PRIVATE-COLUMN-B", 20),
        ("VISIBLE-C", "PRIVATE-COLUMN-C", 30),
        ("VISIBLE-MANUALLY-HIDDEN", "PRIVATE-COLUMN-D", 40),
        ("VISIBLE-E", "PRIVATE-COLUMN-E", 50),
    ]
    .map(|(label, private, number)| {
        vec![
            CellValue::String(label.into()),
            CellValue::String(private.into()),
            CellValue::Int(number),
        ]
    })
    .to_vec();
    tab.table.hide_rows([3]);
    tab.hidden_columns.insert(1);
    tab.search_text = "VISIBLE".into();
    tab.filter_dirty = true;
    app
}

#[test]
fn visible_clipboard_formats_agree_on_rows_columns_and_numeric_values() {
    use octa::data::copy_formats::CopyAs;
    let mut app = visible_clipboard_app();
    app.readonly_mode = true;
    app.tabs[app.active_tab]
        .table_state
        .selected_cols
        .extend([2, 1, 0]);
    assert_eq!(
        clipboard_text(|ctx| app.do_copy(ctx)).as_deref(),
        Some("VISIBLE-A\t10\nVISIBLE-C\t30\nVISIBLE-E\t50")
    );
    assert_eq!(app.tabs[app.active_tab].filtered_rows, [0, 2, 4]);

    let markdown = clipboard_text(|ctx| app.do_copy_as(ctx, CopyAs::Markdown)).unwrap();
    assert_eq!(
        markdown,
        "| col1 | col3 |\n| --- | --- |\n| VISIBLE-A | 10 |\n| VISIBLE-C | 30 |\n| VISIBLE-E | 50 |\n"
    );
    let json = clipboard_text(|ctx| app.do_copy_as(ctx, CopyAs::Json)).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&json).unwrap(),
        serde_json::json!([
            {"col1": "VISIBLE-A", "col3": 10},
            {"col1": "VISIBLE-C", "col3": 30},
            {"col1": "VISIBLE-E", "col3": 50}
        ])
    );
    let insert = clipboard_text(|ctx| app.do_copy_as(ctx, CopyAs::Insert)).unwrap();
    assert_eq!(
        insert,
        concat!(
            "INSERT INTO \"data\" (\"col1\", \"col3\") VALUES ('VISIBLE-A', 10);\n",
            "INSERT INTO \"data\" (\"col1\", \"col3\") VALUES ('VISIBLE-C', 30);\n",
            "INSERT INTO \"data\" (\"col1\", \"col3\") VALUES ('VISIBLE-E', 50);\n",
        )
    );
    assert_eq!(
        clipboard_text(|ctx| app.do_copy_as(ctx, CopyAs::InList)).as_deref(),
        Some("('VISIBLE-A', '10', 'VISIBLE-C', '30', 'VISIBLE-E', '50')")
    );
}

#[test]
fn visible_clipboard_sparse_gap_is_blank_and_hidden_only_never_writes() {
    use octa::data::copy_formats::CopyAs;
    let mut app = visible_clipboard_app();
    let tab = &mut app.tabs[app.active_tab];
    tab.table_state.selected_cell = Some((2, 0));
    tab.table_state
        .selected_cells
        .extend([(0, 0), (1, 2), (3, 0), (2, 1), (4, 2)]);
    assert_eq!(
        app.copy_selection_to_string().as_deref(),
        Some("VISIBLE-A\t\n\t\n\t50")
    );
    assert_eq!(
        clipboard_text(|ctx| app.do_copy_as(ctx, CopyAs::InList)).as_deref(),
        Some("('VISIBLE-A', '50')")
    );
    let old = app.tabs[app.active_tab].table_state.selected_cells.len();
    assert_eq!(old, 5);
    app.tabs[app.active_tab]
        .table_state
        .selected_cells
        .retain(|&(r, c)| matches!((r, c), (1, 2) | (3, 0) | (2, 1)));
    assert!(clipboard_text(|ctx| app.do_copy(ctx)).is_none());
    for format in CopyAs::ALL {
        assert!(clipboard_text(|ctx| app.do_copy_as(ctx, format)).is_none());
    }
    assert!(clipboard_text(|ctx| app.do_cut(ctx)).is_none());
    assert_eq!(cell(&app, 1, 2), "20");
    assert_eq!(cell(&app, 3, 0), "VISIBLE-MANUALLY-HIDDEN");
}

#[test]
fn visible_clipboard_treats_highlighting_as_visible_and_respects_single_cell_hiding() {
    use octa::data::SearchResultMode;
    let mut app = visible_clipboard_app();
    app.search_result_mode = SearchResultMode::Highlight;
    let tab = &mut app.tabs[app.active_tab];
    tab.table_state.selected_cell = Some((1, 0));
    assert_eq!(
        app.copy_selection_to_string().as_deref(),
        Some("FILTERED-ROW")
    );
    app.tabs[app.active_tab].table_state.selected_cell = Some((3, 0));
    assert!(app.copy_selection_to_string().is_none());
    app.tabs[app.active_tab].table_state.selected_cell = Some((0, 1));
    assert!(app.copy_selection_to_string().is_none());
}

#[test]
fn visible_clipboard_cut_clears_only_exported_cells_with_one_undo() {
    use octa::data::CellValue::Null;
    let mut app = visible_clipboard_app();
    app.tabs[app.active_tab]
        .table_state
        .selected_rows
        .extend([4, 1, 2, 3, 0]);
    app.tabs[app.active_tab].table_state.selected_cols.insert(0);
    app.readonly_mode = true;
    assert!(clipboard_text(|ctx| app.do_cut(ctx)).is_none());
    assert_eq!(cell(&app, 0, 0), "VISIBLE-A");
    app.readonly_mode = false;
    let i = app.active_tab;
    let undo_before = app.tabs[i].table.undo_stack.len();
    assert_eq!(
        clipboard_text(|ctx| app.do_cut(ctx)).as_deref(),
        Some("VISIBLE-A\t10\nVISIBLE-C\t30\nVISIBLE-E\t50")
    );
    for row in [0, 2, 4] {
        for col in [0, 2] {
            assert_eq!(app.tabs[i].table.get(row, col), Some(&Null));
        }
        assert!(cell(&app, row, 1).starts_with("PRIVATE-COLUMN"));
    }
    assert_eq!(cell(&app, 1, 0), "FILTERED-ROW");
    assert_eq!(cell(&app, 3, 0), "VISIBLE-MANUALLY-HIDDEN");
    assert_eq!(app.tabs[i].table.undo_stack.len(), undo_before + 1);
    assert!(app.tabs[i].table.undo());
    assert_eq!(app.tabs[i].table.undo_stack.len(), undo_before);
    assert_eq!(cell(&app, 0, 0), "VISIBLE-A");
    assert_eq!(cell(&app, 4, 2), "50");
}

#[test]
fn visible_clipboard_cut_sparse_selection_does_not_clear_its_blanks() {
    let mut app = visible_clipboard_app();
    let i = app.active_tab;
    app.tabs[i]
        .table_state
        .selected_cells
        .extend([(0, 0), (2, 1), (4, 2)]);
    assert_eq!(
        clipboard_text(|ctx| app.do_cut(ctx)).as_deref(),
        Some("VISIBLE-A\t\n\t\n\t50")
    );
    assert_eq!(cell(&app, 0, 0), "");
    assert_eq!(cell(&app, 4, 2), "");
    assert_eq!(cell(&app, 0, 2), "10");
    assert_eq!(cell(&app, 2, 0), "VISIBLE-C");
    assert_eq!(cell(&app, 2, 1), "PRIVATE-COLUMN-C");
    assert!(app.tabs[i].table.undo());
    assert_eq!(cell(&app, 0, 0), "VISIBLE-A");
    assert_eq!(cell(&app, 4, 2), "50");
}

#[test]
fn grid_copy_and_paste_keep_multiline_cells_in_their_columns() {
    use octa::data::CellValue;
    let mut app = app(100);
    let ctx = eframe::egui::Context::default();
    app.open_new_table_tab(2, 2);
    let source = app.active_tab;
    let notes = [
        ["München\tWest", "Call \"buyer\""],
        ["First\nSecond", "A\r\nB"],
    ];
    for (r, row) in notes.iter().enumerate() {
        for (c, value) in row.iter().enumerate() {
            app.tabs[source]
                .table
                .set(r, c, CellValue::String((*value).into()));
        }
    }
    app.tabs[source].table_state.selected_cells.insert((0, 0));
    app.tabs[source].table_state.selected_rows.extend([1, 0]);
    let mut out = ctx.run_ui(eframe::egui::RawInput::default(), |ui| {
        app.do_copy(ui.ctx())
    });
    out.textures_delta.clear();
    let text = out
        .platform_output
        .commands
        .into_iter()
        .find_map(|cmd| {
            if let eframe::egui::OutputCommand::CopyText(text) = cmd {
                Some(text)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(
        text,
        "\"München\tWest\"\t\"Call \"\"buyer\"\"\"\n\"First\nSecond\"\t\"A\r\nB\""
    );
    app.open_new_table_tab(1, 1);
    assert_ne!(app.active_tab, source);
    let target = app.active_tab;
    app.do_paste(&ctx, Some(text));
    assert_eq!(shape(&app), (2, 2));
    for (r, row) in notes.iter().enumerate() {
        for (c, expected) in row.iter().enumerate() {
            assert_eq!(cell(&app, r, c), *expected);
            assert_eq!(
                app.tabs[source].table.get(r, c).unwrap().to_string(),
                *expected
            );
        }
    }
    assert!(app.tabs[target].table.undo());
    assert_eq!(shape(&app), (1, 1));
    assert_eq!(cell(&app, 0, 0), "");
}

#[test]
fn quoted_grid_clips_and_preserves_empty_fields_and_record_endings() {
    let mut app = app(100);
    app.open_new_table_tab(3, 3);
    app.tabs[0].bg_can_load_more = true;
    let ctx = eframe::egui::Context::default();
    let input = "one\t\"two\r\nlines\"\tlast\r\n\r\"say \"\"hi\"\"\"\t\r\nbeyond\trow";
    app.readonly_mode = true;
    app.do_paste(&ctx, Some(input.into()));
    assert_eq!(cell(&app, 0, 0), "");
    app.readonly_mode = false;
    app.do_paste(&ctx, Some(input.into()));
    assert_eq!(shape(&app), (3, 3));
    assert_eq!(cell(&app, 0, 1), "two\r\nlines");
    assert_eq!(cell(&app, 0, 2), "last");
    assert_eq!(cell(&app, 1, 0), "");
    assert_eq!(cell(&app, 2, 0), "say \"hi\"");
    assert_eq!(cell(&app, 2, 1), "");
}

#[test]
fn plain_identifiers_and_malformed_quotes_are_not_silently_rewritten() {
    use octa::data::CellValue;
    let mut app = app(100);
    app.open_new_table_tab(2, 2);
    app.tabs[0]
        .table
        .set(0, 0, CellValue::String("\"quoted identifier\"".into()));
    let text = app.copy_selection_to_string().unwrap();
    assert_eq!(text, "\"quoted identifier\"");
    app.paste_text_into_table(&text);
    assert_eq!(cell(&app, 0, 0), text);
    for text in [
        "\"unclosed\told\rnext\tend",
        "\"closed\" suffix\told\rnext\tend",
    ] {
        app.paste_text_into_table(text);
        assert_eq!(cell(&app, 0, 0), text.split('\t').next().unwrap());
        assert_eq!(cell(&app, 0, 1), "old");
        assert_eq!(cell(&app, 1, 0), "next");
        assert_eq!(cell(&app, 1, 1), "end");
    }
}

#[test]
fn single_multiline_cell_and_empty_last_row_keep_their_shape() {
    use octa::data::CellValue;
    let mut app = app(100);
    app.open_new_table_tab(1, 2);
    app.tabs[0]
        .table
        .set(0, 0, CellValue::String("first\nsecond".into()));
    assert_eq!(
        app.copy_selection_to_string().as_deref(),
        Some("\"first\nsecond\"")
    );
    app.tabs[0].table_state.selected_cols.insert(0);
    let block = app.copy_selection_to_string().unwrap();
    assert_eq!(block, "\"first\nsecond\"\n\"\"");
    app.tabs[0].table_state.selected_cols.clear();
    app.tabs[0]
        .table
        .set(1, 0, CellValue::String("old last row".into()));
    app.paste_text_into_table(&block);
    assert_eq!(shape(&app), (2, 1));
    assert_eq!(cell(&app, 0, 0), "first\nsecond");
    assert_eq!(app.tabs[0].table.get(1, 0), Some(&CellValue::Null));
}

#[test]
fn quoted_grid_notes_survive_csv_save_and_reopen() {
    use octa::data::CellValue;
    use octa::formats::{FormatReader, csv_reader::CsvReader};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("notes.csv");
    std::fs::write(&path, "id,note,flag\nINV-1,old,keep\nINV-2,another,keep\n").unwrap();
    let mut app = app(100);
    app.load_file(path.clone());
    let tab = app.active_tab;
    app.tabs[tab].table_state.selected_cell = Some((0, 1));
    let ctx = eframe::egui::Context::default();
    app.do_paste(
        &ctx,
        Some("\"Follow up\ntomorrow, 10:00\"\n\"Tab:\tdetails\"".into()),
    );
    app.save_tab(tab);
    assert!(
        !app.tabs[tab].table.is_modified(),
        "{:?}",
        app.status_message
    );
    let reopened = CsvReader.read_file(&path).unwrap();
    assert_eq!((reopened.row_count(), reopened.col_count()), (2, 3));
    assert_eq!(
        reopened.get(0, 1),
        Some(&CellValue::String("Follow up\ntomorrow, 10:00".into()))
    );
    assert_eq!(
        reopened.get(1, 1),
        Some(&CellValue::String("Tab:\tdetails".into()))
    );
    assert_eq!(reopened.get(0, 0), Some(&CellValue::String("INV-1".into())));
    assert_eq!(reopened.get(1, 2), Some(&CellValue::String("keep".into())));
}

#[test]
fn large_sheet_paste_and_history_keep_existing_grid() {
    use octa::data::{CellValue, MarkColor, MarkKey};
    use std::fmt::Write as _;
    use std::time::Instant;

    const EXISTING: usize = 3_000;
    const PASTE: usize = 4_500;
    let mut app = app(100);
    app.open_new_table_tab(2, EXISTING);
    let tab = app.active_tab;
    for row in 0..EXISTING {
        app.tabs[tab]
            .table
            .set(row, 0, CellValue::String(format!("old-{row}")));
        app.tabs[tab]
            .table
            .set(row, 1, CellValue::String(format!("note-{row}")));
    }
    let mark = MarkKey::Cell(EXISTING - 2, 0);
    app.tabs[tab].table.set_mark(mark.clone(), MarkColor::Green);
    app.tabs[tab].table.hide_rows([8]);
    let undo_before = app.tabs[tab].table.undo_stack.len();
    app.tabs[tab].table_state.selected_cell = Some((EXISTING - 1, 0));
    let mut clipboard = String::new();
    for row in 0..PASTE {
        writeln!(clipboard, "sku-{row}\tready").unwrap();
    }
    let start = Instant::now();
    app.paste_text_into_table(&clipboard);
    let paste = start.elapsed();
    assert_eq!(shape(&app), (EXISTING + PASTE - 1, 2));
    assert_eq!(cell(&app, EXISTING - 1, 0), "sku-0");
    assert_eq!(cell(&app, EXISTING + PASTE - 2, 0), "sku-4499");
    assert_eq!(app.tabs[tab].table.undo_stack.len(), undo_before + 1);

    let start = Instant::now();
    assert!(app.tabs[tab].table.undo());
    let undo = start.elapsed();
    assert_eq!(shape(&app), (EXISTING, 2));
    assert_eq!(cell(&app, EXISTING - 1, 0), "old-2999");
    assert_eq!(app.tabs[tab].table.edits.len(), EXISTING * 2);
    assert_eq!(app.tabs[tab].table.undo_stack.len(), undo_before);

    let start = Instant::now();
    assert!(app.tabs[tab].table.redo());
    let redo = start.elapsed();
    assert_eq!(shape(&app), (EXISTING + PASTE - 1, 2));
    assert_eq!(cell(&app, EXISTING + PASTE - 2, 1), "ready");
    assert_eq!(cell(&app, 8, 1), "note-8");
    assert_eq!(
        app.tabs[tab].table.marks.get(&mark),
        Some(&MarkColor::Green)
    );
    assert!(app.tabs[tab].table.hidden_rows.contains(&8));
    eprintln!(
        "BULK_GRID existing={EXISTING} incoming={PASTE} paste_us={} undo_us={} redo_us={}",
        paste.as_micros(),
        undo.as_micros(),
        redo.as_micros()
    );
}
