use octa::data::value_frequency::ValueFrequencyRow;

use super::*;

const BAR: egui::Color32 = egui::Color32::from_rgb(231, 83, 199);
const WIDTH: f32 = 404.0;

fn make_report(rows: &[(&str, usize)], total: usize) -> ValueFrequency {
    ValueFrequency {
        column_name: "Artikel".into(),
        rows: rows
            .iter()
            .map(|(label, count)| ValueFrequencyRow {
                label: (*label).into(),
                count: *count,
            })
            .collect(),
        nulls: 0,
        total_non_null: total,
        unique_count: rows.len(),
        binned: false,
    }
}

fn paint(ctx: &egui::Context, mut display: impl FnMut(&mut egui::Ui)) -> egui::FullOutput {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(WIDTH, 238.0),
        )),
        ..Default::default()
    };
    let mut frame = ctx.run_ui(input, |ui| {
        ui.visuals_mut().selection.stroke.color = BAR;
        display(ui);
    });
    frame.textures_delta.clear();
    frame
}

fn text_at(frame: &egui::FullOutput, value: &str) -> Option<(egui::Pos2, f32)> {
    frame.shapes.iter().find_map(|shape| match &shape.shape {
        egui::Shape::Text(label) if label.galley.text() == value => {
            Some((label.pos, label.galley.size().x))
        }
        _ => None,
    })
}

fn bar_widths(frame: &egui::FullOutput) -> Vec<f32> {
    frame
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Rect(rect) if rect.fill == BAR && rect.rect.height() == 3.0 => {
                Some(rect.rect.width())
            }
            _ => None,
        })
        .collect()
}

#[test]
fn percentage_bars_use_the_full_count_and_keep_exact_text_on_a_narrow_dialog() {
    let mut freq = make_report(
        &[("Häufig", 50), ("Seltener", 25), ("Leere Klasse", 0)],
        100,
    );
    freq.binned = true;
    let ctx = egui::Context::default();
    let mut action = FrequencyActions::default();
    // First frame calculates widths; check the stable viewport.
    paint(&ctx, |ui| {
        draw_frequency_grid(ui, &freq, false, &mut action);
    });
    let rendered = paint(&ctx, |ui| {
        draw_frequency_grid(ui, &freq, false, &mut action);
    });
    let bars = bar_widths(&rendered);
    assert_eq!(
        bars.len(),
        2,
        "zero-count bin must have a track but no fill: {bars:?}"
    );
    assert!((bars[0] - 2.0 * bars[1]).abs() < 0.1, "{bars:?}");
    for label in ["50.0%", "25.0%", "0.0%"] {
        let (at, width) = text_at(&rendered, label).unwrap();
        assert!(at.x > 0.0 && at.x + width < WIDTH, "{label} at {at:?}");
    }

    // Top N uses the original 100 counted rows; trimming the report to just
    // its largest value must not turn its 50% bar into a 100% bar.
    freq.rows.truncate(1);
    let top = paint(&ctx, |ui| {
        draw_frequency_grid(ui, &freq, false, &mut action);
    });
    assert_eq!(bar_widths(&top), vec![bars[0]]);
    assert!(text_at(&top, "50.0%").is_some());
}

#[test]
fn all_frequencies_scroll_to_the_last_of_5000_rows_with_sticky_headers() {
    let mut freq = make_report(&[], 5_000);
    freq.rows = (0..5_000)
        .map(|index| ValueFrequencyRow {
            label: format!("VF-{index:04}"),
            count: 1,
        })
        .collect();
    freq.unique_count = freq.rows.len();
    let ctx = egui::Context::default();
    let mut action = FrequencyActions::default();
    let mut info = None;
    let first = paint(&ctx, |ui| {
        let body = draw_frequency_grid(ui, &freq, false, &mut action);
        info = Some((
            body.id,
            body.state,
            body.content_size.y - body.inner_rect.height(),
        ));
    });
    let (id, mut state, bottom) = info.unwrap();
    assert!(bottom > 90_000.0);
    assert!(text_at(&first, "VF-0000").is_some());
    assert!(text_at(&first, "VF-4999").is_none());
    let initial_paint_count = first.shapes.len();
    assert!(
        initial_paint_count < 800,
        "viewport rendered {initial_paint_count} shapes"
    );
    state.offset.y = bottom;
    state.store(&ctx, id);
    let last = paint(&ctx, |ui| {
        draw_frequency_grid(ui, &freq, false, &mut action);
    });
    assert!(text_at(&last, "VF-0000").is_none());
    assert!(text_at(&last, "VF-4999").is_some());
    assert_eq!(
        text_at(&first, "%").unwrap().0.y,
        text_at(&last, "%").unwrap().0.y
    );
}

