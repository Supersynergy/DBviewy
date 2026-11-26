use super::*;
use std::collections::HashSet;

/// One frame of the bar, 900 px wide, with `events`. Returns its action and
/// where each text was painted.
fn frame(
    ctx: &egui::Context,
    table: &DataTable,
    zoom_percent: u32,
    zoom_step: u32,
    events: Vec<egui::Event>,
) -> (StatusBarAction, Vec<(String, egui::Rect)>) {
    let pill = pill(LiveState::Unavailable);
    let (action, texts, _) = frame_with_pill(ctx, table, zoom_percent, zoom_step, pill, events);
    (action, texts)
}

fn pill(state: LiveState) -> LivePill<'static> {
    LivePill {
        text: "Auto-refresh",
        hint: "hint",
        state,
    }
}

/// [`frame`] with a chosen live pill, also returning the fill of the dot.
fn frame_with_pill(
    ctx: &egui::Context,
    table: &DataTable,
    zoom_percent: u32,
    zoom_step: u32,
    live_pill: LivePill<'_>,
    events: Vec<egui::Event>,
) -> (
    StatusBarAction,
    Vec<(String, egui::Rect)>,
    Option<egui::Color32>,
) {
    let state = TableViewState::default();
    let hidden_columns = HashSet::new();
    let raw = egui::RawInput {
        events,
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(900.0, 28.0),
        )),
        ..Default::default()
    };
    let mut action = StatusBarAction::default();
    let mut full = ctx.run_ui(raw, |ui| {
        let cx = StatusBarCtx {
            table,
            state: &state,
            theme_mode: ThemeMode::Dark,
            filtered_count: 0,
            search_active: false,
            nav_focus_requested: false,
            zoom_percent,
            zoom_step,
            readonly: false,
            live_pill,
            busy: false,
            busy_hint: None,
            busy_cancellable: false,
            column_filter_count: 0,
            first_filtered_col: None,
            hidden_row_count: 0,
            visible_rows: &[],
            hidden_columns: &hidden_columns,
        };
        action = draw_status_bar(ui, cx, &mut String::new());
    });
    full.textures_delta.clear();
    let texts = full
        .shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::Shape::Text(t) => Some((t.galley.text().to_string(), t.visual_bounding_rect())),
            _ => None,
        })
        .collect();
    let dot = full.shapes.iter().find_map(|c| match &c.shape {
        egui::Shape::Circle(dot) => Some(dot.fill),
        _ => None,
    });
    (action, texts, dot)
}

/// The user's request: the bottom line says whether a tab refreshes by itself
/// in every state, not only while it does. Each state looks different, and a
/// click does what that state offers.
#[test]
fn the_live_pill_is_always_shown_and_a_click_does_what_its_state_offers() {
    use egui::Color32;
    let table = DataTable::empty();
    let colors = ThemeColors::for_mode(ThemeMode::Dark);
    for (state, stops, opens, fill) in [
        (LiveState::On, true, false, colors.success),
        (LiveState::Paused, true, false, colors.warning),
        (LiveState::Off, false, true, Color32::TRANSPARENT),
        (LiveState::Unavailable, false, false, Color32::TRANSPARENT),
    ] {
        let ctx = egui::Context::default();
        let (_, texts, dot) = frame_with_pill(&ctx, &table, 100, 5, pill(state), Vec::new());
        assert_eq!(dot, Some(fill), "{state:?}");
        let at = texts
            .iter()
            .find(|(t, _)| t == "Auto-refresh")
            .map(|(_, r)| r.center())
            .unwrap_or_else(|| panic!("{state:?} not drawn in {texts:?}"));
        let (action, ..) = frame_with_pill(&ctx, &table, 100, 5, pill(state), click(at));
        assert_eq!(action.stop_live, stops, "{state:?}");
        assert_eq!(action.open_auto_refresh, opens, "{state:?}");
    }
}

/// Press and release at `pos`.
fn click(pos: egui::Pos2) -> Vec<egui::Event> {
    let button = |pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    vec![egui::Event::PointerMoved(pos), button(true), button(false)]
}

