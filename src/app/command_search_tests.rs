use super::*;

/// A text field elsewhere in the window, like the SQL editor.
fn other_field() -> egui::Id {
    egui::Id::new("a_field_elsewhere")
}

fn pointer(at: egui::Pos2, pressed: bool) -> Vec<egui::Event> {
    vec![
        egui::Event::PointerMoved(at),
        egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        },
    ]
}

fn key(key: egui::Key) -> Vec<egui::Event> {
    vec![egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]
}

/// One frame in the app's own order: fire what was queued, read the shortcuts,
/// draw what lies under the box, then the box.
fn step(app: &mut OctaApp, ctx: &egui::Context, events: Vec<egui::Event>) {
    step_at_width(app, ctx, events, 1000.0);
}

fn step_at_width(app: &mut OctaApp, ctx: &egui::Context, events: Vec<egui::Event>, width: f32) {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(width, 800.0),
        )),
        events,
        ..Default::default()
    };
    let mut text = String::new();
    let mut out = ctx.run_ui(input, |ui| {
        octa::ui::shortcuts::begin_frame();
        app.handle_shortcuts(ui.ctx());
        ui.add(egui::TextEdit::singleline(&mut text).id(other_field()));
        app.render_command_search(ui.ctx());
    });
    out.textures_delta.clear();
}

fn settle(app: &mut OctaApp, ctx: &egui::Context) {
    for _ in 0..3 {
        step(app, ctx, vec![]);
    }
}

/// Press, release, and the frame a queued command fires in.
fn click(app: &mut OctaApp, ctx: &egui::Context, at: egui::Pos2) {
    step(app, ctx, pointer(at, true));
    step(app, ctx, pointer(at, false));
    settle(app, ctx);
}

/// The box open with `query` typed and laid out.
fn opened(query: &str) -> (OctaApp, egui::Context) {
    let mut app = OctaApp::new(
        vec![],
        octa::ui::settings::AppSettings::default(),
        octa::ui::settings::IconVariant::Rose,
    );
    let ctx = egui::Context::default();
    app.open_command_search(&ctx);
    app.command_search.as_mut().unwrap().query = query.into();
    settle(&mut app, &ctx);
    (app, ctx)
}

fn window_rect(ctx: &egui::Context) -> egui::Rect {
    ctx.memory(|m| m.area_rect(window_id()))
        .expect("the box was drawn")
}

const OUTSIDE: egui::Pos2 = egui::pos2(950.0, 750.0);

/// Every text egui paints in a frame, with where it starts.
fn painted_texts(app: &mut OctaApp, ctx: &egui::Context) -> Vec<(String, egui::Pos2)> {
    fn walk(shape: &egui::Shape, out: &mut Vec<(String, egui::Pos2)>) {
        match shape {
            egui::Shape::Text(t) => out.push((t.galley.text().to_owned(), t.pos)),
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, out)),
            _ => {}
        }
    }
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        app.render_command_search(ui.ctx());
    });
    output.textures_delta.clear();
    let mut texts = Vec::new();
    output
        .shapes
        .iter()
        .for_each(|c| walk(&c.shape, &mut texts));
    texts
}

/// A row now fills the width, and a centred label would read badly in a list.
#[test]
fn it_shows_its_title_and_row_text_starts_at_the_left() {
    let (mut app, ctx) = opened("new table");
    let window = window_rect(&ctx);
    let texts = painted_texts(&mut app, &ctx);

    assert!(
        texts.iter().any(|(t, _)| t == "Find a command"),
        "no title in {texts:?}"
    );
    let (_, row) = texts
        .iter()
        .find(|(t, _)| t.starts_with("New table"))
        .expect("the row's text");
    assert!(
        row.x < window.min.x + 40.0,
        "row text at {row:?} in {window:?}"
    );
}

#[test]
fn the_click_that_opens_it_from_the_help_menu_does_not_close_it() {
    let mut app = OctaApp::new(
        vec![],
        octa::ui::settings::AppSettings::default(),
        octa::ui::settings::IconVariant::Rose,
    );
    let ctx = egui::Context::default();
    step(&mut app, &ctx, pointer(OUTSIDE, true));
    app.open_command_search(&ctx);
    step(&mut app, &ctx, pointer(OUTSIDE, false));
    settle(&mut app, &ctx);
    assert!(app.command_search.is_some());
}

