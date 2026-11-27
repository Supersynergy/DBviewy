use super::*;
use octa::data::{ColumnInfo, SearchMode};

fn sheet(names: &[&str], rows: &[&[&str]]) -> DataTable {
    DataTable {
        columns: names
            .iter()
            .map(|name| ColumnInfo {
                name: (*name).into(),
                data_type: "Utf8".into(),
            })
            .collect(),
        rows: rows
            .iter()
            .map(|fields| {
                fields
                    .iter()
                    .map(|cell| CellValue::String((*cell).into()))
                    .collect()
            })
            .collect(),
        ..DataTable::empty()
    }
}

fn frame(
    ctx: &egui::Context,
    events: Vec<egui::Event>,
    draw: impl FnMut(&mut egui::Ui),
) -> egui::FullOutput {
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(780.0, 520.0),
            )),
            events,
            ..Default::default()
        },
        draw,
    );
    output.textures_delta.clear();
    output
}

fn text_location(frame: &egui::FullOutput, word: &str) -> Option<egui::Pos2> {
    frame.shapes.iter().find_map(|shape| match &shape.shape {
        egui::Shape::Text(text) if text.galley.text() == word => Some(text.pos),
        _ => None,
    })
}

fn click_at(point: egui::Pos2) -> Vec<egui::Event> {
    let button = |pressed| egui::Event::PointerButton {
        pos: point,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    vec![
        egui::Event::PointerMoved(point),
        button(true),
        button(false),
    ]
}

#[test]
fn hash_column_rules_keep_implicit_all_and_count_only_loaded_columns() {
    let a = sheet(&["part", "region"], &[]);
    let b = sheet(&["owner", "area", "piece"], &[]);
    assert_eq!(comparison_widths(&a, &b, &[], &[]), (2, 3));
    let mut right = vec![2, 99, 1];
    clean_picks(&b, &mut right);
    assert_eq!(right, [2, 1]);
    assert_eq!(comparison_widths(&a, &b, &[], &right), (2, 2));
}

#[test]
fn raw_digest_sort_is_deterministic_and_keeps_every_shared_occurrence() {
    let a = sheet(
        &["part", "region"],
        &[&["a", "east"], &["a", "east"], &["b", "west"]],
    );
    let b = sheet(
        &["area", "piece"],
        &[&["east", "a"], &["north", "x"], &["south", "y"]],
    );
    let left = collect_hash_rows(&a, &[0, 1]);
    let right = collect_hash_rows(&b, &[1, 0]);
    let groups = separate_groups(&left, &right);
    assert_eq!(groups.shared.len(), 1);
    assert_eq!(groups.shared[0].1, [0, 1]);
    assert_eq!(groups.shared[0].2, [0]);
    assert_eq!(groups.left_only[0].1, [2]);
    assert_eq!(groups.right_only.len(), 2);
    assert!(
        groups
            .right_only
            .windows(2)
            .all(|pair| pair[0].0 < pair[1].0)
    );
    let reverted = collect_hash_rows(&b, &[0, 1]);
    assert!(separate_groups(&left, &reverted).shared.is_empty());

    let mut first = [4; 32];
    first[31] = 1;
    let mut second = first;
    second[31] = 2;
    assert_eq!(short_hex(&first), short_hex(&second));
    let tied = HashMap::from([(second, vec![1]), (first, vec![0])]);
    let empty = HashMap::new();
    let order = separate_groups(&tied, &empty);
    assert_eq!(*order.left_only[0].0, first);
    assert_eq!(*order.left_only[1].0, second);
}

#[test]
fn column_numbers_follow_click_order_and_space_preserves_checkbox_focus() {
    let table = sheet(&["SKU", "Depot", "Supplier"], &[]);
    let colors = ui::theme::ThemeColors::for_mode(ThemeMode::Light);
    let ctx = egui::Context::default();
    let mut selection = Vec::new();
    let before = frame(&ctx, Vec::new(), |ui| {
        draw_column_picker(ui, "test_side", "Left", &table, &mut selection, &colors);
    });
    let depot = text_location(&before, "2  Depot").unwrap() + egui::vec2(10.0, 7.0);
    frame(&ctx, click_at(depot), |ui| {
        draw_column_picker(ui, "test_side", "Left", &table, &mut selection, &colors);
    });
    assert_eq!(selection, [1]);
    let chosen = frame(&ctx, Vec::new(), |ui| {
        draw_column_picker(ui, "test_side", "Left", &table, &mut selection, &colors);
    });
    assert!(text_location(&chosen, "1  Depot").is_some());
    let sku = text_location(&chosen, "SKU").unwrap() + egui::vec2(10.0, 7.0);
    frame(&ctx, click_at(sku), |ui| {
        draw_column_picker(ui, "test_side", "Left", &table, &mut selection, &colors);
    });
    assert_eq!(selection, [1, 0]);
    let focused = ctx.memory(|memory| memory.focused()).unwrap();
    let numbered = frame(&ctx, Vec::new(), |ui| {
        draw_column_picker(ui, "test_side", "Left", &table, &mut selection, &colors);
    });
    assert!(text_location(&numbered, "2  SKU").is_some());
    frame(
        &ctx,
        vec![egui::Event::Key {
            key: egui::Key::Space,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
        |ui| {
            draw_column_picker(ui, "test_side", "Left", &table, &mut selection, &colors);
        },
    );
    assert_eq!(selection, [1]);
    assert_eq!(ctx.memory(|memory| memory.focused()), Some(focused));
}

#[test]
fn mismatched_selection_explains_the_counts_and_recovers_on_pairing() {
    let mut tab = TabState::new(SearchMode::Plain);
    tab.table = sheet(&["part", "region"], &[&["a", "east"]]);
    tab.compare_right_table = Some(Box::new(sheet(
        &["owner", "id", "depot"],
        &[&["z", "a", "east"]],
    )));
    let ctx = egui::Context::default();
    let broken = frame(&ctx, Vec::new(), |ui| {
        render(ui, &mut tab, ThemeMode::Light)
    });
    let warning = t("views.rowdiff_pair_count")
        .replace("{left}", "2")
        .replace("{right}", "3");
    assert!(text_location(&broken, &warning).is_some());
    let ready = t("views.rowdiff_summary")
        .replace("{left}", "1")
        .replace("{right}", "1")
        .replace("{shared}", "1")
        .replace("{left_only}", "0")
        .replace("{right_only}", "0");
    assert!(text_location(&broken, &ready).is_none());
    tab.compare_columns_right = vec![1, 2];
    let fixed = frame(&ctx, Vec::new(), |ui| {
        render(ui, &mut tab, ThemeMode::Light)
    });
    assert!(text_location(&fixed, &warning).is_none());
    assert!(text_location(&fixed, &ready).is_some());
}

#[test]
fn clipboard_uses_single_quoted_fields_and_caps_only_the_preview() {
    let exact = "north\t7\r\n\"A\"";
    let mut tab = TabState::new(SearchMode::Plain);
    tab.table = sheet(
        &["key", "detail"],
        &[&["common", "unchanged"], &["left", exact]],
    );
    tab.compare_right_table = Some(Box::new(sheet(
        &["key", "detail"],
        &[&["common", "unchanged"]],
    )));
    let ctx = egui::Context::default();
    frame(&ctx, Vec::new(), |ui| {
        render(ui, &mut tab, ThemeMode::Light)
    });
    let point = egui::pos2(180.0, 110.0);
    let idle = frame(&ctx, vec![egui::Event::PointerMoved(point)], |ui| {
        render(ui, &mut tab, ThemeMode::Light)
    });
    assert!(
        !idle
            .platform_output
            .commands
            .iter()
            .any(|cmd| matches!(cmd, egui::OutputCommand::CopyText(_)))
    );
    let output = frame(&ctx, vec![egui::Event::Copy], |ui| {
        render(ui, &mut tab, ThemeMode::Light)
    });
    let text = output
        .platform_output
        .commands
        .iter()
        .find_map(|cmd| {
            if let egui::OutputCommand::CopyText(text) = cmd {
                Some(text)
            } else {
                None
            }
        })
        .expect("copy reaches the result preview");
    let (title, tsv) = text.split_once('\n').unwrap();
    assert_eq!(title, format!("# {}", t("views.left_only")));
    let fields = csv::ReaderBuilder::new()
        .delimiter(b'\t')
        .from_reader(tsv.as_bytes())
        .records()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(fields.len(), 1);
    assert_eq!(&fields[0][0], "2");
    assert_eq!(&fields[0][2], exact);

    let mut larger = sheet(&["key", "detail"], &[]);
    larger.rows = (0..67)
        .map(|_| vec![CellValue::String("left".into()), CellValue::Null])
        .collect();
    let all = collect_hash_rows(&larger, &[0]);
    let none = HashMap::new();
    let preview = separate_groups(&all, &none);
    assert_eq!(preview.left_only[0].1.len(), 67);
    let tsv = bucket_to_tsv(&larger, &preview.left_only, &[]).unwrap();
    let shown = csv::ReaderBuilder::new()
        .delimiter(b'\t')
        .from_reader(tsv.as_bytes())
        .records()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(shown.len(), ROWS_PER_BUCKET);
    assert_eq!(&shown[49][0], "50");
    assert_eq!(&shown[49][2], "");
}
