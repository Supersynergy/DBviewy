use super::should_show;

#[test]
fn a_fresh_install_sees_its_notes() {
    assert!(should_show(true, "", "0.17.1"));
}

#[test]
fn an_acknowledged_version_stays_silent() {
    assert!(!should_show(true, "0.17.1", "0.17.1"));
}

#[test]
fn the_next_release_opens_the_window_again() {
    assert!(should_show(true, "0.17.1", "0.17.2"));
}

#[test]
fn the_setting_wins_over_everything() {
    assert!(!should_show(false, "", "0.17.1"));
}

#[test]
fn a_dev_build_stays_silent() {
    assert!(!should_show(true, "", "0.0.0-dev"));
}

#[test]
fn dbviewy_notes_and_window_title_distinguish_the_fork_from_upstream() {
    assert!(super::NOTES.starts_with("# DBviewy"));
    assert!(super::NOTES.contains("Inherited Octa release notes"));
    let mut app = crate::app::state::OctaApp::new(
        vec![],
        Default::default(),
        octa::ui::settings::IconVariant::Rose,
    );
    app.pending_release_notes = true;
    let ctx = eframe::egui::Context::default();
    octa::ui::theme::apply_theme(
        &ctx,
        app.settings.default_theme,
        octa::ui::theme::FontSettings {
            size: app.settings.font_size,
            body: app.settings.body_font,
            custom_path: Some(app.settings.custom_font_path.as_str()),
        },
    );

    let mut texts = Vec::new();
    for _ in 0..3 {
        let mut out = ctx.run_ui(
            eframe::egui::RawInput {
                screen_rect: Some(eframe::egui::Rect::from_min_size(
                    eframe::egui::Pos2::ZERO,
                    eframe::egui::vec2(1000.0, 800.0),
                )),
                ..Default::default()
            },
            |_ui| super::render_release_notes_dialog(&mut app, &ctx),
        );
        texts = out
            .shapes
            .iter()
            .filter_map(|s| {
                if let eframe::egui::Shape::Text(t) = &s.shape {
                    Some(t.galley.text().to_owned())
                } else {
                    None
                }
            })
            .collect();
        out.textures_delta.clear();
    }
    assert!(
        texts
            .iter()
            .any(|t| t.contains("DBviewy") && t.contains(super::VERSION)),
        "painted: {texts:?}"
    );
}