/// The box has a close button and can be minimised, so like any other dialog it
/// stays until it is closed.
#[test]
fn a_click_outside_leaves_it_open() {
    let (mut app, ctx) = opened("");
    click(&mut app, &ctx, OUTSIDE);
    assert!(app.command_search.is_some());
}

/// The user's report: rows were only as wide as their words, so a click beside
/// the words hit nothing at all.
#[test]
fn a_click_anywhere_on_a_row_runs_it() {
    let (_, ctx) = opened("new table");
    let window = window_rect(&ctx);
    let ran = (0..160).step_by(8).any(|dy| {
        let (mut app, ctx) = opened("new table");
        // Far to the right of the row's few words.
        click(
            &mut app,
            &ctx,
            egui::pos2(window.max.x - 40.0, window.min.y + dy as f32),
        );
        app.new_table_dialog.is_some()
    });
    assert!(ran);
}

/// With the SQL editor or the search box focused, focus went back to it and
/// `text_edit_focused` then swallowed every command opening a dialog.
#[test]
fn a_picked_command_runs_though_a_text_field_had_focus() {
    let (mut app, ctx) = opened("");
    app.command_search = None;
    step(&mut app, &ctx, vec![]);
    ctx.memory_mut(|m| m.request_focus(other_field()));
    step(&mut app, &ctx, vec![]);

    app.open_command_search(&ctx);
    app.command_search.as_mut().unwrap().query = "open url".into();
    settle(&mut app, &ctx);
    step(&mut app, &ctx, key(egui::Key::Enter));
    settle(&mut app, &ctx);

    assert!(app.command_search.is_none());
    assert!(app.open_url_dialog.is_some());
    assert!(
        ctx.memory(|m| m.has_focus(other_field())),
        "typing can go on where it was"
    );
}

#[test]
fn escape_gives_focus_back_to_the_field_it_came_from() {
    let (mut app, ctx) = opened("");
    app.command_search = None;
    step(&mut app, &ctx, vec![]);
    ctx.memory_mut(|m| m.request_focus(other_field()));
    step(&mut app, &ctx, vec![]);

    app.open_command_search(&ctx);
    settle(&mut app, &ctx);
    assert!(ctx.memory(|m| m.has_focus(field_id())));
    step(&mut app, &ctx, key(egui::Key::Escape));
    settle(&mut app, &ctx);

    assert!(app.command_search.is_none());
    assert!(ctx.memory(|m| m.has_focus(other_field())));
}

#[test]
fn its_title_bar_closes_minimises_and_maximises() {
    let (_, ctx) = opened("");
    let normal = window_rect(&ctx);
    let (mut closed, mut minimised, mut maximised) = (false, false, false);
    // Along the title bar, from the top right corner where the buttons sit.
    for dx in (8..130).step_by(8) {
        let (mut app, ctx) = opened("");
        let at = egui::pos2(normal.max.x - dx as f32, normal.min.y + 27.0);
        click(&mut app, &ctx, at);
        if app.command_search.is_none() {
            closed = true;
        } else {
            let rect = window_rect(&ctx);
            minimised |= rect.height() < normal.height() * 0.5;
            maximised |= rect.width() > normal.width() + 100.0;
        }
    }
    assert!(closed, "no close button");
    assert!(minimised, "no minimise button");
    assert!(maximised, "no maximise button");
}

#[test]
fn it_can_be_resized_by_dragging_its_corner() {
    let (mut app, ctx) = opened("");
    let before = window_rect(&ctx);
    let corner = before.max - egui::vec2(2.0, 2.0);
    let moved = corner + egui::vec2(120.0, 90.0);

    step(&mut app, &ctx, vec![egui::Event::PointerMoved(corner)]);
    step(&mut app, &ctx, pointer(corner, true));
    for t in 1..=6 {
        let at = corner + (moved - corner) * (t as f32 / 6.0);
        step(&mut app, &ctx, vec![egui::Event::PointerMoved(at)]);
    }
    step(&mut app, &ctx, pointer(moved, false));
    settle(&mut app, &ctx);

    let after = window_rect(&ctx);
    assert!(
        after.width() > before.width() + 60.0,
        "{before:?} {after:?}"
    );
    assert!(
        after.height() > before.height() + 40.0,
        "{before:?} {after:?}"
    );
}

