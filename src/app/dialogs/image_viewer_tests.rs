use super::*;

#[test]
fn fit_zoom_contains_the_picture() {
    // Wider viewport than picture: height is the limit.
    let z = fit_zoom(egui::vec2(800.0, 400.0), egui::vec2(200.0, 200.0));
    assert!((z - 2.0).abs() < 1e-6);
    // Taller viewport: width is the limit.
    let z = fit_zoom(egui::vec2(100.0, 800.0), egui::vec2(200.0, 200.0));
    assert!((z - 0.5).abs() < 1e-6);
    // Aspect is kept: a 2:1 picture in a square viewport scales by width.
    let z = fit_zoom(egui::vec2(400.0, 400.0), egui::vec2(400.0, 200.0));
    assert!((z - 1.0).abs() < 1e-6);
}

#[test]
fn fit_zoom_never_breaks_on_empty_input() {
    assert_eq!(fit_zoom(egui::vec2(0.0, 0.0), egui::vec2(10.0, 10.0)), 1.0);
    assert_eq!(
        fit_zoom(egui::vec2(100.0, 100.0), egui::vec2(0.0, 10.0)),
        1.0
    );
    // A tiny picture still gets a usable, positive scale.
    assert!(fit_zoom(egui::vec2(100.0, 100.0), egui::vec2(1.0, 1.0)) > 0.0);
}

fn viewer_app(rows: &[&[&str]], order: &[usize]) -> OctaApp {
    use octa::data::ColumnInfo;
    use octa::ui::settings::{AppSettings, IconVariant};
    let mut app = OctaApp::new(vec![], AppSettings::default(), IconVariant::Rose);
    let tab = &mut app.tabs[0];
    tab.view_mode = octa::data::ViewMode::Table;
    tab.table = DataTable {
        columns: ["Vorschaubild", "Bild", "Weitere Ansicht"]
            .map(|name| ColumnInfo {
                name: name.into(),
                data_type: "Utf8".into(),
            })
            .into(),
        rows: rows
            .iter()
            .map(|row| row.iter().map(|v| CellValue::String((*v).into())).collect())
            .collect(),
        ..DataTable::empty()
    };
    tab.filtered_rows = order.to_vec();
    app
}

#[test]
fn full_images_precede_previews_and_empty_rows_remain_in_the_filtered_walk() {
    let mut app = viewer_app(
        &[
            &[
                "https://example.invalid/preview.jpg",
                "https://example.invalid/front.jpg",
                "https://example.invalid/back.jpg",
            ],
            &["", "", ""],
            &["https://example.invalid/other.jpg", "", ""],
        ],
        &[2, 1, 0],
    );
    app.open_image_viewer(0, 0);
    let mut st = app.image_viewer.take().unwrap();
    let tab = &app.tabs[0];
    assert_eq!(st.items.len(), 2);
    assert_eq!(st.items[st.pos].col, 1);
    st.next_picture(tab, true);
    assert_eq!(st.items[st.pos].col, 2);
    st.next_picture(tab, false);
    st.next_picture(tab, false);
    assert_eq!(st.row, 1);
    assert!(st.items.is_empty());
    st.next_picture(tab, false);
    assert_eq!(st.row, 2);
    assert_eq!(st.items[st.pos].col, 0);
}

#[test]
fn keyboard_viewer_traverses_filtered_empty_rows_then_returns_selection_to_grid() {
    use egui::{Key, Modifiers};
    let mut app = viewer_app(&[&[""], &[""], &[""]], &[2, 0]);
    app.tabs[0].table_state.selected_cell = Some((2, 0));
    let ctx = egui::Context::default();
    let frame = |app: &mut OctaApp, key, modifiers| {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000.0, 800.0),
            )),
            events: vec![
                egui::Event::ModifiersChanged(modifiers),
                egui::Event::Key {
                    key,
                    modifiers,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                },
            ],
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            app.handle_shortcuts(ui.ctx());
            render_image_viewer(app, ui.ctx());
        });
        out.textures_delta.clear();
    };
    frame(&mut app, Key::F6, Modifiers::SHIFT);
    assert_eq!(app.image_viewer.as_ref().unwrap().row, 2);
    frame(&mut app, Key::Space, Modifiers::NONE);
    assert_eq!(app.image_viewer.as_ref().unwrap().row, 0);
    frame(&mut app, Key::Space, Modifiers::SHIFT);
    assert_eq!(app.image_viewer.as_ref().unwrap().row, 2);
    frame(&mut app, Key::ArrowDown, Modifiers::ALT);
    assert_eq!(app.image_viewer.as_ref().unwrap().row, 2);
    frame(&mut app, Key::ArrowDown, Modifiers::NONE);
    frame(&mut app, Key::Plus, Modifiers::SHIFT);
    assert_eq!(app.image_viewer.as_ref().unwrap().zoom, Some(ZOOM_STEP));
    frame(&mut app, Key::Escape, Modifiers::NONE);
    assert!(app.image_viewer.is_none());
    assert_eq!(app.tabs[0].table_state.selected_cell, Some((0, 0)));
    assert!(ctx.memory(|m| m.focused()).is_none());
}

#[test]
fn sqlite_gallery_switches_tables_by_keyboard_and_album_has_one_picture_per_import() {
    use egui::{Key, Modifiers};
    use octa::data::embedded_media::{self, Source};
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("shop.sqlite");
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute_batch("CREATE TABLE Artikel(Name TEXT); INSERT INTO Artikel VALUES('Flasche')")
        .unwrap();
    let image = dir.path().join("bild.png");
    image::RgbImage::new(20, 30).save(&image).unwrap();
    embedded_media::import(&db, Source::File(image)).unwrap();
    let mut app = viewer_app(&[&[""]], &[0]);
    app.load_table(db, "Artikel".into(), Some("SQLite"));
    app.recompute_filter();
    app.open_image_viewer(0, 0);
    let ctx = egui::Context::default();
    let mut frame = |key: Key| {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 700.0),
            )),
            events: vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
            ..Default::default()
        };
        let mut output = ctx.run_ui(raw, |ui| {
            super::super::table_picker::render_table_picker(&mut app, ui.ctx());
            render_image_viewer(&mut app, ui.ctx());
        });
        output.textures_delta.clear();
    };
    frame(Key::T);
    frame(Key::ArrowDown);
    frame(Key::Enter);
    let st = app.image_viewer.as_ref().unwrap();
    assert_eq!(
        app.tabs[app.active_tab]
            .table
            .db_meta
            .as_ref()
            .unwrap()
            .table_name,
        embedded_media::ALBUM
    );
    assert_eq!(st.items.len(), 1);
    assert!(embedded_media::is_reference(&st.items[0].url));
}
