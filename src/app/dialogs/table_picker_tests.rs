use super::*;

#[test]
fn whole_database_opens_every_table_without_writing_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("whole.sqlite");
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch("CREATE TABLE customers (name TEXT); INSERT INTO customers VALUES ('Berlin'); CREATE TABLE orders (amount INTEGER); INSERT INTO orders VALUES (42);").unwrap();
    }
    let before = std::fs::read(&path).unwrap();
    let mut app = OctaApp::new(
        vec![],
        Default::default(),
        octa::ui::settings::IconVariant::Rose,
    );
    app.load_file(path.clone());
    assert_eq!(app.pending_table_picker.as_ref().unwrap().tables.len(), 2);
    let ctx = egui::Context::default();
    let mut out = ctx.run_ui(
        egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::CTRL,
            }],
            ..Default::default()
        },
        |_ui| render_table_picker(&mut app, &ctx),
    );
    out.textures_delta.clear();
    assert!(app.pending_table_picker.is_none());
    let tabs: Vec<_> = app
        .tabs
        .iter()
        .filter(|t| t.table.source_path.as_deref() == path.to_str())
        .collect();
    assert_eq!(tabs.len(), 2);
    assert!(
        tabs.iter()
            .any(|t| t.sheet_name.as_deref() == Some("customers"))
    );
    assert!(
        tabs.iter()
            .any(|t| t.sheet_name.as_deref() == Some("orders"))
    );
    assert!(tabs.iter().all(|t| t.table.row_count() == 1));
    assert_eq!(std::fs::read(path).unwrap(), before);
}