#[test]
fn the_keyboard_drives_the_box_while_its_field_has_focus() {
    let (mut app, ctx) = opened("");
    step(&mut app, &ctx, key(egui::Key::ArrowDown));
    assert_eq!(app.command_search.as_ref().unwrap().selected, 1);
    step(&mut app, &ctx, key(egui::Key::ArrowUp));
    assert_eq!(app.command_search.as_ref().unwrap().selected, 0);
    step(&mut app, &ctx, key(egui::Key::Escape));
    assert!(app.command_search.is_none());
}

#[test]
fn text_and_enter_use_the_new_results_in_the_same_frame() {
    let (mut app, ctx) = opened("new table");
    let mut events = vec![egui::Event::Text("no-such-command-38".into())];
    events.extend(key(egui::Key::Enter));
    step(&mut app, &ctx, events);
    settle(&mut app, &ctx);
    assert!(app.new_table_dialog.is_none(), "Enter ran the old match");
    assert_eq!(
        app.command_search.as_ref().unwrap().query,
        "new tableno-such-command-38"
    );
    assert!(
        painted_texts(&mut app, &ctx)
            .iter()
            .any(|(t, _)| t == "No command matches.")
    );

    let (mut app, ctx) = opened("new tabl");
    let mut events = vec![egui::Event::Text("e".into())];
    events.extend(key(egui::Key::Enter));
    step(&mut app, &ctx, events);
    settle(&mut app, &ctx);
    assert!(
        app.new_table_dialog.is_some(),
        "Enter ignored the new match"
    );
}

#[test]
fn keyboard_scrolls_to_the_highlighted_result_and_typing_resets_the_list() {
    let (mut app, ctx) = opened("");
    for _ in 0..24 {
        step(&mut app, &ctx, key(egui::Key::ArrowDown));
    }
    let st = app.command_search.as_ref().unwrap();
    assert_eq!(st.selected, 24);
    assert!(
        st.scroll_y > 0.0,
        "the highlighted row is below the first screen"
    );
    let label = app.command_rows("")[st.selected].name.clone();
    assert!(
        painted_texts(&mut app, &ctx)
            .iter()
            .any(|(t, _)| t.starts_with(&label))
    );
    step(&mut app, &ctx, vec![egui::Event::Text("new table".into())]);
    let st = app.command_search.as_ref().unwrap();
    assert_eq!((st.selected, st.scroll_y), (0, 0.0));
    assert!(
        painted_texts(&mut app, &ctx)
            .iter()
            .any(|(t, _)| t.starts_with("New table"))
    );
}

#[test]
fn narrow_palette_keeps_the_window_and_shortcut_inside_the_viewport() {
    let (mut app, _) = opened("open file");
    // A fresh context exercises the initial default width on a narrow host.
    let ctx = egui::Context::default();
    app.command_search.as_mut().unwrap().focus_set = false;
    for width in [320.0, 375.0, 414.0, 768.0, 1440.0] {
        for _ in 0..3 {
            step_at_width(&mut app, &ctx, vec![], width);
        }
        let rect = window_rect(&ctx);
        assert!(
            rect.left() >= -1.0 && rect.right() <= width + 1.0,
            "{width}: {rect:?}"
        );
        let rows = app.command_rows("open file");
        let open = rows.iter().find(|row| row.action == SA::OpenFile).unwrap();
        let texts = painted_texts(&mut app, &ctx);
        let title = texts
            .iter()
            .find(|(t, _)| t.starts_with(&open.name))
            .expect("Open File row")
            .1;
        let shortcut = texts
            .iter()
            .find(|(t, _)| t == &open.keys)
            .expect("Shortcut text")
            .1;
        assert!(
            shortcut.x > title.x + 12.0,
            "{width}: {title:?} vs {shortcut:?}"
        );
        assert!(shortcut.x < rect.right());
    }
}

