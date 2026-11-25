use super::*;

#[test]
fn start_screen_shows_recent_files_actions_and_details_without_a_giant_logo() {
    let mut app = OctaApp::new(
        vec![],
        Default::default(),
        octa::ui::settings::IconVariant::Rose,
    );
    let file = tempfile::NamedTempFile::new().unwrap();
    app.recent_files = vec![file.path().to_string_lossy().to_string()];
    let ctx = egui::Context::default();
    let mut out = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 720.0),
            )),
            ..Default::default()
        },
        |ui| app.render_start_screen(ui),
    );
    let text: Vec<_> = out
        .shapes
        .iter()
        .filter_map(|s| {
            if let egui::Shape::Text(t) = &s.shape {
                Some(t.galley.text().to_owned())
            } else {
                None
            }
        })
        .collect();
    assert!(text.iter().any(|s| s == &t("menu.recent_files")));
    assert!(text.iter().any(|s| s == &t("common.open")));
    assert!(text.iter().any(|s| s.contains("Ctrl+O")));
    assert!(
        text.iter()
            .any(|s| s.contains(file.path().file_name().unwrap().to_str().unwrap()))
    );
    assert!(text.iter().any(|s| s.contains("SQLite / DuckDB")));
    out.textures_delta.clear();
}

#[test]
fn search_follows_the_menus_and_precedes_smartchat() {
    let mut app = OctaApp::new(
        vec![],
        Default::default(),
        octa::ui::settings::IconVariant::Rose,
    );
    for width in [680.0, 1100.0, 1500.0, 2056.0] {
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width, 720.0),
                )),
                ..Default::default()
            },
            |ui| app.render_toolbar(ui),
        );
        let pos = |name: &str| {
            out.shapes
                .iter()
                .find_map(|s| match &s.shape {
                    egui::Shape::Text(t) if t.galley.text() == name => Some(t.pos),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("missing {name} at {width}"))
        };
        let search = pos(&t("instant_search.placeholder"));
        // The search follows the menus directly (no centred gap, no second
        // row of search controls) and Smartchat stays to its right.
        assert!(pos(&t("menu.file")).x < search.x);
        if width >= 1100.0 {
            // Wide enough that the menus fit without their wheel-scroll
            // fallback: all eight sit before the search.
            assert!(pos(&t("menu.help")).x < search.x, "help at {width}");
            assert!(search.x < width * 0.65, "search {search:?} at {width}");
        }
        assert!(pos("Smartchat").x > search.x);
        out.textures_delta.clear();
    }
}
