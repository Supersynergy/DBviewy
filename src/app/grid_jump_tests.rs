use eframe::egui;
use octa::data::CellValue;
use octa::ui::settings::{AppSettings, IconVariant};

use super::OctaApp;
use crate::app::state::NavDir;

fn app() -> (OctaApp, egui::Context) {
    let settings = AppSettings {
        zoom_percent: 175,
        ..AppSettings::default()
    };
    let mut app = OctaApp::new(vec![], settings, IconVariant::Rose);
    app.open_new_table_tab(3, 260);
    let tab = &mut app.tabs[app.active_tab];
    for (i, row) in tab.table.rows.iter_mut().enumerate() {
        row[0] = CellValue::String(format!("QA-{i:03}"));
    }
    tab.table.hide_rows((1..260).step_by(2));
    // Uniform pixel guesses land far from the actual display row even
    // when callers remember to account for the active filter.
    tab.table_state.row_heights.extend((0..260).map(|i| {
        let height = if i < 50 {
            26.0
        } else {
            80.0 + (i % 6) as f32 * 20.0
        };
        (i, height)
    }));
    app.recompute_filter();
    let ctx = egui::Context::default();
    ctx.options_mut(|o| o.zoom_with_keyboard = false);
    octa::ui::theme::apply_theme(
        &ctx,
        app.theme_mode,
        octa::ui::theme::FontSettings {
            size: app.settings.font_size,
            body: app.settings.body_font,
            custom_path: None,
        },
    );
    (app, ctx)
}

fn key(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

fn step(app: &mut OctaApp, ctx: &egui::Context, events: Vec<egui::Event>) -> Vec<String> {
    fn collect(shape: &egui::Shape, clip: egui::Rect, text: &mut Vec<String>) {
        match shape {
            egui::Shape::Text(t) if clip.intersects(t.visual_bounding_rect()) => {
                text.push(t.galley.text().to_owned());
            }
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, clip, text)),
            _ => {}
        }
    }
    let events = std::iter::once(egui::Event::ModifiersChanged(
        events
            .iter()
            .find_map(|e| match e {
                egui::Event::Key { modifiers, .. } => Some(*modifiers),
                _ => None,
            })
            .unwrap_or_default(),
    ))
    .chain(events)
    .collect();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1100.0, 670.0),
        )),
        events,
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |ui| {
        app.handle_shortcuts(ui.ctx());
        let n = app.tabs[app.active_tab].filtered_rows.len();
        app.render_status_bar(ui, n, false);
        app.render_central_panel(ui);
    });
    // The font atlas deltas belong to the renderer. This harness only
    // checks layout and input, so there is no texture backend to upload to.
    output.textures_delta.clear();
    let mut text = Vec::new();
    output
        .shapes
        .iter()
        .for_each(|s| collect(&s.shape, s.clip_rect, &mut text));
    text
}

fn assert_target(app: &OctaApp, row: usize, text: &[String]) {
    let tab = &app.tabs[app.active_tab];
    let label = format!("QA-{row:03}");
    assert!(text.iter().any(|t| t == &label), "{label} not drawn");
    assert_eq!(tab.table_state.selected_cell, Some((row, 0)));
    assert!(tab.table_state.selected_rows.is_empty());
    assert!(tab.table_state.selected_cols.is_empty());
    assert!(tab.table_state.selected_cells.is_empty());
    assert_eq!(tab.filtered_rows.len(), 130);
    assert_eq!(app.zoom_percent, 175);
}

#[test]
fn go_to_keyboard_reveals_the_row_and_leaves_the_address_editor() {
    let (mut app, ctx) = app();
    step(&mut app, &ctx, vec![]);
    app.tabs[app.active_tab]
        .table_state
        .selected_cells
        .extend([(0, 0), (2, 0)]);
    step(
        &mut app,
        &ctx,
        vec![key(egui::Key::G, egui::Modifiers::COMMAND)],
    );
    let field = ctx
        .memory(|m| m.focused())
        .expect("Ctrl+G must focus the address field");
    assert!(egui::TextEdit::load_state(&ctx, field).is_some());
    step(&mut app, &ctx, vec![egui::Event::Text("R221:C1".into())]);
    assert_eq!(app.nav_input, "R221:C1");
    let text = step(
        &mut app,
        &ctx,
        vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
    );
    assert!(app.nav_input.is_empty());
    assert_ne!(ctx.memory(|m| m.focused()), Some(field));
    assert_target(&app, 220, &text);
}

#[test]
fn highlighted_results_and_problems_reveal_the_visible_filtered_rows() {
    let (mut app, ctx) = app();
    step(&mut app, &ctx, vec![]);
    let tab = &mut app.tabs[app.active_tab];
    tab.table_state.selected_cells.extend([(0, 0), (2, 0)]);
    tab.search_cell_matches = vec![(10, 0), (180, 0)];
    tab.search_nav.pending_jump = Some(NavDir::Next);
    let text = step(&mut app, &ctx, vec![]);
    assert_target(&app, 180, &text);
    app.tabs[app.active_tab].search_nav.pending_jump = Some(NavDir::Prev);
    let text = step(&mut app, &ctx, vec![]);
    assert_target(&app, 10, &text);
    app.tabs[app.active_tab]
        .outlier_cells
        .extend([(181, 0), (200, 0)]);
    app.jump_to_problem(true);
    let text = step(&mut app, &ctx, vec![]);
    assert_target(&app, 200, &text);
}

#[test]
fn stale_coordinates_and_hidden_rows_do_not_move_the_viewport() {
    let (mut app, ctx) = app();
    step(&mut app, &ctx, vec![]);
    app.jump_to_cell(180, 0);
    let text = step(&mut app, &ctx, vec![]);
    assert_target(&app, 180, &text);
    let old_y = app.tabs[app.active_tab].table_state.scroll_y();
    app.jump_to_cell(900, 0);
    app.jump_to_cell(0, 99);
    assert_eq!(
        app.tabs[app.active_tab].table_state.selected_cell,
        Some((180, 0))
    );
    app.jump_to_cell(1, 2);
    step(&mut app, &ctx, vec![]);
    assert_eq!(app.tabs[app.active_tab].table_state.scroll_y(), old_y);
    assert_eq!(
        app.tabs[app.active_tab].table_state.selected_cell,
        Some((1, 2))
    );
}