#[test]
fn a_long_command_keeps_a_full_width_button_and_accessible_name_at_large_text_size() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let row = Row {
        action: SA::OpenFile,
        hint_of: "command.open_file",
        name: "Vollständige Datenbankverbindungseinstellungen zur Arbeitsmappe öffnen".into(),
        place: "Datenbankverbindungen und Arbeitsmappen".into(),
        keys: "Ctrl+Shift+O".into(),
        why: None,
    };
    ctx.global_style_mut(|style| {
        style
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(26.0));
        style
            .text_styles
            .insert(egui::TextStyle::Small, egui::FontId::proportional(20.0));
    });
    let mut out = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(320.0, 240.0),
            )),
            ..Default::default()
        },
        |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let width = ui.available_width();
                ui.with_layout(egui::Layout::top_down_justified(egui::Align::Min), |ui| {
                    let style = RowStyle::new(ui);
                    let response = style.show(ui, &row, false);
                    assert!(
                        (response.rect.width() - width).abs() <= 2.0,
                        "{:?}",
                        response.rect
                    );
                    assert!(
                        (response.rect.height() - style.height).abs() <= 2.0,
                        "{:?}",
                        response.rect
                    );
                });
            });
        },
    );
    out.textures_delta.clear();
    let tree = out.platform_output.accesskit_update.unwrap();
    assert!(
        tree.nodes
            .iter()
            .any(|(_, n)| n.label().is_some_and(|label| label.contains(&row.name)))
    );
}

/// It can stay open now, so it must not eat the table's arrow keys meanwhile.
#[test]
fn keys_stay_with_the_app_once_the_box_lost_focus() {
    let (mut app, ctx) = opened("");
    click(&mut app, &ctx, OUTSIDE);
    assert!(!ctx.memory(|m| m.has_focus(field_id())));

    step(&mut app, &ctx, key(egui::Key::ArrowDown));
    assert_eq!(app.command_search.as_ref().unwrap().selected, 0);
    step(&mut app, &ctx, key(egui::Key::Enter));
    assert!(app.command_search.is_some(), "Enter ran a command");
    step(&mut app, &ctx, key(egui::Key::Escape));
    assert!(app.command_search.is_some(), "Escape closed it");
}

#[test]
fn opening_it_again_brings_it_back_with_what_was_typed() {
    let (mut app, ctx) = opened("new");
    ctx.data_mut(|d| d.insert_temp(size_key(), DialogSize::Minimized));
    click(&mut app, &ctx, OUTSIDE);
    assert!(!ctx.memory(|m| m.has_focus(field_id())));

    app.open_command_search(&ctx);
    settle(&mut app, &ctx);

    assert_eq!(app.command_search.as_ref().unwrap().query, "new");
    assert!(ctx.memory(|m| m.has_focus(field_id())));
    assert!(matches!(
        ctx.data(|d| d.get_temp::<DialogSize>(size_key())),
        Some(DialogSize::Normal)
    ));
}

/// A feature inside a dialog is found by its own name, in the active language
/// and in English, and says which command opens it.
#[test]
fn a_feature_inside_a_dialog_is_listed_under_its_own_name() {
    let (app, _) = opened("");
    for (query, action) in [
        ("seasonality", SA::OpenChart),
        ("pareto", SA::OpenChart),
        ("working days", SA::OpenTimeCalc),
        ("word pairs", SA::ColumnValueFrequency),
        ("neighbours", SA::OpenOutliers),
        ("abc class", SA::OpenCalculatedColumn),
        ("visit number", SA::OpenCalculatedColumn),
        ("previous row", SA::OpenCalculatedColumn),
        ("currency", SA::OpenReferenceData),
        ("inflation", SA::OpenReferenceData),
        ("postcode", SA::OpenReferenceData),
        ("NUTS", SA::OpenReferenceData),
        ("Kreisschlüssel", SA::OpenReferenceData),
    ] {
        let rows = app.command_rows(query);
        let row = rows
            .iter()
            .find(|r| r.action == action)
            .unwrap_or_else(|| panic!("{query} finds nothing"));
        assert_eq!(row.place, action.display_name(), "{query}");
    }
}

/// A typo in `INSIDE` would list the raw key as a command name.
#[test]
fn every_inside_name_is_a_real_key() {
    for &(key, _) in INSIDE {
        assert!(!t(key).contains('.'), "{key} does not resolve");
    }
}