#[test]
fn tsv_keeps_the_report_scope_and_quotes_multiline_values_and_headers() {
    let value = "Nord\tTor\r\nFreigabe \"B\"";
    let name = "Artikel\tCode";
    let freq = make_report(&[(value, 37), ("0007", 13)], 100);
    let tsv = build_tsv(name, &freq).unwrap();
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(b'\t')
        .from_reader(tsv.as_bytes());
    assert_eq!(
        reader.headers().unwrap().iter().collect::<Vec<_>>(),
        [name, "count", "percent"]
    );
    let rows = reader.records().collect::<Result<Vec<_>, _>>().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].iter().collect::<Vec<_>>(), [value, "37", "37.0"]);
    assert_eq!(rows[1].iter().collect::<Vec<_>>(), ["0007", "13", "13.0"]);
}

#[test]
fn detached_report_keeps_source_percentages_and_cumulative_top_n() {
    let freq = make_report(&[("0007", 50), ("Zulu", 30)], 100);
    let result = frequency_result_table(&freq, false).unwrap();
    assert_eq!(
        result
            .columns
            .iter()
            .map(|c| (c.name.as_str(), c.data_type.as_str()))
            .collect::<Vec<_>>(),
        [
            ("value", "Utf8"),
            ("count", "Int64"),
            ("percent", "Float64"),
            ("cumulative_percent", "Float64")
        ]
    );
    assert_eq!(result.rows.len(), 2);
    assert_eq!(
        result.rows[0],
        [
            CellValue::String("0007".into()),
            CellValue::Int(50),
            CellValue::Float(50.0),
            CellValue::Float(50.0)
        ]
    );
    assert_eq!(result.rows[1][2], CellValue::Float(30.0));
    assert_eq!(result.rows[1][3], CellValue::Float(80.0));
    assert!(result.source_path.is_none());
    assert!(result.db_meta.is_none());
}

#[test]
fn bins_and_overlapping_words_do_not_claim_a_cumulative_share() {
    let mut report = make_report(&[("warning", 8), ("delay", 5)], 10);
    let words = frequency_result_table(&report, true).unwrap();
    assert_eq!(words.col_count(), 3);
    assert_eq!(words.rows[0][2], CellValue::Float(80.0));
    assert_eq!(words.rows[1][2], CellValue::Float(50.0));
    report.binned = true;
    report.rows[0].label = "[2, 3)".into();
    let bins = frequency_result_table(&report, false).unwrap();
    assert_eq!(bins.col_count(), 3);
    assert_eq!(bins.rows[0][0], CellValue::String("[2, 3)".into()));
}

#[test]
fn frequency_table_roundtrips_to_a_typed_workbook() {
    use octa::formats::{FormatReader, excel_reader::ExcelReader};
    let report = make_report(&[("0007", 4), ("Tab\tline\n\"X\"", 2)], 8);
    let table = frequency_result_table(&report, false).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("frequency-report.xlsx");
    ExcelReader.write_file(&path, &table).unwrap();
    let next = ExcelReader.read_file(&path).unwrap();
    assert_eq!(next.rows.len(), 2);
    assert_eq!(next.get(0, 0), Some(&CellValue::String("0007".into())));
    assert_eq!(
        next.get(1, 0),
        Some(&CellValue::String("Tab\tline\n\"X\"".into()))
    );
    assert_eq!(next.get(0, 1), Some(&CellValue::Float(4.0)));
    assert_eq!(next.get(1, 2), Some(&CellValue::Float(25.0)));
    assert_eq!(next.get(1, 3), Some(&CellValue::Float(75.0)));
}

fn report_frame(
    ctx: &egui::Context,
    events: Vec<egui::Event>,
    render: impl FnMut(&mut egui::Ui),
) -> egui::FullOutput {
    let mut frame = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000.0, 720.0),
            )),
            events,
            ..Default::default()
        },
        render,
    );
    frame.textures_delta.clear();
    frame
}