/// The zoom used to appear only away from 100%, and only with a table open.
/// It is always there now, and its -, percentage and + each ask for a zoom.
#[test]
fn the_zoom_is_always_shown_and_its_buttons_zoom() {
    let ctx = egui::Context::default();
    let table = DataTable::empty();
    let (_, texts) = frame(&ctx, &table, 100, 5, Vec::new());
    let at = |text: &str| {
        texts
            .iter()
            .find(|(t, _)| t == text)
            .map(|(_, r)| r.center())
            .unwrap_or_else(|| panic!("{text:?} not drawn in {texts:?}"))
    };
    let (minus, percent, plus) = (at("-"), at("100%"), at("+"));
    assert!(
        minus.x < percent.x && percent.x < plus.x,
        "not - 100% +: {minus:?} {percent:?} {plus:?}"
    );

    // The step is the one from Settings > Appearance, 5 unless changed.
    for (pos, zoom, step, wanted) in [
        (plus, 100, 5, 105),
        (minus, 100, 5, 95),
        (plus, 100, 12, 112),
        (minus, 100, 25, 75),
        (percent, 140, 5, 100),
    ] {
        frame(&ctx, &table, zoom, step, Vec::new());
        let (action, _) = frame(&ctx, &table, zoom, step, click(pos));
        assert_eq!(
            action.zoom_to,
            Some(wanted),
            "click at {pos:?} from {zoom}% by {step}"
        );
    }
}

struct SelectionBar {
    table: DataTable,
    state: TableViewState,
    visible_rows: [usize; 3],
    hidden_columns: HashSet<usize>,
}

impl SelectionBar {
    fn new() -> Self {
        use crate::data::{CellValue as V, ColumnInfo};
        Self {
            table: DataTable {
                columns: ["Amount", "Note", "Folded"]
                    .map(|n| ColumnInfo {
                        name: n.into(),
                        data_type: "Float64".into(),
                    })
                    .to_vec(),
                rows: vec![
                    vec![V::Float(0.1), V::String("Ready".into()), V::Int(1000)],
                    vec![V::Int(900), V::Null, V::Int(4000)],
                    vec![V::Float(0.2), V::Null, V::Int(2000)],
                    vec![V::Float(0.3), V::String("Paid".into()), V::Int(3000)],
                ],
                ..DataTable::empty()
            },
            state: TableViewState::default(),
            visible_rows: [0, 2, 3],
            hidden_columns: HashSet::from([2]),
        }
    }

    fn stats(&self) -> Option<SelectionStats> {
        visible_selection_stats(
            &self.table,
            &self.state,
            &self.visible_rows,
            &self.hidden_columns,
        )
    }

    fn frame(
        &self,
        ctx: &egui::Context,
        width: f32,
        zoom: u32,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, 640.0),
            )),
            events,
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            egui::Panel::bottom("selection_test_status")
                .exact_size(28.0)
                .show(ui, |ui| {
                    draw_status_bar(
                        ui,
                        StatusBarCtx {
                            table: &self.table,
                            state: &self.state,
                            theme_mode: ThemeMode::Dark,
                            filtered_count: self.visible_rows.len(),
                            search_active: true,
                            nav_focus_requested: false,
                            zoom_percent: zoom,
                            zoom_step: 25,
                            readonly: false,
                            live_pill: pill(LiveState::Unavailable),
                            busy: false,
                            busy_hint: None,
                            busy_cancellable: false,
                            column_filter_count: 1,
                            first_filtered_col: Some(0),
                            hidden_row_count: 0,
                            visible_rows: &self.visible_rows,
                            hidden_columns: &self.hidden_columns,
                        },
                        &mut String::new(),
                    );
                });
        });
        output.textures_delta.clear();
        output
    }
}

