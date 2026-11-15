use super::*;
use eframe::egui;

#[test]
fn file_menu_copies_source_and_command_field_stays_on_screen() {
    let mut app = OctaApp::new(
        vec![],
        Default::default(),
        octa::ui::settings::IconVariant::Rose,
    );
    let source = "/tmp/Octa Prüfliste.xlsx";
    app.tabs[0].table.source_path = Some(source.into());
    let ctx = egui::Context::default();
    let mut frame = |width, events| {
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
    };
    let label = |out: &egui::FullOutput, name: &str| {
        out.shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(t) if t.galley.text() == name => {
                    Some(t.pos + t.galley.size() * 0.5)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing visible label {name}"))
    };
    for width in [680.0, 1500.0] {
        let out = frame(width, vec![]);
        let command = label(&out, "Smartchat");
        assert!(
            command.x > width * 0.5 && command.x < width && command.y < 70.0,
            "{command:?} on {width}"
        );
    }
    let press = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    let out = frame(1500.0, vec![]);
    let file = label(&out, &octa::i18n::t("menu.file"));
    frame(1500.0, vec![egui::Event::PointerMoved(file)]);
    frame(1500.0, vec![press(file, true)]);
    frame(1500.0, vec![press(file, false)]);
    let out = frame(1500.0, vec![]);
    let copy = label(&out, &octa::i18n::t("file_menu.copy_path"));
    frame(1500.0, vec![egui::Event::PointerMoved(copy)]);
    frame(1500.0, vec![press(copy, true)]);
    let out = frame(1500.0, vec![press(copy, false)]);
    assert!(
        out.platform_output
            .commands
            .iter()
            .any(|cmd| matches!(cmd,egui::OutputCommand::CopyText(text) if text==source))
    );
}