/// Typing the name of a feature added on 2026-11-18 finds its command.
#[test]
fn fit_columns_and_tab_colour_are_found_by_name() {
    let (app, _) = opened("");
    for (query, action) in [
        ("fit columns", SA::FitSqlResultColumns),
        ("tab colour", SA::NextTabColour),
    ] {
        assert!(
            app.command_rows(query).iter().any(|r| r.action == action),
            "{query} finds nothing"
        );
    }
}

#[test]
fn fill_down_is_discoverable_and_runs_on_an_editable_visible_selection() {
    let (mut app, ctx) = opened("fill down values");
    let row = |app: &OctaApp| {
        app.command_rows("fill down values")
            .into_iter()
            .find(|row| row.action == SA::FillDown)
            .expect("Fill down values is findable")
    };
    assert!(row(&app).why.is_some());
    app.open_new_table_tab(1, 3);
    let tab = &mut app.tabs[app.active_tab];
    tab.table.rows[0][0] = octa::data::CellValue::Int(23);
    tab.table_state.selected_cells.extend([(0, 0), (2, 0)]);
    app.recompute_filter();
    assert!(row(&app).why.is_none());
    app.readonly_mode = true;
    assert!(row(&app).why.is_some());
    app.readonly_mode = false;
    settle(&mut app, &ctx);
    step(&mut app, &ctx, key(egui::Key::Enter));
    settle(&mut app, &ctx);
    let table = &app.tabs[app.active_tab].table;
    assert_eq!(table.get(1, 0), Some(&octa::data::CellValue::Null));
    assert_eq!(table.get(2, 0), Some(&octa::data::CellValue::Int(23)));
    assert_eq!(table.undo_stack.len(), 1);
    assert!(SA::FillDown.default_combo().key.is_none());
    assert_eq!(SA::DuplicateRow.default_combo().key, Some(egui::Key::D));
}

#[test]
fn fill_right_command_is_discoverable_and_keeps_reload_bound() {
    let (mut app, ctx) = opened("fill right values");
    let row = |app: &OctaApp| {
        app.command_rows("fill right values")
            .into_iter()
            .find(|item| item.action == SA::FillRight)
            .expect("Fill right values is findable")
    };
    assert!(row(&app).why.is_some());
    app.open_new_table_tab(3, 1);
    let tab = &mut app.tabs[app.active_tab];
    let value = octa::data::CellValue::Int(42);
    tab.table.rows[0][0] = value.clone();
    tab.table_state.selected_cells.extend([(0, 0), (0, 2)]);
    app.recompute_filter();
    assert!(row(&app).why.is_none());
    app.readonly_mode = true;
    assert!(row(&app).why.is_some());
    app.readonly_mode = false;
    settle(&mut app, &ctx);
    step(&mut app, &ctx, key(egui::Key::Enter));
    settle(&mut app, &ctx);
    let table = &app.tabs[app.active_tab].table;
    assert_eq!(table.get(0, 1), Some(&octa::data::CellValue::Null));
    assert_eq!(table.get(0, 2), Some(&value));
    assert_eq!(table.undo_stack.len(), 1);
    assert!(SA::FillRight.default_combo().key.is_none());
    assert_eq!(SA::ReloadFile.default_combo().key, Some(egui::Key::R));
}

#[test]
fn formula_examples_command_opens_the_picker_without_writing_cells() {
    let (mut app, ctx) = opened("formula examples");
    let row = |app: &OctaApp| {
        app.command_rows("formula examples")
            .into_iter()
            .find(|item| item.action == SA::OpenFormulaExamples)
            .expect("examples action must be findable")
    };
    assert!(row(&app).why.is_some());
    app.open_new_table_tab(2, 3);
    assert!(row(&app).why.is_none());
    app.readonly_mode = true;
    assert!(row(&app).why.is_some());
    app.readonly_mode = false;
    settle(&mut app, &ctx);
    step(&mut app, &ctx, key(egui::Key::Enter));
    settle(&mut app, &ctx);
    let tab = &app.tabs[app.active_tab];
    assert!(tab.show_add_column_dialog);
    assert!(tab.new_col_examples.is_some());
    assert_eq!(tab.table.col_count(), 2);
    assert!(tab.table.edits.is_empty());
    assert!(tab.table.undo_stack.is_empty());
}

