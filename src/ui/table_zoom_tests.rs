use super::*;
use egui::accesskit::{Action, ActionRequest, Node, NodeId, Role, TreeId, TreeUpdate};

#[derive(Clone, Copy, Debug)]
enum Surface {
    Status,
    Ribbon,
    View,
}

#[derive(Debug, Eq, PartialEq)]
enum Request {
    None,
    Percent(u32),
    In,
    Out,
    Reset,
}

fn frame(
    ctx: &egui::Context,
    surface: Surface,
    percent: u32,
    width: f32,
    events: Vec<egui::Event>,
) -> (Request, TreeUpdate, Vec<String>) {
    let mut actual = Request::None;
    let mut full = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, 220.0),
            )),
            events,
            ..Default::default()
        },
        |ui| {
            let mut toolbar = ToolbarAction::default();
            let zoom = match surface {
                Surface::Status => {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        status_controls(ui, percent, 5, Color32::GRAY)
                    })
                    .inner
                }
                Surface::Ribbon => {
                    ribbon_controls(ui, percent, &mut toolbar);
                    None
                }
                Surface::View => {
                    view_controls(ui, percent, Color32::GRAY, &mut toolbar);
                    None
                }
            };
            actual = match (zoom, toolbar.zoom_in, toolbar.zoom_out, toolbar.zoom_reset) {
                (Some(value), false, false, false) => Request::Percent(value),
                (None, true, false, false) => Request::In,
                (None, false, true, false) => Request::Out,
                (None, false, false, true) => Request::Reset,
                (None, false, false, false) => Request::None,
                a => panic!("got multiple zoom actions: {a:?}"),
            };
        },
    );
    full.textures_delta.clear();
    let tree = full.platform_output.accesskit_update.take().unwrap();
    let paint = full
        .shapes
        .iter()
        .filter_map(|item| match &item.shape {
            egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
            _ => None,
        })
        .collect();
    (actual, tree, paint)
}

fn find<'a>(tree: &'a TreeUpdate, label: &str) -> (NodeId, &'a Node) {
    let mut matches = tree
        .nodes
        .iter()
        .filter(|(_, node)| node.role() == Role::Button && node.label() == Some(label));
    let node = matches
        .next()
        .unwrap_or_else(|| panic!("missing zoom button: {label}"));
    assert!(matches.next().is_none(), "duplicate zoom button: {label}");
    (node.0, &node.1)
}

fn event(node: NodeId, action: Action) -> egui::Event {
    egui::Event::AccessKitActionRequest(ActionRequest {
        action,
        target_tree: TreeId::ROOT,
        target_node: node,
        data: None,
    })
}

#[test]
fn all_view_ribbon_and_status_buttons_name_the_table_and_route_their_actions() {
    for surface in [Surface::Status, Surface::Ribbon, Surface::View] {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let scope = t("view_menu.table");
        let reset = if matches!(surface, Surface::View) {
            format!("{scope}: {}", t("view_menu.zoom_reset"))
        } else {
            format!("{scope}: 125%, {}", t("view_menu.zoom_reset_hint"))
        };
        let is_status = matches!(surface, Surface::Status);
        for (label, expected) in [
            (
                format!("{scope}: {}", t("command.zoom_in")),
                if is_status {
                    Request::Percent(130)
                } else {
                    Request::In
                },
            ),
            (
                format!("{scope}: {}", t("command.zoom_out")),
                if is_status {
                    Request::Percent(120)
                } else {
                    Request::Out
                },
            ),
            (
                reset,
                if is_status {
                    Request::Percent(100)
                } else {
                    Request::Reset
                },
            ),
        ] {
            let (start, tree, paint) = frame(&ctx, surface, 125, 320.0, vec![]);
            assert_eq!(start, Request::None);
            if matches!(surface, Surface::View) {
                assert!(paint.contains(&format!("{scope}: {}", t("view_menu.zoom"))));
            }
            let (id, node) = find(&tree, &label);
            assert!(!node.is_disabled());
            assert!(node.supports_action(Action::Focus));
            assert!(node.supports_action(Action::Click));
            let bounds = node.bounds().expect("zoom button bounds");
            assert!(
                bounds.x0 >= 0.0 && bounds.x1 <= 320.0,
                "{surface:?} {label}: {bounds:?}"
            );
            let (result, _, _) = frame(&ctx, surface, 125, 320.0, vec![event(id, Action::Click)]);
            assert_eq!(result, expected, "{surface:?} {label}");
        }
    }
}

#[test]
fn ribbon_zoom_focused_by_accessibility_accepts_enter() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let (_, tree, _) = frame(&ctx, Surface::Ribbon, 125, 420.0, vec![]);
    let label = format!(
        "{}: 125%, {}",
        t("view_menu.table"),
        t("view_menu.zoom_reset_hint")
    );
    let (id, _) = find(&tree, &label);
    let (result, tree, _) = frame(
        &ctx,
        Surface::Ribbon,
        125,
        420.0,
        vec![event(id, Action::Focus)],
    );
    assert_eq!(result, Request::None);
    assert_eq!(tree.focus, id);
    let enter = egui::Event::Key {
        key: egui::Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    let (result, _, _) = frame(&ctx, Surface::Ribbon, 125, 420.0, vec![enter]);
    assert_eq!(result, Request::Reset);
}

#[test]
fn view_zoom_reset_is_still_present_but_disabled_at_one_hundred_percent() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let (_, tree, _) = frame(&ctx, Surface::View, 100, 420.0, vec![]);
    let label = format!("{}: {}", t("view_menu.table"), t("view_menu.zoom_reset"));
    let (id, node) = find(&tree, &label);
    assert!(node.is_disabled());
    assert_eq!(
        frame(
            &ctx,
            Surface::View,
            100,
            420.0,
            vec![event(id, Action::Click)]
        )
        .0,
        Request::None
    );
}
