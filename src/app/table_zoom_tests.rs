//! Table zoom integration checks against real egui frame output.
use eframe::egui;
use octa::data::CellValue;
use octa::ui::settings::{AppSettings, IconVariant};

use super::OctaApp;

const CHILD: &str = "DBVIEWY_TABLE_ZOOM_TEST_CHILD";

#[derive(Debug, PartialEq)]
struct Text {
    pos: egui::Pos2,
    size: egui::Vec2,
    font: f32,
}

fn frame(app: &mut OctaApp, ctx: &egui::Context, key: Option<egui::Key>) -> Vec<(String, Text)> {
    let modifiers = if key.is_some() {
        egui::Modifiers::COMMAND
    } else {
        egui::Modifiers::NONE
    };
    let mut events = vec![egui::Event::ModifiersChanged(modifiers)];
    if let Some(key) = key {
        events.push(egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        });
    }
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1100.0, 670.0),
        )),
        events,
        ..Default::default()
    };
    let mut out = ctx.run_ui(input, |ui| {
        if key.is_some() {
            app.handle_shortcuts(ui.ctx());
        }
        app.render_toolbar(ui);
        app.render_tab_bar(ui);
        app.render_status_bar(ui, 3, false);
        app.render_central_panel(ui);
    });
    out.textures_delta.clear();
    out.shapes
        .iter()
        .filter_map(|shape| {
            let egui::Shape::Text(text) = &shape.shape else {
                return None;
            };
            Some((
                text.galley.text().to_owned(),
                Text {
                    pos: text.pos,
                    size: text.galley.size(),
                    font: text.galley.job.sections.first()?.format.font_id.size,
                },
            ))
        })
        .collect()
}

fn label<'a>(frame: &'a [(String, Text)], name: &str) -> &'a Text {
    &frame
        .iter()
        .find(|(text, _)| text == name)
        .unwrap_or_else(|| {
            panic!(
                "missing {name:?}; labels: {:?}",
                frame.iter().map(|(s, _)| s).collect::<Vec<_>>()
            )
        })
        .1
}

fn app(settings: AppSettings) -> OctaApp {
    let mut app = OctaApp::new(vec![], settings, IconVariant::Rose);
    app.open_new_table_tab(1, 3);
    let tab = &mut app.tabs[app.active_tab];
    tab.custom_tab_label = Some("Zoom-QA".into());
    tab.table.rows[0][0] = CellValue::String("Q7R9".into());
    tab.table.rows[1][0] = CellValue::String("S3T8".into());
    tab.table_state.selected_cell = Some((2, 0));
    app.recompute_filter();
    app
}

#[test]
fn keyboard_table_zoom_preserves_painted_chrome_and_restarts_at_the_same_zoom() {
    if std::env::var_os(CHILD).is_none() {
        let dir = tempfile::tempdir().unwrap();
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "app::clipboard::zoom_tests::keyboard_table_zoom_preserves_painted_chrome_and_restarts_at_the_same_zoom", "--nocapture"])
            .env(CHILD, "1")
            .env("OCTA_CONFIG_DIR", dir.path())
            .output()
            .unwrap();
        assert!(
            child.status.success() && dir.path().join("settings.toml").is_file(),
            "{}\n{}",
            String::from_utf8_lossy(&child.stdout),
            String::from_utf8_lossy(&child.stderr),
        );
        return;
    }
    let ctx = egui::Context::default();
    ctx.options_mut(|o| o.zoom_with_keyboard = false);
    let mut first = app(AppSettings {
        font_size: 13.0,
        zoom_step: 25,
        show_ribbon: true,
        ..AppSettings::default()
    });
    first.apply_app_theme(&ctx);
    frame(&mut first, &ctx, None);
    let before = frame(&mut first, &ctx, None);
    let zoomed = frame(&mut first, &ctx, Some(egui::Key::Equals));
    let file = octa::i18n::t("menu.file");
    assert_eq!(first.zoom_percent, 125);
    assert_eq!(
        first.tabs[first.active_tab].table_state.selected_cell,
        Some((2, 0))
    );
    assert_eq!(label(&before, &file), label(&zoomed, &file));
    assert_eq!(label(&before, "Zoom-QA"), label(&zoomed, "Zoom-QA"));
    assert_eq!(label(&before, "100%").font, label(&zoomed, "125%").font);
    assert!(label(&zoomed, "Q7R9").font > label(&before, "Q7R9").font);
    let row_distance =
        |frame: &[(String, Text)]| label(frame, "S3T8").pos.y - label(frame, "Q7R9").pos.y;
    assert!(row_distance(&zoomed) > row_distance(&before));

    let mut reopened = app(AppSettings::load());
    let fresh_ctx = egui::Context::default();
    fresh_ctx.options_mut(|options| options.zoom_with_keyboard = false);
    reopened.apply_app_theme(&fresh_ctx);
    frame(&mut reopened, &fresh_ctx, None);
    let restored = frame(&mut reopened, &fresh_ctx, None);
    assert_eq!(reopened.zoom_percent, 125);
    assert_eq!(label(&zoomed, &file), label(&restored, &file));
    assert_eq!(label(&zoomed, "Q7R9"), label(&restored, "Q7R9"));
    let reset = frame(&mut reopened, &fresh_ctx, Some(egui::Key::Num0));
    assert_eq!(reopened.zoom_percent, 100);
    assert_eq!(label(&before, &file), label(&reset, &file));
    assert_eq!(label(&before, "Q7R9"), label(&reset, "Q7R9"));
    assert_eq!(AppSettings::load().zoom_percent(), 100);
}
