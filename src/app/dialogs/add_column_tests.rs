use super::*;
use octa::data::{CellValue, ColumnInfo};

/// One Ctrl+Z takes the whole formula column back, not one cell of it.
#[test]
fn a_formula_column_is_one_undo_step() {
    let mut app = OctaApp::new(
        vec![],
        octa::ui::settings::AppSettings::default(),
        octa::ui::settings::IconVariant::Rose,
    );
    let tab = &mut app.tabs[0];
    tab.table.columns = ["a", "b"]
        .map(|n| ColumnInfo {
            name: n.into(),
            data_type: "Int64".into(),
        })
        .to_vec();
    tab.table.rows = (1..=3)
        .map(|i| vec![CellValue::Int(i), CellValue::Int(10 * i)])
        .collect();
    tab.new_col_name = "sum".into();
    tab.new_col_type = "Int64".into();
    tab.new_col_formula = "=A1+B1".into();
    tab.insert_col_at = None;

    add_formula_column(&mut app);
    let table = &app.tabs[0].table;
    assert_eq!(table.columns[2].name, "sum");
    assert_eq!(table.get(2, 2), Some(&CellValue::Int(33)));

    app.tabs[0].table.undo();
    assert_eq!(app.tabs[0].table.col_count(), 2);
}

fn workbook_app() -> OctaApp {
    OctaApp::new(
        vec![],
        octa::ui::settings::AppSettings::default(),
        octa::ui::settings::IconVariant::Rose,
    )
}

#[test]
fn fill_moves_relative_but_not_anchored_ranges_or_columns() {
    let template = FormulaRows::new("SUM($A$1:$A1; B$2; $AA2)+ROUND(1.25,2)");
    let mut buffer = String::new();
    assert_eq!(
        template.at(7, &mut buffer),
        Some("SUM($A$1:$A8; B$2; $AA9)+ROUND(1.25,2)")
    );
    assert_eq!(
        template.at(8, &mut buffer),
        Some("SUM($A$1:$A9; B$2; $AA10)+ROUND(1.25,2)")
    );
    assert_eq!(
        FormulaRows::new("A0+$B$1").at(2, &mut buffer),
        Some("A0+$B$1")
    );
    let max = format!("A{}", usize::MAX);
    assert_eq!(FormulaRows::new(&max).at(1, &mut buffer), None);
}

#[test]
fn anchored_formula_column_reads_original_positions_and_edits_then_survives_xlsx_save() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tax.csv");
    std::fs::write(&path, "qty,price,tax\n2,10,0.2\n5,20,0.99\n3,2.5,0.5\n").unwrap();
    let mut app = workbook_app();
    app.load_file(path);
    let active = app.active_tab;
    let tab = &mut app.tabs[active];
    tab.table.set(0, 2, CellValue::Float(0.25));
    tab.new_col_name = "gross".into();
    tab.new_col_type = "Float64".into();
    tab.new_col_formula = "=ROUND($A1*B1*(1+$C$1),2)".into();
    tab.insert_col_at = Some(0);
    let undo_before = tab.table.undo_stack.len();

    add_formula_column(&mut app);
    let table = &mut app.tabs[active].table;
    assert_eq!(table.undo_stack.len(), undo_before + 1);
    assert_eq!(
        table
            .columns
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        ["gross", "qty", "price", "tax"]
    );
    for (row, value) in [
        CellValue::Int(25),
        CellValue::Int(125),
        CellValue::Float(9.38),
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(table.get(row, 0), Some(&value));
    }
    assert!(table.undo());
    assert_eq!(table.col_count(), 3);
    assert_eq!(table.get(0, 2), Some(&CellValue::Float(0.25)));
    assert!(table.redo());

    use octa::formats::{FormatReader, excel_reader::ExcelReader};
    let output = dir.path().join("gross.xlsx");
    ExcelReader.write_file(&output, table).unwrap();
    let reopened = ExcelReader.read_file(&output).unwrap();
    assert_eq!(reopened.columns[0].name, "gross");
    for (row, value) in [25.0, 125.0, 9.38].into_iter().enumerate() {
        let reopened = match reopened.get(row, 0) {
            Some(CellValue::Int(n)) => *n as f64,
            Some(CellValue::Float(n)) => *n,
            other => panic!("expected a numeric XLSX cell at row {row}, got {other:?}"),
        };
        assert_eq!(reopened, value);
    }
    assert!(reopened.formulas.is_empty());
}

#[test]
fn anchored_range_builds_a_running_total() {
    let mut app = workbook_app();
    let tab = &mut app.tabs[0];
    tab.table.columns.push(ColumnInfo {
        name: "revenue".into(),
        data_type: "Int64".into(),
    });
    tab.table.rows = [12, 8, -5].map(|n| vec![CellValue::Int(n)]).to_vec();
    tab.new_col_name = "running".into();
    tab.new_col_formula = "=SUM(A$1:A1)".into();
    add_formula_column(&mut app);
    let table = &app.tabs[0].table;
    for (row, expected) in [12, 20, 15].into_iter().enumerate() {
        assert_eq!(table.get(row, 1), Some(&CellValue::Int(expected)));
    }
}

#[test]
fn relative_row_zero_does_not_turn_into_a_valid_reference_later_in_fill() {
    let mut app = workbook_app();
    let tab = &mut app.tabs[0];
    tab.table.columns.push(ColumnInfo {
        name: "qty".into(),
        data_type: "Int64".into(),
    });
    tab.table.rows = [1, 2, 3].map(|n| vec![CellValue::Int(n)]).to_vec();
    tab.new_col_name = "invalid".into();
    tab.new_col_formula = "=A0+100".into();
    add_formula_column(&mut app);
    let tab = &app.tabs[0];
    assert!((0..3).all(|row| tab.table.get(row, 1) == Some(&CellValue::Null)));
    assert!(tab.parse_error_banner.as_deref().unwrap().contains("3/3"));
}

#[test]
fn fill_diagnostics_name_the_source_column_even_when_output_is_inserted_first() {
    let mut app = workbook_app();
    let tab = &mut app.tabs[0];
    tab.table.columns.push(ColumnInfo {
        name: "source price".into(),
        data_type: "Float64".into(),
    });
    tab.table.rows = vec![
        vec![CellValue::String("not numeric".into())],
        vec![CellValue::Int(12)],
    ];
    tab.new_col_name = "gross".into();
    tab.new_col_formula = "=A1+1".into();
    tab.insert_col_at = Some(0);
    add_formula_column(&mut app);
    let tab = &app.tabs[0];
    assert_eq!(tab.table.get(0, 0), Some(&CellValue::Null));
    assert_eq!(tab.table.get(1, 0), Some(&CellValue::Int(13)));
    let banner = tab.parse_error_banner.as_deref().unwrap();
    assert!(
        banner.contains("1/2") && banner.contains("source price") && banner.contains("not numeric")
    );
}

#[test]
fn calculated_column_does_not_write_if_read_only_changes_while_dialog_is_open() {
    let mut app = workbook_app();
    let tab = &mut app.tabs[0];
    tab.show_add_column_dialog = true;
    tab.new_col_name = "not written".into();
    tab.new_col_formula = "=ROUND($A$1,2)".into();
    let before = tab.table.col_count();
    app.readonly_mode = true;
    add_formula_column(&mut app);
    assert_eq!(app.tabs[0].table.col_count(), before);
    assert!(app.tabs[0].table.undo_stack.is_empty());
}
