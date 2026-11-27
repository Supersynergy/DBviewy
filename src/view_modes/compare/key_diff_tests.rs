use super::*;
use octa::data::{CellValue, ColumnInfo};

fn table() -> DataTable {
    let mut t = DataTable::empty();
    t.columns = vec![ColumnInfo {
        name: "a".into(),
        data_type: "Utf8".into(),
    }];
    t.rows = vec![vec![CellValue::String("x".into())]];
    t
}

fn cache(key: KeyDiffKey, pass: u64) -> KeyDiffCache {
    KeyDiffCache {
        key,
        pass,
        outcome: Err(String::new()),
    }
}

fn key(t: &DataTable, join: bool) -> KeyDiffKey {
    KeyDiffKey {
        join,
        keys: vec![],
        options: CompareOptions::default(),
        left: generation(t),
    }
}

#[test]
fn same_shape_tables_have_different_generations() {
    let (a, b) = (table(), table());
    assert_ne!(generation(&a), generation(&b));
}

#[test]
fn cache_counts_only_when_used_on_the_previous_pass() {
    let t = table();
    let c = cache(key(&t, false), 10);
    assert!(reusable(&c, &key(&t, false), 11));
    assert!(!reusable(&c, &key(&t, false), 12));
    assert!(!reusable(&c, &key(&t, true), 11));
}

fn many_changes() -> DataTable {
    let mut table = table();
    table.columns[0].name = "Record".into();
    table.rows = (0..5_000)
        .map(|i| vec![CellValue::String(format!("D{i:04}"))])
        .collect();
    table
}

fn frame_input(events: Vec<egui::Event>) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(760.0, 310.0),
        )),
        events,
        ..Default::default()
    }
}

fn render_frame(
    ctx: &egui::Context,
    events: Vec<egui::Event>,
    draw: impl FnMut(&mut egui::Ui),
) -> egui::FullOutput {
    let mut out = ctx.run_ui(frame_input(events), draw);
    // No graphics backend in this test, so acknowledge the font atlas updates.
    out.textures_delta.clear();
    out
}

fn text_y(frame: &egui::FullOutput, text: &str) -> Option<f32> {
    frame.shapes.iter().find_map(|shape| {
        let egui::Shape::Text(label) = &shape.shape else {
            return None;
        };
        (label.galley.text() == text).then_some(label.pos.y)
    })
}

#[test]
fn scrolled_compare_keeps_its_header_and_draws_the_last_of_5000_rows() {
    let left = many_changes();
    let right = DataTable::empty();
    let change = compare_ordered(&left, &right, &CompareOptions::default());
    let rows = CompareOutput::new(&left, &right, &change, false);
    let ctx = egui::Context::default();
    let colors = ui::theme::ThemeColors::for_mode(ThemeMode::Light);
    let mut info = None;
    let first = render_frame(&ctx, Vec::new(), |ui| {
        let grid = draw_result_grid(ui, &rows, &colors);
        info = Some((
            grid.id,
            grid.state,
            grid.content_size.y - grid.inner_rect.height(),
        ));
    });
    let (id, mut state, bottom) = info.unwrap();
    assert!(text_y(&first, "D0000").is_some());
    assert!(text_y(&first, "D4999").is_none());
    assert!(bottom > 80_000.0);
    state.offset.y = bottom;
    state.store(&ctx, id);
    let last = render_frame(&ctx, Vec::new(), |ui| {
        draw_result_grid(ui, &rows, &colors);
    });
    assert_eq!(
        text_y(&first, "Record").unwrap(),
        text_y(&last, "Record").unwrap()
    );
    assert!(text_y(&last, "D0000").is_none());
    assert!(text_y(&last, "D4999").is_some());
}

#[test]
fn compare_copy_event_outputs_all_rows_only_when_requested() {
    let left = many_changes();
    let right = DataTable::empty();
    let change = compare_ordered(&left, &right, &CompareOptions::default());
    let rows = CompareOutput::new(&left, &right, &change, false);
    let ctx = egui::Context::default();
    let colors = ui::theme::ThemeColors::for_mode(ThemeMode::Light);
    let draw = |ui: &mut egui::Ui| draw_result_table(ui, &rows, &colors);
    let point = egui::pos2(140.0, 100.0);
    let _ = render_frame(&ctx, Vec::new(), draw);
    let idle = render_frame(&ctx, vec![egui::Event::PointerMoved(point)], draw);
    assert!(
        idle.platform_output
            .commands
            .iter()
            .all(|cmd| !matches!(cmd, egui::OutputCommand::CopyText(_)))
    );
    let requested = render_frame(&ctx, vec![egui::Event::Copy], draw);
    let payloads = requested
        .platform_output
        .commands
        .iter()
        .filter_map(|cmd| {
            if let egui::OutputCommand::CopyText(text) = cmd {
                Some(text)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(payloads.len(), 1);
    let tsv = payloads[0];
    assert_eq!(tsv.lines().count(), 5001);
    assert_eq!(tsv.lines().last(), Some("only_in_a\t\tD4999"));
}
