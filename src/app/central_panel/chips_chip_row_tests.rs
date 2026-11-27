use super::*;
use octa::data::{ColumnInfo, DataTable};
use std::collections::{HashMap, HashSet};

fn two_col_table() -> DataTable {
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "city".into(),
            data_type: "Utf8".into(),
        },
        ColumnInfo {
            name: "year".into(),
            data_type: "Int64".into(),
        },
    ];
    t
}

/// A chip has to say what it is doing without being read twice. One
/// allowed value reads as the value itself; several read as a count,
/// because a chip listing fifteen values is not a chip any more.
#[test]
fn chip_labels_name_the_column_and_the_value_count() {
    let t = two_col_table();
    let mut filters: HashMap<usize, HashSet<String>> = HashMap::new();
    filters.insert(0, ["Aachen".to_string()].into_iter().collect());
    filters.insert(
        1,
        ["2025".to_string(), "2026".to_string()]
            .into_iter()
            .collect(),
    );

    let labels = column_filter_chip_labels(&t, &filters);
    assert_eq!(labels.len(), 2);
    assert_eq!(labels[0], (0, "city: Aachen".to_string()));
    assert_eq!(labels[1].0, 1);
    assert!(
        labels[1].1.starts_with("year: "),
        "names the column: {}",
        labels[1].1
    );
    assert!(
        labels[1].1.contains('2'),
        "several values report a count: {}",
        labels[1].1
    );
}

/// Chips come back in column order whatever order the map iterates, or
/// the row would reshuffle itself between frames.
#[test]
fn chips_are_ordered_by_column_not_by_hash() {
    let t = two_col_table();
    let mut filters: HashMap<usize, HashSet<String>> = HashMap::new();
    filters.insert(1, ["2025".to_string()].into_iter().collect());
    filters.insert(0, ["Aachen".to_string()].into_iter().collect());
    let labels = column_filter_chip_labels(&t, &filters);
    assert_eq!(
        labels.iter().map(|(c, _)| *c).collect::<Vec<_>>(),
        vec![0, 1]
    );
}

/// A filter on a column that no longer exists (deleted while filtered)
/// must be skipped, not panic and not render a nameless chip.
#[test]
fn a_filter_on_a_missing_column_is_skipped() {
    let t = two_col_table();
    let mut filters: HashMap<usize, HashSet<String>> = HashMap::new();
    filters.insert(9, ["x".to_string()].into_iter().collect());
    assert!(column_filter_chip_labels(&t, &filters).is_empty());
}

fn filter_app() -> OctaApp {
    use octa::data::CellValue::{Int, String as Text};
    let mut app = OctaApp::new(
        vec![],
        Default::default(),
        octa::ui::settings::IconVariant::Rose,
    );
    app.search_result_mode = SearchResultMode::Filter;
    let mut table = two_col_table();
    table.columns.push(ColumnInfo {
        name: "region".into(),
        data_type: "Utf8".into(),
    });
    table.rows = vec![
        vec![Text("Aachen".into()), Int(2025), Text("West".into())],
        vec![Text("Berlin".into()), Int(2026), Text("East".into())],
        vec![Text("Aachen".into()), Int(2026), Text("West".into())],
    ];
    app.tabs[0].table = table;
    app.tabs[0].filter_dirty = true;
    app
}

#[test]
fn zero_value_chip_explains_an_empty_grid_and_can_restore_its_rows() {
    let mut app = filter_app();
    app.tabs[0].column_filters.insert(0, HashSet::new());
    app.recompute_filter();
    assert!(app.tabs[0].filtered_rows.is_empty());
    let chips = filter_chips(&app.tabs[0], app.search_result_mode);
    assert_eq!(chips.len(), 1);
    assert_eq!(chips[0].removal, ChipRemoval::Column(0));
    assert_eq!(
        chips[0].label,
        format!("city: {}", t("facet.chip_values").replace("{count}", "0"))
    );
    app.apply_chip_action(ChipAction::Remove(chips[0].removal));
    app.recompute_filter();
    assert_eq!(app.tabs[0].filtered_rows, [0, 1, 2]);
}