#[test]
fn open_report_button_is_keyboard_activatable_but_disabled_for_zero_rows() {
    let ctx = egui::Context::default();
    let mut activated = false;
    let frame = report_frame(&ctx, Vec::new(), |ui| {
        let zero = frequency_result_button(ui, 0);
        assert!(!zero.enabled());
        let two = frequency_result_button(ui, 2);
        assert!(two.enabled());
        two.request_focus();
    });
    let label = format!("{} (2)", octa::i18n::t("dialog.vf_result_tab"));
    assert!(text_at(&frame, &label).is_some());
    assert!(result_scope_label(2).contains('2'));
    report_frame(
        &ctx,
        vec![egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
        |ui| {
            assert!(!frequency_result_button(ui, 0).clicked());
            activated |= frequency_result_button(ui, 2).clicked();
        },
    );
    assert!(activated);
}

fn report_app() -> OctaApp {
    use octa::ui::settings::{AppSettings, IconVariant};
    let mut app = OctaApp::new(vec![], AppSettings::default(), IconVariant::Rose);
    let tab = &mut app.tabs[0];
    tab.custom_tab_label = Some("Inventur".into());
    tab.table = DataTable {
        columns: vec![ColumnInfo {
            name: "Artikel".into(),
            data_type: "Utf8".into(),
        }],
        rows: ["0007", "0007", "0007", "0010", "0010", "0014", "0011"]
            .into_iter()
            .map(|code| vec![CellValue::String(code.into())])
            .collect(),
        source_path: Some("/fixture/inventory.csv".into()),
        ..DataTable::empty()
    };
    tab.table.set(5, 0, CellValue::String("0007".into()));
    tab.column_filters
        .insert(0, std::collections::HashSet::from(["0010".into()]));
    tab.value_frequency_col = Some(0);
    tab.value_frequency_top_n = Some(2);
    app
}

#[test]
fn activating_the_report_opens_a_separate_tab_and_keeps_the_source_intact() {
    let mut app = report_app();
    let original_id = app.tabs[0].id;
    let ctx = egui::Context::default();
    let draw = |app: &mut OctaApp, ui: &mut egui::Ui| render_value_frequency_dialog(app, ui.ctx());
    report_frame(&ctx, Vec::new(), |ui| draw(&mut app, ui));
    let initial = report_frame(&ctx, Vec::new(), |ui| draw(&mut app, ui));
    let label = format!("{} (2)", octa::i18n::t("dialog.vf_result_tab"));
    let (start, width) =
        text_at(&initial, &label).expect("result button visible within the dialog");
    let at = start + egui::vec2(width * 0.5, 6.0);
    let pointer = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    report_frame(&ctx, vec![egui::Event::PointerMoved(at)], |ui| {
        draw(&mut app, ui)
    });
    report_frame(&ctx, vec![pointer(true), pointer(false)], |ui| {
        draw(&mut app, ui)
    });
    assert_eq!(app.tabs.len(), 2);
    assert_eq!(app.active_tab, 1);
    assert_eq!(app.tabs[0].id, original_id);
    assert_eq!(
        app.tabs[0].table.source_path.as_deref(),
        Some("/fixture/inventory.csv")
    );
    assert_eq!(app.tabs[0].table.edits.len(), 1);
    assert_eq!(app.tabs[0].table.undo_stack.len(), 1);
    assert!(app.tabs[0].column_filters[&0].contains("0010"));
    assert_eq!(app.tabs[0].value_frequency_top_n, Some(2));
    assert_eq!(app.tabs[0].value_frequency_col, None);
    assert!(app.tabs[1].table.source_path.is_none());
    assert_eq!(
        app.tabs[1].table.get(0, 0),
        Some(&CellValue::String("0007".into()))
    );
    assert_eq!(app.tabs[1].table.get(0, 1), Some(&CellValue::Int(4)));
    assert_eq!(app.tabs[1].table.row_count(), 2);
    assert_eq!(
        app.tabs[1].table.get(1, 3),
        Some(&CellValue::Float(6.0 / 7.0 * 100.0))
    );
    app.tabs[1]
        .table
        .set(0, 0, CellValue::String("changed result only".into()));
    assert_eq!(
        app.tabs[0].table.get(0, 0),
        Some(&CellValue::String("0007".into()))
    );
}

#[test]
fn result_preserves_whether_counts_are_partial_or_from_a_server() {
    let mut app = report_app();
    app.tabs[0].table.total_rows = Some(800);
    let mut snapshot = FrequencySnapshot {
        report: Arc::new(make_report(&[("0010", 2)], 7)),
        by_words: false,
        server_total: None,
    };
    let local = frequency_result_tab(&app, 0, &snapshot).unwrap();
    assert_eq!(
        local.parse_error_banner,
        Some(octa::i18n::t("cohort.loaded_note"))
    );
    snapshot.server_total = Some(800);
    let remote = frequency_result_tab(&app, 0, &snapshot).unwrap();
    assert_eq!(
        remote.parse_error_banner,
        Some(octa::i18n::t("pushdown.note_unsaved"))
    );
    assert!(remote.tab_hint.as_deref().unwrap().contains("800"));
    assert!(remote.db_origin.is_none());
}
