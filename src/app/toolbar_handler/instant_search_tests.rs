use super::*;
use eframe::egui;
use octa::data::{CellValue, ColumnInfo, DataTable};

fn frame(
    app: &mut OctaApp,
    ctx: &egui::Context,
    width: f32,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let mut out = ctx.run_ui(
        egui::RawInput {
            events,
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, 720.0),
            )),
            ..Default::default()
        },
        |ui| app.render_toolbar(ui),
    );
    out.textures_delta.clear();
    out
}

#[test]
fn instant_search_is_pinned_and_filters_without_enter() {
    let mut app = OctaApp::new(
        vec![],
        Default::default(),
        octa::ui::settings::IconVariant::Rose,
    );
    app.tabs[0].table = DataTable {
        columns: vec![ColumnInfo {
            name: "City".into(),
            data_type: "String".into(),
        }],
        rows: vec![
            vec![CellValue::String("Berlin".into())],
            vec![CellValue::String("Hamburg".into())],
        ],
        ..DataTable::empty()
    };
    let ctx = egui::Context::default();
    let hint = octa::i18n::t("instant_search.placeholder");
    for width in [680.0, 1100.0, 1500.0] {
        let out = frame(&mut app, &ctx, width, vec![]);
        for name in [&hint, &"Smartchat".to_string()] {
            let shape = out
                .shapes
                .iter()
                .find_map(|s| match &s.shape {
                    egui::Shape::Text(t) if t.galley.text() == name => Some(t),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("missing {name} at {width}"));
            assert!(
                shape.pos.x >= 0.0 && shape.pos.x + shape.galley.size().x <= width,
                "{name} offscreen at {width}"
            );
            assert!(shape.pos.y < 70.0);
        }
    }
    app.search_focus_requested = true;
    frame(&mut app, &ctx, 680.0, vec![]);
    frame(
        &mut app,
        &ctx,
        680.0,
        vec![egui::Event::Text("Berlin".into())],
    );
    assert_eq!(app.tabs[0].search_text, "Berlin");
    assert!(app.tabs[0].filter_dirty);
    app.recompute_filter();
    assert_eq!(app.tabs[0].filtered_rows, vec![0]);
    frame(
        &mut app,
        &ctx,
        680.0,
        vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        }],
    );
    app.recompute_filter();
    assert!(app.tabs[0].search_text.is_empty());
    assert_eq!(app.tabs[0].filtered_rows, vec![0, 1]);
}

#[test]
fn instant_search_can_be_found_by_name() {
    assert!(crate::app::command_search::INSIDE.contains(&(
        "instant_search.placeholder",
        octa::ui::shortcuts::ShortcutAction::FocusSearch,
    )));
}
