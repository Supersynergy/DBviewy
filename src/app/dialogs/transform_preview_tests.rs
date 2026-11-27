use super::*;
use crate::app::state::{OctaApp, SplitMode};
use eframe::egui::accesskit::{Action, ActionRequest, TreeId};
use octa::data::{CellValue::String as Text, ColumnInfo};
use octa::ui::settings::{AppSettings, IconVariant};

fn fixture(values: &[&str]) -> DataTable {
    DataTable {
        columns: ["Code", "Piece_1", "Piece_1_2"]
            .into_iter()
            .map(|name| ColumnInfo {
                name: name.into(),
                data_type: "Utf8".into(),
            })
            .collect(),
        rows: values
            .iter()
            .map(|s| vec![Text((*s).into()), Text("keep".into()), CellValue::Null])
            .collect(),
        ..DataTable::empty()
    }
}

fn step(by: &str, value: &str) -> Split {
    Split {
        column: "Code".into(),
        by: by.into(),
        value: value.into(),
        new_name: "Piece".into(),
        position: Some(2),
    }
}

#[test]
fn all_split_modes_preview_the_same_values_and_names_as_recipe_apply() {
    for (by, value, inputs, first) in [
        ("delimiter", "/", ["0007/Nord/", "0010", "11//Süd"], "0007"),
        ("regex", r"[ ;]+", ["0007 Nord;", "0010", "11 ;Süd"], "0007"),
        ("width", "2", ["Å界Ñ", "α", "007"], "Å界"),
    ] {
        let mut table = fixture(&inputs);
        table.set(1, 1, Text("unsaved".into()));
        let step = step(by, value);
        let preview = sample(&table, &step).unwrap().unwrap();
        assert_eq!(table.col_count(), 3);
        assert_eq!(table.undo_stack.len(), 1);
        assert_eq!(preview.columns[0].0, "Piece_1_3");
        assert_eq!(preview.columns[0].1[0], Text(first.into()));
        assert_eq!(preview.columns[1].1[1], CellValue::Null);
        if by != "width" {
            assert_eq!(preview.columns[2].1[0], Text(String::new()));
        }

        step.apply(&mut table).unwrap();
        for (offset, (name, values)) in preview.columns.iter().enumerate() {
            assert_eq!(&table.columns[offset + 1].name, name);
            for (row, value) in values.iter().enumerate() {
                assert_eq!(table.get(row, offset + 1), Some(value));
            }
        }
        assert_eq!(table.get(0, 0), Some(&Text(inputs[0].into())));
        assert_eq!(
            table.get(1, table.col_count() - 2),
            Some(&Text("unsaved".into()))
        );
    }
}

#[test]
fn preview_bounds_rows_columns_and_large_input_without_a_full_scan() {
    let mut table = fixture(&["a/b/c/d/e/f/g/h", "0010", "x/y", "must not be read"]);
    table.set(1, 0, Text("edited/0007".into()));
    table.set(3, 0, Text("x".repeat(SAMPLE_CELL_BYTES + 1)));
    let delim = step("delimiter", "/");
    let preview = sample(&table, &delim).unwrap().unwrap();
    assert_eq!(preview.columns.len(), SAMPLE_COLS);
    assert_eq!(preview.columns[0].1.len(), SAMPLE_ROWS);
    assert_eq!(preview.columns[1].1[1], Text("0007".into()));
    assert_eq!(preview.columns[5].1[0], Text("f".into()));

    assert!(sample(&table, &step("regex", "[")).is_err());
    assert!(sample(&fixture(&[]), &delim).unwrap().is_none());
    assert!(
        sample(&table, &step("regex", &"x".repeat(SAMPLE_RULE_BYTES + 1)))
            .unwrap()
            .is_none()
    );
    table.set(0, 0, Text("x".repeat(SAMPLE_CELL_BYTES + 1)));
    assert!(sample(&table, &delim).unwrap().is_none());
}

fn frame(app: &mut OctaApp, ctx: &egui::Context, events: Vec<egui::Event>) -> egui::FullOutput {
    let mut out = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(460.0, 540.0),
            )),
            events,
            ..Default::default()
        },
        |ui| super::super::render_transform_dialog(app, ui.ctx()),
    );
    out.textures_delta.clear();
    out
}

fn text_rect(out: &egui::FullOutput, text: &str) -> Option<egui::Rect> {
    out.shapes.iter().find_map(|s| match &s.shape {
        egui::Shape::Text(shape) if shape.galley.text() == text => {
            let rect = shape.visual_bounding_rect().intersect(s.clip_rect);
            rect.is_positive().then_some(rect)
        }
        _ => None,
    })
}

#[test]
fn focused_preview_opens_without_inserting_columns_and_esc_returns_to_the_control() {
    let mut app = OctaApp::new(vec![], AppSettings::default(), IconVariant::Rose);
    app.tabs[0].table = fixture(&["0007/Nord", "0010/Süd", "Teil//", "Outside/preview"]);
    app.transform_dialog = Some(TransformState {
        col: Some(0),
        split_mode: SplitMode::Delimiter,
        split_delim: "/".into(),
        new_name: "Piece".into(),
        ..TransformState::default()
    });

    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    frame(&mut app, &ctx, vec![]);
    let initial = frame(&mut app, &ctx, vec![]);
    let name = t("dialog.pv_preview");
    assert!(text_rect(&initial, &name).is_some());
    let (id, _) = initial
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .find(|(_, node)| {
            node.label() == Some(name.as_str()) && node.supports_action(Action::Focus)
        })
        .expect("Preview is named and keyboard focusable");

    let focus = frame(
        &mut app,
        &ctx,
        vec![egui::Event::AccessKitActionRequest(ActionRequest {
            action: Action::Focus,
            target_tree: TreeId::ROOT,
            target_node: *id,
            data: None,
        })],
    );
    assert_eq!(focus.platform_output.accesskit_update.unwrap().focus, *id);
    let key = |key| egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    frame(&mut app, &ctx, vec![key(egui::Key::Enter)]);
    let shown = frame(&mut app, &ctx, vec![]);
    assert!(text_rect(&shown, "0007").is_some());
    assert!(text_rect(&shown, "Piece_1_3").is_some());
    assert!(text_rect(&shown, "Outside").is_none());
    assert_eq!(app.tabs[0].table.col_count(), 3);
    assert!(app.tabs[0].table.edits.is_empty());
    assert!(app.tabs[0].table.undo_stack.is_empty());

    frame(&mut app, &ctx, vec![key(egui::Key::Escape)]);
    let dismissed = frame(&mut app, &ctx, vec![]);
    assert!(text_rect(&dismissed, "0007").is_none());
    assert!(text_rect(&dismissed, &name).is_some());
    assert!(app.transform_dialog.is_some());
    // Enter again from the same focus, with no mouse click or reassignment.
    frame(&mut app, &ctx, vec![key(egui::Key::Enter)]);
    let reopened = frame(&mut app, &ctx, vec![]);
    assert!(text_rect(&reopened, "0007").is_some());
}