#[test]
fn focus_guides_command_toggles_in_read_only_and_is_local_to_one_tab() {
    let (mut app, ctx) = opened("focus cell guides");
    let checked_command = |app: &OctaApp| {
        app.command_rows("focus cell guides")
            .into_iter()
            .find(|item| item.action == SA::ToggleFocusGuides)
            .expect("Focus cell guides command")
    };
    assert_eq!(checked_command(&app).why, Some(t("greyed.need_table")));
    assert_eq!(SA::ToggleFocusGuides.place_key(), "menu.view");
    assert!(SA::ToggleFocusGuides.default_combo().key.is_none());
    app.open_new_table_tab(3, 4);
    let focused_tab = app.active_tab;
    app.tabs[focused_tab].table_state.selected_cell = Some((2, 1));
    app.readonly_mode = true;
    assert!(checked_command(&app).why.is_none());
    settle(&mut app, &ctx);
    step(&mut app, &ctx, key(egui::Key::Enter));
    settle(&mut app, &ctx);
    let tab = &app.tabs[focused_tab];
    assert!(tab.table_state.focus_guides);
    assert_eq!(tab.table_state.selected_cell, Some((2, 1)));
    assert!(tab.table.edits.is_empty() && tab.table.undo_stack.is_empty());
    assert!(app.command_search.is_none());

    app.open_new_table_tab(3, 4);
    assert_ne!(app.active_tab, focused_tab);
    assert!(!app.tabs[app.active_tab].table_state.focus_guides);
    assert!(app.tabs[focused_tab].table_state.focus_guides);
    app.active_tab = focused_tab;
    app.open_command_search(&ctx);
    app.command_search.as_mut().unwrap().query = t("view_menu.focus_guides");
    settle(&mut app, &ctx);
    step(&mut app, &ctx, key(egui::Key::Enter));
    settle(&mut app, &ctx);
    assert!(!app.tabs[focused_tab].table_state.focus_guides);
}

#[test]
fn focus_guides_menu_dispatch_refuses_non_table_views_without_changing_state() {
    use egui::accesskit::Toggled;
    use octa::data::ViewMode;
    let (mut app, _) = opened("focus cell guides");
    app.command_search = None;
    app.open_new_table_tab(3, 4);
    let active = app.active_tab;
    app.readonly_mode = true;
    let frame = |app: &mut OctaApp, ctx: &egui::Context, events| {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400.0, 900.0),
                )),
                events,
                ..Default::default()
            },
            |ui| app.render_toolbar(ui),
        );
        output.textures_delta.clear();
        output
    };
    let position = |frame: &egui::FullOutput, name: &str| {
        frame.shapes.iter().find_map(|item| match &item.shape {
            egui::Shape::Text(text) if text.galley.text() == name => {
                Some(text.visual_bounding_rect().center())
            }
            _ => None,
        })
    };
    let label = t("view_menu.focus_guides");
    for (mode, expected_enabled, checked) in [
        (ViewMode::Raw, false, false),
        (ViewMode::Table, true, false),
        (ViewMode::Table, true, true),
    ] {
        app.tabs[active].view_mode = mode;
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        frame(&mut app, &ctx, vec![]);
        frame(&mut app, &ctx, vec![]);
        let toolbar = frame(&mut app, &ctx, vec![]);
        let Some(at) = position(&toolbar, &t("menu.view")) else {
            panic!("view menu button must be painted");
        };
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(at)]);
        frame(&mut app, &ctx, pointer(at, true));
        let released = frame(&mut app, &ctx, pointer(at, false));
        let laying_out = frame(&mut app, &ctx, vec![]);
        let settled = frame(&mut app, &ctx, vec![]);
        // The popup is inert in its first layout pass. AccessKit sends
        // deltas, so read the most recent node, not the first report.
        let check = [&settled, &laying_out, &released]
            .iter()
            .filter_map(|frame| frame.platform_output.accesskit_update.as_ref())
            .flat_map(|tree| &tree.nodes)
            .find(|(_, node)| node.label() == Some(label.as_str()))
            .expect("Focus cell guides checkbox present in View");
        assert_eq!(
            !check.1.is_disabled(),
            expected_enabled,
            "{mode:?}, checked {checked}"
        );
        assert_eq!(
            check.1.toggled(),
            Some(if checked {
                Toggled::True
            } else {
                Toggled::False
            })
        );
        let entry = position(&settled, &label).expect("Focus cell guides label painted");
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(entry)]);
        frame(&mut app, &ctx, pointer(entry, true));
        frame(&mut app, &ctx, pointer(entry, false));
        assert_eq!(
            app.tabs[active].table_state.focus_guides,
            expected_enabled && !checked
        );
        if !expected_enabled {
            // A disabled menu row must remain visible; dismiss the menu
            // before the next View state is tested.
            frame(&mut app, &ctx, key(egui::Key::Escape));
            assert_eq!(
                app.command_unavailable(SA::ToggleFocusGuides),
                Some(t("greyed.need_table_view"))
            );
        }
    }
    assert!(app.tabs[active].table.undo_stack.is_empty());
}

