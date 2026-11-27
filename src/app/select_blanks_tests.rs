use super::*;
use octa::data::{CellValue as C, ColumnInfo, DataTable, SearchMode};

fn fixture() -> TabState {
    let mut tab = TabState::new(SearchMode::Plain);
    tab.table = DataTable {
        columns: ["one", "two", "hidden", "four"]
            .map(|name| ColumnInfo {
                name: name.into(),
                data_type: "Utf8".into(),
            })
            .into(),
        rows: vec![
            vec![C::Null, C::String(" ".into()), C::Null, C::Int(0)],
            vec![
                C::String(String::new()),
                C::Bool(false),
                C::Null,
                C::String("tag".into()),
            ],
            vec![C::Null, C::Null, C::Null, C::Null],
            vec![
                C::String("set".into()),
                C::Null,
                C::Null,
                C::String(String::new()),
            ],
            vec![C::Int(5), C::Bool(true), C::Null],
        ],
        ..DataTable::empty()
    };
    tab.filtered_rows = vec![0, 1, 3, 4];
    tab.hidden_columns.insert(2);
    tab
}

#[test]
fn cursor_scope_excludes_filtered_and_hidden_cells_without_coercing_values() {
    let mut tab = fixture();
    tab.table_state.selected_cell = Some((1, 3));
    let p = gather(&tab, LIMITS).unwrap();
    assert_eq!(p.first, (0, 0));
    assert_eq!(p.cells, [(0, 0), (1, 0), (3, 1), (3, 3), (4, 3)].into());
}

#[test]
fn free_selection_preserves_gaps_and_excludes_off_scope_addresses() {
    let mut tab = fixture();
    tab.table_state.selected_cells =
        [(0, 0), (0, 2), (2, 0), (3, 0), (3, 1), (100, 0), (0, 99)].into();
    let p = gather(&tab, LIMITS).unwrap();
    assert_eq!(p.first, (0, 0));
    assert_eq!(p.cells, [(0, 0), (3, 1)].into());
}

#[test]
fn row_and_column_headers_stay_scoped_even_if_every_chosen_row_is_filtered() {
    let mut tab = fixture();
    tab.table_state.selected_rows = [3, 2].into();
    assert_eq!(gather(&tab, LIMITS).unwrap().cells, [(3, 1), (3, 3)].into());
    tab.table_state.selected_rows = [2].into();
    assert_eq!(gather(&tab, LIMITS), Err(Refusal::Empty));
    tab.table_state.selected_rows.clear();
    tab.table_state.selected_cols = [1, 2].into();
    assert_eq!(gather(&tab, LIMITS).unwrap().cells, [(3, 1)].into());
    tab.table_state.selected_cols = [2].into();
    assert_eq!(gather(&tab, LIMITS), Err(Refusal::Empty));
}

#[test]
fn pending_cell_edits_define_emptiness_without_being_applied_or_removed() {
    let mut tab = fixture();
    tab.table.set(3, 1, C::String("pending".into()));
    tab.table.set(1, 1, C::Null);
    let before = tab.table.edits.clone();
    let p = gather(&tab, LIMITS).unwrap();
    assert!(!p.cells.contains(&(3, 1)));
    assert!(p.cells.contains(&(1, 1)));
    assert_eq!(tab.table.edits, before);
}

#[test]
fn scan_match_and_sparse_output_limits_never_return_a_partial_placement() {
    let mut tab = fixture();
    assert_eq!(
        gather(
            &tab,
            Limits {
                scan: 11,
                matches: 100
            }
        ),
        Err(Refusal::TooLarge)
    );
    assert_eq!(
        gather(
            &tab,
            Limits {
                scan: 20,
                matches: 4
            }
        ),
        Err(Refusal::TooLarge)
    );
    assert_eq!(
        gather(
            &tab,
            Limits {
                scan: 12,
                matches: 5
            }
        )
        .unwrap()
        .cells
        .len(),
        5
    );
    tab.table_state.selected_cells = [(0, 0), (4, 3)].into();
    assert_eq!(
        gather(
            &tab,
            Limits {
                scan: 3,
                matches: 8
            }
        ),
        Err(Refusal::TooLarge)
    );
    assert_eq!(
        gather(
            &tab,
            Limits {
                scan: 12,
                matches: 8
            }
        )
        .unwrap()
        .cells,
        [(0, 0), (4, 3)].into()
    );
}

#[test]
fn choosing_blanks_only_changes_selection_on_a_readonly_loaded_page() {
    let mut app = OctaApp::new(
        vec![],
        Default::default(),
        octa::ui::settings::IconVariant::Rose,
    );
    app.readonly_mode = true;
    let mut tab = fixture();
    tab.table.row_offset = 500;
    tab.table.total_rows = Some(10_000);
    tab.filter_dirty = false;
    tab.table_state.selection_anchor_display = Some(3);
    tab.table_state.selected_cols = [1, 3].into();
    let original = tab.table.rows.clone();
    app.tabs[0] = tab;
    app.select_visible_blanks();
    let tab = &app.tabs[0];
    assert_eq!(tab.table_state.selected_cell, Some((3, 1)));
    assert!(tab.table_state.selected_cols.is_empty());
    assert!(tab.table_state.selection_anchor_display.is_none());
    assert_eq!(
        tab.table_state.selected_cells,
        [(3, 1), (3, 3), (4, 3)].into()
    );
    assert_eq!(tab.table.row_offset, 500);
    assert_eq!(tab.table.total_rows, Some(10_000));
    assert_eq!(tab.table.rows, original);
    assert!(tab.table.edits.is_empty() && tab.table.undo_stack.is_empty());
    assert_eq!(
        app.status_message.as_ref().unwrap().0,
        t("search_menu.select_blanks_done").replace("{count}", "3")
    );
}

#[test]
fn no_match_or_active_editor_preserves_the_previous_selection() {
    let mut app = OctaApp::new(
        vec![],
        Default::default(),
        octa::ui::settings::IconVariant::Rose,
    );
    app.tabs[0] = fixture();
    let tab = &mut app.tabs[0];
    tab.filter_dirty = false;
    tab.table_state.selected_cells = [(0, 1), (0, 3)].into();
    tab.table_state.selected_cell = Some((0, 1));
    app.select_visible_blanks();
    assert_eq!(
        app.tabs[0].table_state.selected_cells,
        [(0, 1), (0, 3)].into()
    );
    assert_eq!(app.tabs[0].table_state.selected_cell, Some((0, 1)));
    assert_eq!(
        app.status_message.as_ref().unwrap().0,
        t("search_menu.select_blanks_none")
    );
    app.tabs[0]
        .table_state
        .begin_edit(0, 1, "pending draft".into());
    app.select_visible_blanks();
    assert_eq!(
        app.tabs[0].table_state.editing_cell.as_ref().unwrap().2,
        "pending draft"
    );
    assert!(app.tabs[0].table.edits.is_empty());
}
