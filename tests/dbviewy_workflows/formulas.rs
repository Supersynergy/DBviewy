use super::*;
use octa::data::{evaluate_formula, evaluate_formula_with_diagnostics};
use octa::formats::{FormatReader, excel_reader::ExcelReader, write_options::WriteOptions};
use rust_xlsxwriter::{Formula, Workbook};

fn invoice_book(fx: &Fixture) -> PathBuf {
    let path = fx.path("invoices.xlsx");
    let mut book = Workbook::new();
    let sheet = book.add_worksheet().set_name("Invoices").unwrap();
    sheet.write_string(0, 0, "qty").unwrap();
    sheet.write_string(0, 1, "price").unwrap();
    sheet.write_string(0, 2, "net").unwrap();
    for (i, (qty, price)) in [(2.0, 8.0), (3.0, 9.0)].into_iter().enumerate() {
        let row = i as u32 + 1;
        sheet.write_number(row, 0, qty).unwrap();
        sheet.write_number(row, 1, price).unwrap();
        sheet
            .write_formula(
                row,
                2,
                Formula::new(format!("=A{n}*B{n}", n = row + 1))
                    .set_result((qty * price).to_string()),
            )
            .unwrap();
    }
    book.save(&path).unwrap();
    path
}

#[test]
fn uc25_quantity_edit_changes_a_line_formula_and_undo_restores_it() {
    let fx = Fixture::new();
    let mut grid = read(&fx.file("orders.csv", "qty,unit_price\n3,12\n5,10\n"));
    assert_eq!(evaluate_formula("A1*B1", &grid), Some(36.0));
    grid.set(0, 0, CellValue::Int(4));
    assert_eq!(evaluate_formula("A1*B1", &grid), Some(48.0));
    assert_eq!(evaluate_formula("A2*B2", &grid), Some(50.0));
    assert!(grid.undo());
    assert_eq!(evaluate_formula("A1*B1", &grid), Some(36.0));
}

#[test]
fn uc26_invoice_discount_and_tax_respect_parentheses_after_export() {
    let fx = Fixture::new();
    let input = fx.file(
        "invoice.csv",
        "quantity,price,discount,surcharge\n4,25,10,5\n",
    );
    let mut grid = read(&input);
    let value = evaluate_formula("(A1*B1-C1)+D1", &grid).unwrap();
    assert_eq!(value, 95.0);
    grid.insert_column(4, "due".into(), "Float64".into());
    grid.set(0, 4, CellValue::Float(value));
    grid.apply_edits();
    let path = fx.path("invoice.parquet");
    write(&path, &grid);
    numbers(&read(&path), "due", &[95.0]);
    assert_eq!(read(&input).col_count(), 4);
}

#[test]
fn uc27_average_of_issued_invoices_excludes_the_missing_amount() {
    let fx = Fixture::new();
    let t = read(&fx.file("weekly.csv", "invoice,amount\n201,12\n202,\n203,30\n"));
    assert_eq!(evaluate_formula("AVERAGE(B1:B3)", &t), Some(21.0));
    assert_eq!(evaluate_formula("ANZAHL(B1:B3)", &t), Some(2.0));
    assert_eq!(evaluate_formula("SUMME(B1:B3)", &t), Some(42.0));
}

#[test]
fn uc28_formula_diagnostics_identify_the_invoice_cell_needing_correction() {
    let fx = Fixture::new();
    let p = fx.file("input.tsv", "qty\tprice\n4\t10\nunknown\t12\n");
    let mut t = read(&p);
    let invalid = evaluate_formula_with_diagnostics("A2*B2", &t);
    let bad = invalid
        .bad_cell
        .expect("address and current value are visible to UI");
    assert_eq!(invalid.value, None);
    assert_eq!((bad.row, bad.col, bad.content.as_str()), (1, 0, "unknown"));
    t.set(1, 0, CellValue::Int(3));
    assert_eq!(evaluate_formula("A2*B2", &t), Some(36.0));
    assert_eq!(
        fs::read_to_string(&p).unwrap(),
        "qty\tprice\n4\t10\nunknown\t12\n"
    );
}

