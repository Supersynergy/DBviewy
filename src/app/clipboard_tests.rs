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