#[test]
fn text_chip_shows_scope_only_when_search_hides_rows() {
    let mut app = filter_app();
    app.tabs[0].search_text = "Aach".into();
    app.tabs[0].search_scope_col = Some(0);
    let chips = filter_chips(&app.tabs[0], SearchResultMode::Filter);
    assert_eq!(chips[0].removal, ChipRemoval::Search);
    assert_eq!(chips[0].label, format!("{} / city: Aach", t("menu.search")));
    assert!(filter_chips(&app.tabs[0], SearchResultMode::Highlight).is_empty());
    app.tabs[0].view_mode = octa::data::ViewMode::Raw;
    assert!(filter_chips(&app.tabs[0], SearchResultMode::Filter).is_empty());
}

#[test]
fn clear_all_removes_only_filter_state_and_restores_manual_visibility() {
    use crate::app::state::DuplicateFilter;
    use octa::data::conditional_format::CondOp;
    use octa::data::predicate_filter::PredicateFilter;
    use octa::data::{MarkColor, MarkKey};
    let mut app = filter_app();
    let rows = app.tabs[0].table.rows.clone();
    {
        let tab = &mut app.tabs[0];
        tab.search_text = "Aach".into();
        tab.search_scope_col = Some(0);
        tab.search_nav.match_count = 5;
        tab.table
            .marks
            .insert(MarkKey::Column(0), MarkColor::Yellow);
        tab.hidden_columns.insert(1);
        tab.table.hide_rows([2]);
        tab.duplicate_filter = Some(DuplicateFilter {
            key_cols: vec![0],
            keep_duplicates: true,
        });
        tab.column_filters.insert(2, ["West".to_owned()].into());
        tab.predicate_filters.push(PredicateFilter {
            col: 1,
            op: CondOp::Gt,
            value: "0".into(),
            case_sensitive: false,
        });
    }
    app.toggle_filter_to_marked();
    assert!(app.tabs[0].hidden_columns.contains(&2));
    let order: Vec<_> = filter_chips(&app.tabs[0], app.search_result_mode)
        .iter()
        .map(|chip| chip.removal)
        .collect();
    assert_eq!(
        order,
        [
            ChipRemoval::Search,
            ChipRemoval::Marked,
            ChipRemoval::Duplicate,
            ChipRemoval::Column(2),
            ChipRemoval::Predicate(0)
        ]
    );
    let undo = app.tabs[0].table.undo_stack.len();
    app.apply_chip_action(ChipAction::ClearAll);
    app.recompute_filter();
    let tab = &app.tabs[0];
    assert!(tab.search_text.is_empty());
    assert_eq!(tab.search_nav.match_count, 0);
    assert_eq!(tab.search_scope_col, Some(0), "scope remains a preference");
    assert!(filter_chips(tab, app.search_result_mode).is_empty());
    assert_eq!(tab.hidden_columns, [1].into());
    assert_eq!(tab.filtered_rows, [0, 1]);
    assert_eq!(tab.table.rows, rows);
    assert_eq!(tab.table.undo_stack.len(), undo);
    assert!(tab.table.marks.contains_key(&MarkKey::Column(0)));
}

