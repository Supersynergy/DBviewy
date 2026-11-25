use super::{
    ToolbarAction,
    instant_search::{self, Entry},
};
use egui::{Context, Event, Key, Modifiers, Pos2, RawInput, Rect, Vec2};

fn frame(
    ctx: &Context,
    text: &mut String,
    events: Vec<Event>,
    focus: bool,
    asking: bool,
) -> (ToolbarAction, egui::Response) {
    let mut action = ToolbarAction::default();
    let mut response = None;
    let mut out = ctx.run_ui(
        RawInput {
            events,
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(680.0, 100.0))),
            ..Default::default()
        },
        |ui| {
            crate::ui::control_row::control_line(ui, |ui| {
                response = Some(instant_search::draw(
                    ui,
                    text,
                    Entry {
                        asking,
                        has_data: true,
                        focus_requested: focus,
                        width: 180.0,
                    },
                    &mut action,
                ));
            });
        },
    );
    out.textures_delta.clear();
    (action, response.unwrap())
}

#[test]
fn instant_typing_and_escape_restore_the_query_and_focus() {
    let ctx = Context::default();
    let mut text = String::new();
    frame(&ctx, &mut text, vec![], true, false);
    let (action, response) = frame(
        &ctx,
        &mut text,
        vec![Event::Text("Berlin".into())],
        false,
        false,
    );
    assert_eq!(text, "Berlin");
    assert!(action.search_changed);
    assert!(response.has_focus());
    let (action, response) = frame(
        &ctx,
        &mut text,
        vec![Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
        false,
        false,
    );
    assert!(text.is_empty());
    assert!(action.search_changed);
    assert!(response.has_focus());
}

#[test]
fn ask_draft_never_becomes_an_instant_filter() {
    let ctx = Context::default();
    let mut text = String::new();
    frame(&ctx, &mut text, vec![], true, true);
    let (action, _) = frame(
        &ctx,
        &mut text,
        vec![Event::Text("show Berlin".into())],
        false,
        true,
    );
    assert_eq!(text, "show Berlin");
    assert!(!action.search_changed);
}

#[test]
fn dbviewy_logo_is_embedded_and_renderable() {
    let svg = crate::ui::settings::IconVariant::Rose.svg_source();
    assert!(svg.contains("DBviewy"));
    let tree = resvg::usvg::Tree::from_str(svg, &Default::default()).unwrap();
    let mut pixels = resvg::tiny_skia::Pixmap::new(256, 256).unwrap();
    resvg::render(&tree, Default::default(), &mut pixels.as_mut());
    assert!(pixels.data().chunks_exact(4).filter(|p| p[3] > 0).count() > 30_000);
}
