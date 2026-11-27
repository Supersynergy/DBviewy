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