#[test]
fn clearing_chips_updates_the_wanted_database_view_but_keeps_sort_and_highlight() {
    use crate::app::db_view::wanted_view;
    use octa::db::pushdown::view::SortKey;
    let mut app = filter_app();
    app.tabs[0].server_sort.push(SortKey {
        column: "year".into(),
        ascending: false,
        text: false,
    });
    app.tabs[0].search_text = "Berlin".into();
    app.tabs[0].search_scope_col = Some(0);
    app.tabs[0]
        .column_filters
        .insert(2, ["East".to_owned()].into());
    let (before, _) = wanted_view(
        &app.tabs[0],
        search_hides_rows(&app.tabs[0], app.search_result_mode),
    );
    assert_eq!(before.filters.len(), 2);
    app.apply_chip_action(ChipAction::Remove(ChipRemoval::Search));
    let (new, _) = wanted_view(
        &app.tabs[0],
        search_hides_rows(&app.tabs[0], app.search_result_mode),
    );
    assert_eq!(new.filters.len(), 1);
    assert_eq!(before.order, new.order);
    app.search_result_mode = SearchResultMode::Highlight;
    app.tabs[0].search_text = "Berlin".into();
    app.apply_chip_action(ChipAction::ClearAll);
    let (cleared, _) = wanted_view(
        &app.tabs[0],
        search_hides_rows(&app.tabs[0], app.search_result_mode),
    );
    assert!(cleared.filters.is_empty());
    assert_eq!(new.order, cleared.order);
    assert_eq!(app.tabs[0].search_text, "Berlin");
}

fn chip_frame(
    app: &mut OctaApp,
    ctx: &egui::Context,
    width: f32,
    toolbar: bool,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let mut output = ctx.run_ui(
        egui::RawInput {
            events,
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, 430.0),
            )),
            ..Default::default()
        },
        |ui| {
            if toolbar {
                app.render_toolbar(ui);
            }
            app.render_filter_chips(ui);
            if app.tabs[app.active_tab].filter_dirty {
                app.recompute_filter();
            }
        },
    );
    output.textures_delta.clear();
    output
}

#[test]
fn keyboard_remove_moves_focus_to_the_real_search_field() {
    use egui::accesskit::{Action, ActionRequest, TreeId};
    let mut app = filter_app();
    app.tabs[0].search_text = "Berlin".into();
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let out = chip_frame(&mut app, &ctx, 960.0, true, vec![]);
    let tree = out.platform_output.accesskit_update.unwrap();
    let (chip, _) = tree
        .nodes
        .iter()
        .find(|(_, node)| {
            node.label().is_some_and(|label| {
                label.starts_with(&t("search.ask_filter_remove")) && label.contains("Berlin")
            })
        })
        .expect("named remove-search filter button");
    chip_frame(
        &mut app,
        &ctx,
        960.0,
        true,
        vec![egui::Event::AccessKitActionRequest(ActionRequest {
            action: Action::Focus,
            target_tree: TreeId::ROOT,
            target_node: *chip,
            data: None,
        })],
    );
    chip_frame(
        &mut app,
        &ctx,
        960.0,
        true,
        vec![egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert!(app.tabs[0].search_text.is_empty());
    assert_eq!(app.tabs[0].filtered_rows, [0, 1, 2]);
    chip_frame(&mut app, &ctx, 960.0, true, vec![]);
    chip_frame(
        &mut app,
        &ctx,
        960.0,
        true,
        vec![egui::Event::Text("Aach".into())],
    );
    assert_eq!(app.tabs[0].search_text, "Aach");
    assert_eq!(app.tabs[0].filtered_rows, [0, 2]);
}

#[test]
fn long_search_chip_and_clear_all_fit_a_narrow_pane() {
    let mut app = filter_app();
    app.tabs[0].search_text = "Bülowstraße 大きな文字 ".repeat(20);
    app.tabs[0].column_filters.insert(0, HashSet::new());
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let output = chip_frame(&mut app, &ctx, 320.0, false, vec![]);
    let tree = output.platform_output.accesskit_update.unwrap();
    let label = t("search.ask_filters_clear");
    let (_, clear) = tree
        .nodes
        .iter()
        .find(|(_, n)| n.label() == Some(&label))
        .unwrap();
    let (_, search) = tree
        .nodes
        .iter()
        .find(|(_, n)| {
            n.label().is_some_and(|s| {
                s.starts_with(&t("search.ask_filter_remove")) && s.contains("Bülowstraße")
            })
        })
        .unwrap();
    for node in [clear, search] {
        let bounds = node.bounds().unwrap();
        assert!(bounds.x0 >= 0.0 && bounds.x1 <= 320.0, "{bounds:?}");
    }
    assert!(search.label().unwrap().ends_with("..."));
}
