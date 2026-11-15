use crate::app::OctaApp;
use octa::data::{CellValue, ColumnInfo, TextOrder};
use octa::ui::settings::{AppSettings, IconVariant};

#[test]
fn row_move_uses_visible_neighbour_after_filtering_and_keeps_formula() {
    let mut app = OctaApp::new(vec![], AppSettings::default(), IconVariant::Rose);
    let tab = &mut app.tabs[0];
    tab.table.columns.push(ColumnInfo {
        name: "amount".into(),
        data_type: "Int64".into(),
    });
    tab.table.rows = [40, 10, 30, 20].map(|v| vec![CellValue::Int(v)]).into();
    tab.table.formulas.insert((0, 0), "=A1".into());
    tab.table
        .sort_rows_by_columns(&[(0, true)], TextOrder::from_natural(false));
    tab.filtered_rows = vec![0, 3];
    tab.table_state.selected_cell = Some((3, 0));
    tab.table_state.row_heights.insert(3, 54.0);
    tab.move_row_to_neighbour(false);
    assert_eq!(tab.table.rows[0][0], CellValue::Int(40));
    assert_eq!(tab.table_state.selected_cell, Some((0, 0)));
    assert_eq!(tab.table.formulas.get(&(0, 0)).unwrap(), "=A1");
    app.do_undo();
    let tab = &app.tabs[0];
    assert_eq!(tab.table_state.selected_cell, Some((3, 0)));
    assert_eq!(tab.table_state.row_heights.get(&3), Some(&54.0));
    assert_eq!(tab.table.rows[3][0], CellValue::Int(40));
    assert_eq!(tab.table.formulas.get(&(3, 0)).unwrap(), "=A1");
    app.do_redo();
    let tab = &app.tabs[0];
    assert_eq!(tab.table_state.selected_cell, Some((0, 0)));
    assert_eq!(tab.table_state.row_heights.get(&0), Some(&54.0));
}