#[test]
fn keyboard_quick_filter_preserves_focus_and_only_replaces_the_active_column() {
    use octa::data::{CellValue as Cell, ColumnInfo};
    let (mut app, ctx) = opened("");
    app.command_search = None;
    app.readonly_mode = true;
    let tab = &mut app.tabs[0];
    tab.table.columns = ["item", "warehouse"]
        .map(|name| ColumnInfo {
            name: name.into(),
            data_type: "Utf8".into(),
        })
        .into();
    tab.table.rows = [
        ("A.01", "East"),
        ("A.01", "West"),
        ("A-01", "West"),
        ("A.01", "West"),
    ]
    .map(|(sku, warehouse)| vec![Cell::String(sku.into()), Cell::String(warehouse.into())])
    .into();
    let original = tab.table.rows.clone();
    tab.table_state.selected_cell = Some((1, 0));
    tab.column_filters
        .insert(0, ["A.01".into(), "A-01".into()].into());
    tab.column_filters.insert(1, ["West".into()].into());
    app.recompute_filter();
    assert_eq!(app.tabs[0].filtered_rows, [1, 2, 3]);
    ctx.memory_mut(|m| m.request_focus(other_field()));
    app.open_command_search(&ctx);
    let label = t("dialog.vf_filter_to_value");
    app.command_search.as_mut().unwrap().query = label.clone();
    settle(&mut app, &ctx);
    let rows = app.command_rows(&label);
    assert_eq!(rows[0].action, SA::FilterToCellValue);
    assert!(rows[0].why.is_none());
    step(&mut app, &ctx, key(egui::Key::Enter));
    settle(&mut app, &ctx);
    assert!(app.command_search.is_none());
    assert!(ctx.memory(|m| m.has_focus(other_field())));
    assert_eq!(app.tabs[0].column_filters[&0], ["A.01".into()].into());
    assert_eq!(app.tabs[0].column_filters[&1], ["West".into()].into());
    app.recompute_filter();
    assert_eq!(app.tabs[0].filtered_rows, [1, 3]);
    assert_eq!(app.tabs[0].table.rows, original);
    assert!(app.tabs[0].table.edits.is_empty() && app.tabs[0].table.undo_stack.is_empty());
    app.filter_to_selected_cell();
    assert!(
        !app.tabs[0].filter_dirty,
        "reapplying the same value must not repeat the filter"
    );
}

#[test]
fn quick_filter_does_not_use_a_hidden_or_non_table_selection() {
    use octa::data::ViewMode;
    let (mut app, _) = opened("");
    app.tabs[0] = crate::app::db_view::tests::db_tab();
    app.tabs[0].db_origin = None;
    app.recompute_filter();
    assert!(app.command_unavailable(SA::FilterToCellValue).is_some());
    app.tabs[0].table_state.selected_cell = Some((0, 1));
    app.tabs[0].hidden_columns.insert(1);
    assert!(app.command_unavailable(SA::FilterToCellValue).is_some());
    app.filter_to_selected_cell();
    assert!(app.tabs[0].column_filters.is_empty());
    app.tabs[0].hidden_columns.clear();
    app.tabs[0].filtered_rows.clear();
    assert!(app.command_unavailable(SA::FilterToCellValue).is_some());
    app.recompute_filter();
    assert!(app.command_unavailable(SA::FilterToCellValue).is_none());
    app.tabs[0].view_mode = ViewMode::Raw;
    assert_eq!(
        app.command_unavailable(SA::FilterToCellValue),
        Some(t("greyed.need_table_view"))
    );
    app.filter_to_selected_cell();
    assert!(app.tabs[0].column_filters.is_empty());
}

