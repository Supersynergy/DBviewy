//! The chart view's buttons, clicked the way a user clicks them.

use super::*;
use octa::data::{CellValue, ColumnInfo, DataTable};

/// Two years of monthly sales, as a line chart tab.
fn app() -> OctaApp {
    let mut app = OctaApp::new(
        vec![],
        octa::ui::settings::AppSettings::default(),
        octa::ui::settings::IconVariant::Rose,
    );
    let mut table = DataTable::empty();
    table.columns = vec![
        ColumnInfo {
            name: "month".into(),
            data_type: "Date32".into(),
        },
        ColumnInfo {
            name: "sales".into(),
            data_type: "Float64".into(),
        },
    ];
    table.rows = (0..24)
        .map(|i| {
            let summer = if i % 12 == 5 || i % 12 == 6 {
                10.0
            } else {
                0.0
            };
            vec![
                CellValue::Date(format!("{}-{:02}-01", 2024 + i / 12, i % 12 + 1)),
                CellValue::Float(100.0 + i as f64 + summer),
            ]
        })
        .collect();
    let tab = &mut app.tabs[0];
    tab.filtered_rows = (0..table.row_count()).collect();
    tab.table = table;
    tab.is_chart_tab = true;
    tab.view_mode = ViewMode::Chart;
    tab.chart_config = octa::data::chart::ChartConfig {
        kind: octa::data::chart::ChartKind::Line,
        x_col: Some(0),
        y_cols: vec![1],
        ..Default::default()
    };
    app
}

/// One frame of the active tab's view; every text painted, with its rect.
fn frame(
    app: &mut OctaApp,
    ctx: &egui::Context,
    events: Vec<egui::Event>,
) -> Vec<(String, egui::Rect)> {
    fn walk(shape: &egui::Shape, out: &mut Vec<(String, egui::Rect)>) {
        match shape {
            egui::Shape::Text(t) => out.push((
                t.galley.text().to_string(),
                t.galley.rect.translate(t.pos.to_vec2()),
            )),
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
            _ => {}
        }
    }
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1400.0, 900.0),
        )),
        events,
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |ui| {
        let ctx = ui.ctx().clone();
        if !app.render_non_table_view(&ctx, ui) {
            app.render_table_view(&ctx, ui);
        }
    });
    output.textures_delta.clear();
    let mut out = Vec::new();
    output.shapes.iter().for_each(|s| walk(&s.shape, &mut out));
    out
}

#[test]
fn seasonality_opens_the_numbers_and_a_chart_of_them() {
    let mut app = app();
    let ctx = egui::Context::default();
    frame(&mut app, &ctx, vec![]);
    let texts = frame(&mut app, &ctx, vec![]);
    let label = octa::i18n::t("chart.seasonality_to_table");
    let (_, rect) = texts
        .iter()
        .find(|(t, _)| *t == label)
        .unwrap_or_else(|| panic!("no {label} button: {texts:?}"));
    let at = rect.center();
    for pressed in [true, false] {
        let events = vec![
            egui::Event::PointerMoved(at),
            egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ];
        frame(&mut app, &ctx, events);
    }

    assert_eq!(app.tabs.len(), 3, "the chart, the numbers, their chart");
    let numbers = &app.tabs[1];
    assert!(!numbers.is_chart_tab, "a table that can be read and saved");
    assert_eq!(numbers.table.row_count(), 24);
    let names: Vec<&str> = numbers
        .table
        .columns
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(names, ["x", "value", "trend", "seasonal", "rest"]);
    let chart = &app.tabs[2];
    assert!(chart.is_chart_tab, "a real chart tab, which keeps its view");
    assert_eq!(chart.available_view_modes(), [ViewMode::Chart]);
    assert_eq!(chart.chart_config.y_cols, [1, 2, 3, 4]);
    assert_eq!(app.active_tab, 2);
}

#[test]
fn hidden_column_buttons_restore_only_the_clicked_column() {
    let mut app = app();
    let tab = &mut app.tabs[0];
    tab.is_chart_tab = false;
    tab.view_mode = ViewMode::Table;
    tab.table.columns.push(ColumnInfo {
        name: "region".into(),
        data_type: "Utf8".into(),
    });
    for row in &mut tab.table.rows {
        row.push(CellValue::String("Berlin".into()));
    }
    tab.hidden_columns.extend([0, 2]);
    let ctx = egui::Context::default();
    frame(&mut app, &ctx, vec![]);
    for (index, name) in [(2, "region"), (0, "month")] {
        let label = format!("＋ {} · {}", index + 1, name);
        let texts = frame(&mut app, &ctx, vec![]);
        let at = texts
            .iter()
            .find(|(text, _)| *text == label)
            .expect("restore button")
            .1
            .center();
        for pressed in [true, false] {
            frame(
                &mut app,
                &ctx,
                vec![
                    egui::Event::PointerMoved(at),
                    egui::Event::PointerButton {
                        pos: at,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
            );
        }
        assert!(!app.tabs[0].hidden_columns.contains(&index));
        assert_eq!(app.tabs[0].hidden_columns.contains(&0), index == 2);
    }
    assert!(
        !frame(&mut app, &ctx, vec![])
            .iter()
            .any(|(text, _)| text.starts_with("＋ "))
    );
}

#[test]
fn entering_a_target_line_in_the_chart_leaves_source_and_forecast_settings_alone() {
    use octa::i18n::t;

    let mut app = app();
    let ctx = egui::Context::default();
    // The headless driver has no vsync or real-time animation frames.
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    let render = |app: &mut OctaApp| frame(app, &ctx, vec![]);
    let click = |app: &mut OctaApp, at: egui::Pos2| {
        for pressed in [true, false] {
            frame(
                app,
                &ctx,
                vec![
                    egui::Event::PointerMoved(at),
                    egui::Event::PointerButton {
                        pos: at,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
    };
    render(&mut app);
    let texts = render(&mut app);
    let (_, customize) = texts
        .iter()
        .find(|(text, _)| *text == t("chart.customise"))
        .unwrap();
    click(&mut app, customize.center());
    render(&mut app);
    let texts = render(&mut app);
    let (_, label) = texts
        .iter()
        .find(|(text, _)| *text == t("chart.target_line"))
        .unwrap_or_else(|| panic!("Target control is missing from expanded chart: {texts:?}"));
    let (_, field) = texts
        .iter()
        .find(|(text, rect)| {
            *text == t("chart.placeholder_none")
                && (rect.center().y - label.center().y).abs() < 12.0
                && rect.center().x > label.center().x
        })
        .expect("target value input on the label row");
    click(&mut app, field.center());
    frame(&mut app, &ctx, vec![egui::Event::Text("120,5".into())]);
    let shown = render(&mut app);
    let expected = format!("{}: 120,5", t("chart.target_line"));
    assert!(shown.iter().any(|(text, _)| *text == expected));
    assert_eq!(app.tabs.len(), 1);
    assert_eq!(app.tabs[0].chart_buffers.target_line, "120,5");
    assert_eq!(app.tabs[0].table.col_count(), 2);
    assert!(app.tabs[0].table.edits.is_empty());
    assert_eq!(app.tabs[0].chart_config.forecast_periods, 0);

    app.tabs[0].chart_config.y_log_scale = true;
    let hidden = render(&mut app);
    assert!(!hidden.iter().any(|(text, _)| *text == expected));
    app.tabs[0].chart_config.y_log_scale = false;
    let shown_again = render(&mut app);
    assert!(shown_again.iter().any(|(text, _)| *text == expected));
}