#[test]
fn stats_count_only_visible_rows_and_columns_in_each_selection_mode() {
    let mut bar = SelectionBar::new();
    bar.state.selected_cols.extend([0, 2]);
    let stats = bar.stats().unwrap();
    assert_eq!((stats.count, stats.numeric_count), (3, 3));
    assert_eq!(
        (stats.sum, stats.avg, stats.min, stats.max),
        (0.6, 0.2, 0.1, 0.3)
    );

    bar.state.selected_cols.clear();
    bar.state.selected_rows.extend([0, 1, 3, 999]);
    let stats = bar.stats().unwrap();
    assert_eq!((stats.count, stats.numeric_count, stats.sum), (4, 2, 0.4));

    bar.state.selected_rows.clear();
    bar.state
        .selected_cells
        .extend([(0, 0), (1, 0), (2, 0), (0, 2), (3, 1), (3, 99)]);
    let stats = bar.stats().unwrap();
    assert_eq!((stats.count, stats.numeric_count, stats.sum), (3, 2, 0.3));
}

#[test]
fn two_visible_blank_cells_show_count_zero_but_a_single_cell_has_no_summary() {
    let mut bar = SelectionBar::new();
    bar.table.rows[0][1] = crate::data::CellValue::Null;
    bar.state.selected_cells.extend([(0, 1), (2, 1)]);
    let stats = bar.stats().unwrap();
    assert_eq!((stats.count, stats.numeric_count), (0, 0));
    bar.state.selected_cells.remove(&(2, 1));
    bar.state.selected_cells.insert((1, 1));
    assert!(bar.stats().is_none());
}

#[test]
fn compact_summary_follows_go_to_and_keyboard_opens_its_visible_scope() {
    use crate::i18n::t;
    use egui::accesskit::{Action, ActionRequest, TreeId};

    fn text(output: &egui::FullOutput, wanted: &str) -> Option<egui::Rect> {
        output.shapes.iter().find_map(|s| {
            let egui::Shape::Text(t) = &s.shape else {
                return None;
            };
            let bounds = t.visual_bounding_rect();
            (t.galley.text() == wanted && s.clip_rect.intersects(bounds)).then_some(bounds)
        })
    }
    let mut bar = SelectionBar::new();
    bar.state.selected_cols.insert(0);
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let caption = format!("{} 3  {} 0.600", t("status_bar.count"), t("status_bar.sum"));
    let nav = t("status_bar.nav_hint");
    let min = format!("{} 0.100", t("status_bar.min"));
    let scope = t("status_bar.visible_selection_hint");
    let before = bar.frame(&ctx, 1000.0, 100, vec![]);
    assert!(text(&before, &min).is_none());
    let caption_at = text(&before, &caption).expect("summary was drawn");
    let nav_at = text(&before, &nav).expect("Go to was drawn");
    assert!(caption_at.left() >= nav_at.right());
    let tree = before.platform_output.accesskit_update.as_ref().unwrap();
    let (button, _) = tree
        .nodes
        .iter()
        .find(|(_, node)| {
            node.label() == Some(caption.as_str()) && node.supports_action(Action::Focus)
        })
        .expect("a named, focusable summary control");
    let focused = bar.frame(
        &ctx,
        1000.0,
        100,
        vec![egui::Event::AccessKitActionRequest(ActionRequest {
            action: Action::Focus,
            target_tree: TreeId::ROOT,
            target_node: *button,
            data: None,
        })],
    );
    assert_eq!(
        focused
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .focus,
        *button
    );
    let key = |key| egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    bar.frame(&ctx, 1000.0, 100, vec![key(egui::Key::Enter)]);
    let open = bar.frame(&ctx, 1000.0, 100, vec![]);
    assert!(text(&open, &scope).is_some());
    assert!(text(&open, &min).is_some());
    bar.frame(&ctx, 1000.0, 100, vec![key(egui::Key::Escape)]);
    let closed = bar.frame(&ctx, 1000.0, 100, vec![]);
    assert!(text(&closed, &scope).is_none());

    let narrow_ctx = egui::Context::default();
    let narrow = bar.frame(&narrow_ctx, 440.0, 200, vec![]);
    assert!(
        text(&narrow, "200%").is_some(),
        "zoom stays pinned in a narrow window"
    );
}