#[test]
fn database_quick_filter_uses_the_existing_exact_value_route_and_preserves_typed_fallback() {
    use octa::data::CellValue;
    use octa::db::pushdown::view::ViewFilter;
    let (mut app, _) = opened("");
    app.tabs[0] = crate::app::db_view::tests::db_tab();
    app.settings.db_connections = vec![crate::app::db_view::tests::conn()];
    app.settings.db_pushdown = true;
    app.tabs[0].table.total_rows = Some(10_000);
    app.tabs[0].table.rows[0][0] = CellValue::String("D'Azur".into());
    app.tabs[0].filtered_rows = vec![0];
    app.tabs[0].table_state.selected_cell = Some((0, 1));
    assert_eq!(
        app.command_unavailable(SA::FilterToCellValue),
        Some(t("facet.from_cell_list_hint"))
    );
    app.filter_to_selected_cell();
    assert!(app.tabs[0].column_filters.is_empty());
    app.tabs[0].table_state.selected_cell = Some((0, 0));
    app.filter_to_selected_cell();
    let (view, _) = crate::app::db_view::wanted_view(&app.tabs[0], false);
    assert_eq!(
        view.filters,
        vec![ViewFilter::Values {
            column: "name".into(),
            values: vec!["D'Azur".into()]
        }]
    );
    let condition = view.where_sql(octa::db::DbEngine::Postgres).unwrap();
    assert!(condition.contains("D''Azur"), "{condition}");
    assert!(app.tabs[0].table.edits.is_empty());
}

#[test]
fn keyboard_select_blanks_returns_focus_and_leaves_other_tabs_unchanged() {
    use octa::data::CellValue;
    let (mut app, ctx) = opened("");
    app.command_search = None;
    app.tabs[0] = crate::app::db_view::tests::db_tab();
    app.tabs[0].db_origin = None;
    app.tabs[0].table.rows[0][0] = CellValue::Null;
    let mut other = crate::app::db_view::tests::db_tab();
    other.db_origin = None;
    other.table_state.selected_cell = Some((0, 1));
    app.tabs.push(other);
    app.recompute_filter();
    ctx.memory_mut(|m| m.request_focus(other_field()));
    app.open_command_search(&ctx);
    let command = t("search_menu.select_blanks");
    app.command_search.as_mut().unwrap().query = command.clone();
    settle(&mut app, &ctx);
    assert_eq!(app.command_rows(&command)[0].action, SA::SelectBlanks);
    assert!(app.command_unavailable(SA::SelectBlanks).is_none());
    step(&mut app, &ctx, key(egui::Key::Enter));
    settle(&mut app, &ctx);
    assert!(app.command_search.is_none());
    assert!(ctx.memory(|m| m.has_focus(other_field())));
    assert_eq!(app.tabs[0].table_state.selected_cells, [(0, 0)].into());
    assert_eq!(app.tabs[1].table_state.selected_cell, Some((0, 1)));
    assert!(app.tabs[1].table_state.selected_cells.is_empty());
    assert!(app.tabs[0].table.edits.is_empty());
    app.tabs[0].view_mode = octa::data::ViewMode::Raw;
    assert_eq!(
        app.command_unavailable(SA::SelectBlanks),
        Some(t("greyed.need_table_view"))
    );
}

#[test]
fn fill_selection_command_opens_the_review_and_is_disabled_in_readonly() {
    let (mut app, ctx) = opened("");
    app.tabs[0] = crate::app::db_view::tests::db_tab();
    app.tabs[0].db_origin = None;
    app.tabs[0].table_state.selected_cell = Some((0, 0));
    app.recompute_filter();
    let label = t("edit_menu.fill_value");
    app.command_search.as_mut().unwrap().query = label.clone();
    settle(&mut app, &ctx);
    assert_eq!(app.command_rows(&label)[0].action, SA::FillSelectedValue);
    app.readonly_mode = true;
    assert!(app.command_unavailable(SA::FillSelectedValue).is_some());
    app.readonly_mode = false;
    step(&mut app, &ctx, key(egui::Key::Enter));
    settle(&mut app, &ctx);
    assert!(app.command_search.is_none());
    assert!(app.value_fill.is_some());
    assert!(app.tabs[0].table.edits.is_empty());
}