#[test]
fn uc29_zero_and_invalid_references_fail_without_numeric_fabrication() {
    let fx = Fixture::new();
    let t = read(&fx.file("hours.csv", "budget,hours\n120,0\n120,8\n"));
    assert_eq!(evaluate_formula("A1/B1", &t), None);
    assert_eq!(evaluate_formula("A1/A0", &t), None);
    assert_eq!(evaluate_formula("A1/Z99", &t), None);
    assert_eq!(evaluate_formula("A2/B2", &t), Some(15.0));
}

#[test]
fn uc30_wide_forecast_can_reference_columns_beyond_z() {
    let fx = Fixture::new();
    let headers = (1..=28)
        .map(|n| format!("week{n}"))
        .collect::<Vec<_>>()
        .join(",");
    let numbers = (1..=28)
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let input = read(&fx.file("forecast.csv", format!("{headers}\n{numbers}\n")));
    assert_eq!(evaluate_formula("SUM(Z1:AB1)", &input), Some(81.0));
    assert_eq!(evaluate_formula("AA1-A1", &input), Some(26.0));
}

#[test]
fn uc31_excel_cached_formula_results_flow_into_json_without_formula_strings() {
    let fx = Fixture::new();
    let src = invoice_book(&fx);
    let workbook = ExcelReader.read_table(&src, "Invoices").unwrap();
    assert_eq!(workbook.formula(0, 2), Some("=A2*B2"));
    assert_eq!(workbook.formula(1, 2), Some("=A3*B3"));
    let path = fx.path("invoice-summary.json");
    write(&path, &workbook);
    let records: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(
        records,
        serde_json::json!([{"qty":2.0,"price":8.0,"net":16.0},{"qty":3.0,"price":9.0,"net":27.0}])
    );
}

#[test]
fn uc32_explicit_workbook_copy_preserves_formula_and_cached_total() {
    let fx = Fixture::new();
    let source = invoice_book(&fx);
    let before = fs::read(&source).unwrap();
    let values = read(&source);
    let mut options = WriteOptions::default();
    options.xlsx.preserve_formulas = true;
    let dest = fx.path("review.xlsx");
    ExcelReader
        .write_file_with_options(&dest, &values, &options)
        .unwrap();
    let reopened = read(&dest);
    assert_eq!(reopened.formula(0, 2), Some("=A2*B2"));
    assert_eq!(reopened.formula(1, 2), Some("=A3*B3"));
    numbers(&reopened, "net", &[16.0, 27.0]);
    assert_eq!(fs::read(source).unwrap(), before);
}

#[test]
fn uc33_sorting_a_workbook_copy_does_not_leave_stale_reference_formulas() {
    let fx = Fixture::new();
    let mut data = read(&invoice_book(&fx));
    data.sort_rows_by_columns(&[(0, false)], octa::data::TextOrder::Letters);
    numbers(&data, "qty", &[3.0, 2.0]);
    assert!(data.formula(0, 2).is_none());
    let mut options = WriteOptions::default();
    options.xlsx.preserve_formulas = true;
    let path = fx.path("ordered-review.xlsx");
    ExcelReader
        .write_file_with_options(&path, &data, &options)
        .unwrap();
    let result = read(&path);
    assert!(result.formulas.is_empty());
    numbers(&result, "net", &[27.0, 16.0]);
}

#[test]
fn uc34_manual_formula_cell_override_survives_a_formula_aware_export() {
    let fx = Fixture::new();
    let source = invoice_book(&fx);
    let mut grid = read(&source);
    assert_eq!(grid.formula(0, 2), Some("=A2*B2"));
    grid.set(0, 2, CellValue::Int(19));
    assert_eq!(grid.formula(0, 2), None);
    assert_eq!(grid.formula(1, 2), Some("=A3*B3"));
    let mut options = WriteOptions::default();
    options.xlsx.preserve_formulas = true;
    let path = fx.path("override.xlsx");
    ExcelReader
        .write_file_with_options(&path, &grid, &options)
        .unwrap();
    let saved = read(&path);
    numbers(&saved, "net", &[19.0, 27.0]);
    assert_eq!(saved.formula(0, 2), None);
    assert_eq!(saved.formula(1, 2), Some("=A3*B3"));
    assert_eq!(read(&source).formula(0, 2), Some("=A2*B2"));
}
