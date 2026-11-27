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

fn numeric_grid(cells: impl IntoIterator<Item = CellValue>) -> data::DataTable {
    data::DataTable {
        columns: vec![ColumnInfo {
            name: "amount".into(),
            data_type: "Float64".into(),
        }],
        rows: cells.into_iter().map(|cell| vec![cell]).collect(),
        ..data::DataTable::empty()
    }
}

#[test]
fn prefix_sum_matches_original_order_and_coercion() {
    let mut table = numeric_grid([
        CellValue::Float(1e16),
        CellValue::Float(1.0),
        CellValue::Float(-1e16),
        CellValue::String(" 12.5 ".into()),
        CellValue::Null,
        CellValue::Bool(true),
        CellValue::String("not a number".into()),
        CellValue::Float(-2.5),
        CellValue::Float(f64::INFINITY),
        CellValue::Float(f64::NEG_INFINITY),
        CellValue::Int(20),
    ]);
    table.set(6, 0, CellValue::String(" -4.25 ".into()));
    for formula in [
        "SUM(A$1:A1)",
        "summe ( $a4 : $A$2 )",
        "SUM(A$2:A100)",
        "SUM(A$20:A20)",
        "SUM(AAA$1:AAA1)",
        &format!("SUM(A$1:A{})", usize::MAX),
    ] {
        let actual = fill_running_aggregate(formula, &table).unwrap();
        let (baseline, bad) = fill_formula_template(formula, &table);
        assert!(bad.is_none());
        assert_eq!(actual.len(), baseline.len(), "{formula}");
        for ((row, value), (baseline_row, expected)) in actual.into_iter().zip(baseline) {
            assert_eq!(row, baseline_row);
            assert!(
                value.to_bits() == expected.to_bits() || value.is_nan() && expected.is_nan(),
                "{formula} at row {row}: {value:?}, expected {expected:?}"
            );
        }
    }
}

#[test]
fn prefix_sum_falls_back_for_unrelated_or_malformed_formulas() {
    let table = numeric_grid([CellValue::Int(1), CellValue::Int(2), CellValue::Int(3)]);
    for formula in [
        "SUM(A1:A2)",
        "SUM(A$1:A$3)",
        "SUM(A$2:A1)",
        "SUM(A$1:B1)",
        "SUM(A$1:A1,5)",
        "SUM(A$1:A1)+1",
        "MEDIAN(A$1:A1)",
        "COUNTBLANK(A$1:A1, A1)",
        "AVERAGE(A$2:A1)",
        "COUNTA(A$1:B1)",
        "ANZAHL2(A$1:A1)+1",
        "SUM(A$0:A1)",
        "SUM(A$$1:A1)",
        "SUM(A$1:A 1)",
        "SUM(\u{00a0}A$1:A1)",
        "SUM(XXXXXXXXXXXXXXXXXXXXXX$1:XXXXXXXXXXXXXXXXXXXXXX1)",
    ] {
        assert!(
            fill_running_aggregate(formula, &table).is_none(),
            "{formula}"
        );
    }

    let mut app = workbook_app();
    let tab = &mut app.tabs[0];
    tab.table = table;
    tab.new_col_name = "sum plus one".into();
    tab.new_col_formula = "=SUM(A$1:A1)+1".into();
    add_formula_column(&mut app);
    for (row, expected) in [2, 4, 7].into_iter().enumerate() {
        assert_eq!(
            app.tabs[0].table.get(row, 1),
            Some(&CellValue::Int(expected))
        );
    }
}

#[test]
fn prefix_sum_fills_fifty_thousand_rows_as_one_undo_step() {
    const ROWS: usize = 50_000;
    let mut app = workbook_app();
    let tab = &mut app.tabs[0];
    tab.table = numeric_grid((1..=ROWS as i64).map(CellValue::Int));
    tab.new_col_name = "running sum".into();
    tab.new_col_formula = "=SUM(A$1:A1)".into();
    tab.insert_col_at = Some(0);
    let start = std::time::Instant::now();
    add_formula_column(&mut app);
    println!(
        "actual_fill_rows={ROWS} elapsed_us={}",
        start.elapsed().as_micros()
    );
    let table = &mut app.tabs[0].table;
    for row in [0, 1, 99, 999, ROWS - 1] {
        let n = row as i64 + 1;
        assert_eq!(table.get(row, 0), Some(&CellValue::Int(n * (n + 1) / 2)));
        assert_eq!(table.get(row, 1), Some(&CellValue::Int(n)));
    }
    assert_eq!(table.undo_stack.len(), 1);
    assert!(table.undo());
    assert_eq!(table.col_count(), 1);
    assert_eq!(table.get(ROWS - 1, 0), Some(&CellValue::Int(ROWS as i64)));
}

#[test]
fn prefix_sum_performance_fixture_matches_every_baseline_row() {
    const ROWS: usize = 4_000;
    let table = numeric_grid((0..ROWS as i64).map(|n| CellValue::Int(n % 23 - 11)));
    let formula = "SUM(A$1:A1)";
    // Alternating runs on the exact same input. Timings are observations,
    // not machine-dependent assertions; correctness is checked on every row.
    let mut old_times = [0; 3];
    let mut new_times = [0; 3];
    for i in 0..3 {
        let baseline = || {
            let start = std::time::Instant::now();
            let result = fill_formula_template(formula, &table).0;
            (result, start.elapsed().as_nanos())
        };
        let optimized = || {
            let start = std::time::Instant::now();
            let result = fill_running_aggregate(formula, &table).unwrap();
            (result, start.elapsed().as_nanos())
        };
        let ((old, old_ns), (new, new_ns)) = if i % 2 == 0 {
            (baseline(), optimized())
        } else {
            let candidate = optimized();
            (baseline(), candidate)
        };
        assert_eq!(new, old);
        assert_eq!(new.len(), ROWS);
        old_times[i] = old_ns;
        new_times[i] = new_ns;
    }
    old_times.sort_unstable();
    new_times.sort_unstable();
    println!(
        "running_sum_bench rows={ROWS} baseline_ns={} incremental_ns={} samples=3",
        old_times[1], new_times[1]
    );
}

#[test]
fn running_counts_preserve_empty_text_edits_and_incomplete_rows() {
    let mut table = numeric_grid([
        CellValue::Null,
        CellValue::String("".into()),
        CellValue::Int(0),
        CellValue::Bool(false),
        CellValue::String(" ".into()),
        CellValue::Date("2024-04-01".into()),
        CellValue::String(" 12.5 ".into()),
        CellValue::Float(f64::NAN),
    ]);
    table.set(4, 0, CellValue::Null);
    table.rows.push(Vec::new());
    for (formula, last) in [
        ("COUNT(A$1:A1)", 3.0),
        ("ANZAHL(A$1:A1)", 3.0),
        ("COUNTA(A$1:A1)", 6.0),
        ("ANZAHL2(A$1:A1)", 6.0),
        ("COUNTBLANK(A$1:A1)", 4.0),
        ("ANZAHLLEEREZELLEN ($a1 : A$1)", 4.0),
        ("COUNTBLANK(A$1:A9000000)", 4.0),
        ("COUNTA(A$9000000:A9000000)", 0.0),
        ("COUNTBLANK(AAA$1:AAA1)", 0.0),
    ] {
        let candidate = fill_running_aggregate(formula, &table).unwrap();
        let (expected, bad) = fill_formula_template(formula, &table);
        assert!(bad.is_none());
        assert_eq!(candidate, expected, "{formula}");
        assert_eq!(candidate.last(), Some(&(8, last)), "{formula}");
    }
}

#[test]
fn indexed_rank_fill_matches_the_scalar_evaluator_and_keeps_diagnostics() {
    let mut table = numeric_grid([
        CellValue::Int(90),
        CellValue::String(" 75 ".into()),
        CellValue::Float(75.0),
        CellValue::Null,
        CellValue::String("pending".into()),
        CellValue::Float(-0.0),
        CellValue::Float(f64::INFINITY),
    ]);
    table.set(0, 0, CellValue::Int(80));
    for formula in [
        "RANK.EQ(A1,$A$1:$A$2000000)",
        " RANG.MITTELW ( $a1 ; A$7:$A$1 ; +1 ) ",
        "RANG(a1;A$2:A$2;0)",
        "RANK.AVG(A4,A$1:A$6)",
        "RANK.EQ(A1,AA$1:AA$20)",
    ] {
        let (normal, normal_bad) = fill_shifted_template(
            &FormulaRows::new(formula),
            &table,
            &[],
            data::evaluate_formula_with_diagnostics,
        );
        let (indexed, indexed_bad) = formula_rank_fill::fill(formula, &table).unwrap();
        assert_eq!(indexed, normal, "{formula}");
        let cell = |bad: Option<data::FormulaBadCell>| bad.map(|b| (b.row, b.col, b.content));
        assert_eq!(cell(indexed_bad), cell(normal_bad), "{formula}");
    }
    for unsupported in [
        "RANK.EQ(A1,A2:A$20)",
        "RANK.AVG(A1,A$1:B$20)",
        "RANK.EQ(A1,A$1:A$20,IF(1,1,0))",
        "RANK.EQ(A1+B1,A$1:A$20)",
        "RANK.EQ($A$1,A$1:A$20)",
        "RANK.EQ(A1,A$1:A$20,)",
    ] {
        assert!(
            formula_rank_fill::fill(unsupported, &table).is_none(),
            "{unsupported}"
        );
    }
}

#[test]
fn indexed_rank_fill_handles_a_full_column_of_distinct_values() {
    let count = 20_000;
    let table = numeric_grid((0..count).rev().map(|n| CellValue::Int(n as i64)));
    let (values, bad) = formula_rank_fill::fill("RANK.EQ(A1,$A$1:$A$999999)", &table).unwrap();
    assert!(bad.is_none());
    assert_eq!(values.len(), count);
    assert!(
        values
            .iter()
            .enumerate()
            .all(|(i, row)| *row == (i, (i + 1) as f64))
    );
}

#[test]
fn indexed_rank_column_preview_and_xlsx_export_agree_and_undo_once() {
    use octa::formats::{FormatReader, excel_reader::ExcelReader};
    let dir = tempfile::tempdir().unwrap();
    let mut app = workbook_app();
    let tab = &mut app.tabs[0];
    tab.table = numeric_grid([90, 75, 75, 60].map(CellValue::Int));
    tab.new_col_name = "Average rank".into();
    tab.new_col_formula = "=RANG.MITTELW(A1;$A$1:$A$4)".into();
    tab.insert_col_at = Some(0);
    let preview = FormulaPreview::read(&tab.new_col_formula, &tab.table).unwrap();
    assert_eq!(preview.values, [Some(1.0), Some(2.5), Some(2.5)]);
    let undo_count = tab.table.undo_stack.len();
    add_formula_column(&mut app);
    let tab = &mut app.tabs[0];
    assert_eq!(tab.table.undo_stack.len(), undo_count + 1);
    let path = dir.path().join("average-rank.xlsx");
    ExcelReader.write_file(&path, &tab.table).unwrap();
    let exported = ExcelReader.read_file(&path).unwrap();
    for (r, expected) in [1.0, 2.5, 2.5, 4.0].into_iter().enumerate() {
        assert_eq!(exported.get(r, 0), Some(&CellValue::Float(expected)));
    }
    assert!(tab.table.undo());
    assert_eq!(tab.table.col_count(), 1);
    assert_eq!(tab.table.get(0, 0), Some(&CellValue::Int(90)));
}

#[test]
fn running_means_start_at_the_first_number_and_preserve_sum_order() {
    let mut table = numeric_grid([
        CellValue::Null,
        CellValue::Bool(true),
        CellValue::String("not yet recorded".into()),
        CellValue::Float(1e16),
        CellValue::Float(1.0),
        CellValue::Float(-1e16),
        CellValue::String(" 4.5 ".into()),
        CellValue::Float(f64::INFINITY),
        CellValue::Float(f64::NEG_INFINITY),
        CellValue::Float(-0.0),
    ]);
    table.set(6, 0, CellValue::String(" 5.25 ".into()));
    for (formula, first) in [
        ("AVERAGE(A$1:A1)", Some(3)),
        ("MITTELWERT ( A3 : $A$2 )", Some(1)),
        ("AVERAGE(A$1:A99999999)", Some(0)),
        ("AVERAGE(A$20:A20)", None),
        ("AVERAGE(XXX$1:XXX1)", None),
    ] {
        let actual = fill_running_aggregate(formula, &table).unwrap();
        let (normal, bad) = fill_formula_template(formula, &table);
        assert!(bad.is_none());
        assert_eq!(actual.first().map(|(row, _)| *row), first);
        assert_eq!(actual.len(), normal.len(), "{formula}");
        for ((r, value), (expected_row, expected)) in actual.into_iter().zip(normal) {
            assert_eq!(r, expected_row);
            assert!(
                value.to_bits() == expected.to_bits() || value.is_nan() && expected.is_nan(),
                "{formula} row {r}: received {value}, expected {expected}"
            );
        }
    }
}

#[test]
fn running_blank_column_exports_as_numbers_and_is_one_undo_step() {
    use octa::formats::{FormatReader, csv_reader::CsvReader, excel_reader::ExcelReader};
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("complete-invoices.csv");
    std::fs::write(
        &input,
        "invoice,approval\nINV-1,\nINV-2,false\nINV-3,paid\nINV-4,\n",
    )
    .unwrap();
    let table = CsvReader.read_file(&input).unwrap();
    let mut app = workbook_app();
    let tab = &mut app.tabs[0];
    tab.table = table;
    tab.new_col_name = "Missing approvals so far".into();
    tab.new_col_formula = "=COUNTBLANK(B$1:B1)".into();
    tab.insert_col_at = Some(0);
    add_formula_column(&mut app);
    let table = &mut app.tabs[0].table;
    let values = [1, 1, 1, 2];
    for (row, expected) in values.into_iter().enumerate() {
        assert_eq!(table.get(row, 0), Some(&CellValue::Int(expected)));
    }
    let export = dir.path().join("approval-check.xlsx");
    ExcelReader.write_file(&export, table).unwrap();
    let report = ExcelReader.read_file(&export).unwrap();
    assert_eq!(report.columns[0].name, "Missing approvals so far");
    for (row, expected) in values.into_iter().enumerate() {
        assert_eq!(
            data::evaluate_formula(&format!("A{}", row + 1), &report),
            Some(expected as f64)
        );
    }
    assert_eq!(table.undo_stack.len(), 1);
    assert!(table.undo());
    assert_eq!(table.col_count(), 2);
    assert_eq!(table.get(1, 1), Some(&CellValue::Bool(false)));
}

#[test]
fn running_blank_benchmark_agrees_with_full_receipt_counts() {
    const ROWS: usize = 4_000;
    let mut table = numeric_grid((0..ROWS).map(|i| {
        if i % 9 == 0 {
            CellValue::Null
        } else if i % 11 == 0 {
            CellValue::String("".into())
        } else {
            CellValue::String(format!("INV-{i}"))
        }
    }));
    table.set(0, 0, CellValue::String("INV-0".into()));
    let mut count = 0usize;
    let expected: Vec<_> = (0..ROWS)
        .map(|i| {
            count += usize::from(i != 0 && (i % 9 == 0 || i % 11 == 0));
            (i, count as f64)
        })
        .collect();
    let formula = "COUNTBLANK(A$1:A1)";
    let uncached = || {
        let now = std::time::Instant::now();
        let (value, bad) = fill_formula_template(formula, &table);
        let time = now.elapsed().as_nanos();
        assert!(bad.is_none());
        (value, time)
    };
    let incremental = || {
        let now = std::time::Instant::now();
        let value = fill_running_aggregate(formula, &table).unwrap();
        (value, now.elapsed().as_nanos())
    };
    let (mut baseline, mut candidate) = ([0; 3], [0; 3]);
    for i in 0..3 {
        let ((from_scratch, base_time), (running, running_time)) = if i % 2 == 0 {
            (uncached(), incremental())
        } else {
            let running = incremental();
            (uncached(), running)
        };
        assert_eq!(from_scratch, expected);
        assert_eq!(running, expected);
        baseline[i] = base_time;
        candidate[i] = running_time;
    }
    baseline.sort_unstable();
    candidate.sort_unstable();
    println!(
        "running_blank_bench rows={ROWS} baseline_ns={} incremental_ns={} samples=3",
        baseline[1], candidate[1]
    );
}

#[test]
fn conditional_fill_shifts_cell_references_but_not_quoted_identifiers() {
    let mut app = workbook_app();
    let tab = &mut app.tabs[0];
    tab.table.columns = ["code", "amount"]
        .map(|name| ColumnInfo {
            name: name.into(),
            data_type: "Utf8".into(),
        })
        .to_vec();
    tab.table.rows = [("A1\"B9", 10), ("A2\"B10", 20), ("A1\"B9", 5)]
        .into_iter()
        .map(|(code, amount)| vec![CellValue::String(code.into()), CellValue::Int(amount)])
        .collect();
    tab.new_col_name = "amount plus matching total".into();
    tab.new_col_formula = r#"=SUMIF($A$1:$A$3,"A1""B9",$B$1:$B$3)+B1"#.into();
    add_formula_column(&mut app);
    let table = &app.tabs[0].table;
    for (row, expected) in [25, 35, 20].into_iter().enumerate() {
        assert_eq!(table.get(row, 2), Some(&CellValue::Int(expected)));
    }
}

#[test]
fn iferror_column_keeps_fixed_discount_and_ignores_the_unselected_branch() {
    let mut app = workbook_app();
    let tab = &mut app.tabs[0];
    tab.table.columns = ["region", "amount", "qty", "discount"]
        .map(|name| ColumnInfo {
            name: name.into(),
            data_type: "Utf8".into(),
        })
        .to_vec();
    tab.table.rows = vec![
        vec![
            CellValue::String("West".into()),
            CellValue::Int(30),
            CellValue::Int(3),
            CellValue::Float(0.5),
        ],
        vec![
            CellValue::String("East".into()),
            CellValue::String("not numeric".into()),
            CellValue::Int(0),
            CellValue::Null,
        ],
        vec![
            CellValue::String("West".into()),
            CellValue::Int(18),
            CellValue::Int(0),
            CellValue::Null,
        ],
    ];
    tab.new_col_name = "discount per item".into();
    tab.new_col_formula = r#"=IFERROR(IF(A1="West",B1/C1*$D$1,0),-1)"#.into();
    tab.insert_col_at = Some(0);
    add_formula_column(&mut app);
    let tab = &mut app.tabs[0];
    for (row, expected) in [5, 0, -1].into_iter().enumerate() {
        assert_eq!(tab.table.get(row, 0), Some(&CellValue::Int(expected)));
    }
    assert!(tab.parse_error_banner.is_none());
    assert!(tab.table.undo());
    assert_eq!(tab.table.col_count(), 4);
}

#[test]
fn multi_criteria_column_respects_fixed_and_shifted_references() {
    let mut app = workbook_app();
    let tab = &mut app.tabs[0];
    tab.table.columns = ["region", "status", "amount", "minimum"]
        .map(|name| ColumnInfo {
            name: name.into(),
            data_type: "Utf8".into(),
        })
        .to_vec();
    tab.table.rows = [
        ("West", "Paid", 40, CellValue::Int(10)),
        ("West", "Paid", 5, CellValue::Int(100)),
        ("East", "Paid", 20, CellValue::Null),
        ("East", "Pending", 100, CellValue::Null),
    ]
    .into_iter()
    .map(|(region, status, amount, minimum)| {
        vec![
            CellValue::String(region.into()),
            CellValue::String(status.into()),
            CellValue::Int(amount),
            minimum,
        ]
    })
    .collect();
    tab.insert_col_at = Some(0);
    tab.new_col_name = "paid for this region".into();
    tab.new_col_formula =
        r#"=SUMIFS($C$1:$C$4,$A$1:$A$4,A1,$B$1:$B$4,"Paid",$C$1:$C$4,">="&$D$1)"#.into();
    add_formula_column(&mut app);
    let table = &mut app.tabs[0].table;
    for (r, expected) in [40, 40, 20, 20].into_iter().enumerate() {
        assert_eq!(table.get(r, 0), Some(&CellValue::Int(expected)));
    }
    assert!(table.undo());
    assert_eq!(table.col_count(), 4);
    assert_eq!(table.get(0, 0), Some(&CellValue::String("West".into())));
}

#[test]
fn lookup_column_keeps_catalog_fixed_and_shifts_the_requested_sku() {
    let mut app = workbook_app();
    let tab = &mut app.tabs[0];
    tab.table.columns = ["sku", "price", "request"]
        .map(|name| ColumnInfo {
            name: name.into(),
            data_type: "Utf8".into(),
        })
        .to_vec();
    tab.table.rows = [
        ("bolt", 10, "nut"),
        ("bolt", 15, "BOLT"),
        ("nut", 20, "missing"),
    ]
    .into_iter()
    .map(|(sku, price, request)| {
        vec![
            CellValue::String(sku.into()),
            CellValue::Int(price),
            CellValue::String(request.into()),
        ]
    })
    .collect();
    tab.insert_col_at = Some(0);
    tab.new_col_name = "price from catalog".into();
    tab.new_col_formula = "=XLOOKUP(C1,$A$1:$A$3,$B$1:$B$3,-1,0,-1)".into();
    add_formula_column(&mut app);
    let tab = &mut app.tabs[0];
    for (row, expected) in [20, 15, -1].into_iter().enumerate() {
        assert_eq!(tab.table.get(row, 0), Some(&CellValue::Int(expected)));
    }
    assert!(tab.parse_error_banner.is_none());
    assert!(tab.table.undo());
    assert_eq!(tab.table.col_count(), 3);
    assert_eq!(tab.table.get(0, 0), Some(&CellValue::String("bolt".into())));
}

#[test]
fn lookup_fill_cache_only_admits_a_single_relative_lookup_key() {
    for (source, expected) in [
        ("XLOOKUP($AA002; $A$1:$A$6; $B$1:$B$6; -1)", Some((2, 26))),
        ("xvergleich(b1,$A$1:$A$6,, -1)", Some((1, 1))),
        ("VERGLEICH(C1;$A$1:$A$9;0)", Some((1, 2))),
        ("SVERWEIS(C1;$A$1:$B$9;2;FALSCH)", Some((1, 2))),
        ("HLOOKUP(D1,$A$1:$C$2,2,FALSE)", Some((1, 3))),
        ("WVERWEIS(A1,B$1:$D2,2)", None),
        (
            r#"XLOOKUP(D1,$A$1:$A$6,$B$1:$B$6,IF("A1""(test)"="x",5,-1))"#,
            Some((1, 3)),
        ),
        ("XLOOKUP(D1,$A$1:$A$6,$B$1:$B$6)+ABS(2)", None),
        ("XLOOKUP(D1,$A$1:$A$6,$B$1:$B$6)+2", None),
        ("XLOOKUP(D1,$A$1:$A$6,$B$1:$B$6,E1)", None),
        ("XLOOKUP(D1,A$1:A6,$B$1:$B$6)", None),
        ("XLOOKUP($D$1,$A$1:$A$6,$B$1:$B$6)", None),
        ("XLOOKUP(D1+0,$A$1:$A$6,$B$1:$B$6)", None),
        ("SUM(D1,$A$1,$B$1)", None),
        ("XLOOKUP_FAKE(D1,$A$1:$A$6,$B$1:$B$6)", None),
        ("IF(TRUE,XLOOKUP(D1,$A$1:$A$6,$B$1:$B$6),-1)", None),
        (r#"XLOOKUP("D1",A1:$A$6,$B$1:$B$6)"#, None),
    ] {
        assert_eq!(
            FormulaRows::new(source).memo_inputs(),
            expected.map(|key| vec![key]),
            "{source}"
        );
    }

    // The last representable 1-based reference is valid but out of bounds.
    // On the following row the shift overflows; a cached blank must not
    // turn the unrepresentable reference into another value.
    let table = numeric_grid([CellValue::Null, CellValue::Null]);
    let formula = format!("XLOOKUP(A{},$A$1:$A$2,$A$1:$A$2,-1)", usize::MAX);
    let values = fill_formula_template(&formula, &table).0;
    assert_eq!(values, vec![(0, 0.0)]);
}

#[test]
fn classic_lookup_fill_reuses_fixed_catalogs_without_moving_them() {
    use data::CellValue::{Int, String as Text};
    let table = data::DataTable {
        columns: ["sku", "price", "order"]
            .map(|name| data::ColumnInfo {
                name: name.into(),
                data_type: "Utf8".into(),
            })
            .to_vec(),
        rows: vec![
            vec![Text("nuts".into()), Int(25), Text("bolt".into())],
            vec![Text("bolt".into()), Int(40), Text("nuts".into())],
            vec![Text("bolt".into()), Int(65), Text("bolt".into())],
            vec![Text("nut".into()), Int(90), Text("missing".into())],
        ],
        ..data::DataTable::empty()
    };
    let (values, _) = fill_formula_template("VLOOKUP(C1,$A$1:$B$4,2,FALSE)", &table);
    assert_eq!(values, vec![(0, 40.0), (1, 25.0), (2, 40.0)]);
    let (positions, _) = fill_formula_template("MATCH(C1,$A$1:$A$4,0)", &table);
    assert_eq!(positions, vec![(0, 2.0), (1, 1.0), (2, 2.0)]);
}

#[test]
fn shift_report_fills_overnight_hours_and_can_be_undone() {
    use data::CellValue::Time;
    use data::times::parse_time;
    let time = |s| Time(parse_time(s).unwrap());
    let mut app = workbook_app();
    let rows = vec![
        vec![time("22:30"), time("06:15")],
        vec![time("09:00"), time("17:00")],
        vec![time("12:00"), time("12:00")],
    ];
    app.tabs[0].table = data::DataTable {
        columns: ["Start", "Finish"]
            .map(|name| data::ColumnInfo {
                name: name.into(),
                data_type: "Time64(Microsecond)".into(),
            })
            .to_vec(),
        rows: rows.clone(),
        ..data::DataTable::empty()
    };
    app.tabs[0].new_col_name = "Hours".into();
    app.tabs[0].new_col_formula = "=24*REST(ZEITWERT(B1)-ZEITWERT(A1);1)".into();
    add_formula_column(&mut app);
    let table = &mut app.tabs[0].table;
    assert_eq!(table.col_count(), 3);
    for (formula, expected) in [("C1", 7.75), ("C2", 8.0), ("C3", 0.0)] {
        let hours = data::evaluate_formula(formula, table).unwrap();
        assert!((hours - expected).abs() < 1e-12, "{formula}: {hours}");
    }
    assert_eq!(table.undo_stack.len(), 1);
    table.undo();
    assert_eq!(table.col_count(), 2);
    assert_eq!(table.rows, rows);
}

#[test]
fn german_counta_fills_relative_rows_without_shifting_its_function_name() {
    use data::CellValue::{Bool, Int, Null, String as Text};
    let mut app = workbook_app();
    app.tabs[0].table = data::DataTable {
        columns: ["ID", "approval", "receipt"]
            .map(|name| data::ColumnInfo {
                name: name.into(),
                data_type: "Utf8".into(),
            })
            .to_vec(),
        rows: vec![
            vec![Int(1), Null, Text("".into())],
            vec![Int(2), Bool(false), Int(0)],
            vec![Int(3), Null, Null],
        ],
        ..data::DataTable::empty()
    };
    let formula = "AnZaHl2 (B1:C1)";
    let mut shifted = String::new();
    assert_eq!(
        FormulaRows::new(formula).at(2, &mut shifted),
        Some("AnZaHl2 (B3:C3)")
    );
    assert_eq!(
        FormulaRows::new("ANZAHL2+$ANZAHL2").at(2, &mut shifted),
        Some("ANZAHL4+$ANZAHL4")
    );
    app.tabs[0].new_col_name = "Recorded".into();
    app.tabs[0].new_col_formula = format!("={formula}");
    add_formula_column(&mut app);
    let table = &mut app.tabs[0].table;
    assert_eq!(table.col_count(), 4);
    assert_eq!(table.get(0, 3), Some(&Int(1)));
    assert_eq!(table.get(1, 3), Some(&Int(2)));
    assert_eq!(table.get(2, 3), Some(&Int(0)));
    assert_eq!(table.undo_stack.len(), 1);
    table.undo();
    assert_eq!(table.col_count(), 3);
}

#[test]
fn intl_workday_formulas_shift_dates_but_keep_the_holiday_list_fixed() {
    use data::CellValue::{Date, Null};
    let table = data::DataTable {
        columns: ["start", "finish", "holiday"]
            .map(|name| data::ColumnInfo {
                name: name.into(),
                data_type: "Date32".into(),
            })
            .to_vec(),
        rows: vec![
            vec![
                Date("2024-03-08".into()),
                Date("2024-03-10".into()),
                Date("2024-03-11".into()),
            ],
            vec![Date("2024-03-10".into()), Date("2024-03-12".into()), Null],
        ],
        ..data::DataTable::empty()
    };
    let (days, bad) =
        fill_formula_template(r#"NETWORKDAYS.INTL(A1,B1,"0000110",$C$1:$C$2)"#, &table);
    assert!(bad.is_none());
    assert_eq!(days, vec![(0, 1.0), (1, 2.0)]);
    let (deadline, bad) = fill_formula_template(r#"DAY(WORKDAY.INTL(A1,1,7,$C$1:$C$2))"#, &table);
    assert!(bad.is_none());
    assert_eq!(deadline, vec![(0, 10.0), (1, 12.0)]);
}

#[test]
fn lookup_fill_cache_preserves_types_fallback_and_diagnostics() {
    use data::CellValue::{Bool, Int, Null, String as Text};
    let text = |s: &str| Text(s.into());
    let mut table = data::DataTable {
        columns: ["sku", "price", "fixed default", "request"]
            .map(|name| data::ColumnInfo {
                name: name.into(),
                data_type: "Utf8".into(),
            })
            .to_vec(),
        rows: vec![
            vec![Int(7), Int(70), text("error"), Int(7)],
            vec![text("7"), Int(71), Null, text("7")],
            vec![Bool(true), Int(72), Null, Bool(true)],
            vec![Null, Int(73), Null, Null],
            vec![text("broken"), text("bad price"), Null, text("broken")],
            vec![text("unused"), Int(20), Null, text("miss")],
            vec![Null, Null, Null, text("miss")],
            vec![Null, Null, Null, text("broken")],
            vec![Null, Null, Null, Int(7)],
        ],
        ..data::DataTable::empty()
    };
    let source = "XLOOKUP($D1,$A$1:$A$6,$B$1:$B$6,IFERROR($C$1+1,99))";
    let template = FormulaRows::new(source);
    let expected = vec![
        (0, 70.),
        (1, 71.),
        (2, 72.),
        (3, 73.),
        (5, 99.),
        (6, 99.),
        (8, 70.),
    ];
    let optimized = fill_formula_template(source, &table);
    let normal = fill_shifted_template(
        &template,
        &table,
        &[],
        data::evaluate_formula_with_diagnostics,
    );
    assert_eq!(optimized.0, expected);
    assert_eq!(optimized.0, normal.0);
    let optimized_bad = optimized.1.unwrap();
    let normal_bad = normal.1.unwrap();
    assert_eq!(
        (
            optimized_bad.row,
            optimized_bad.col,
            optimized_bad.content.as_str()
        ),
        (4, 1, "bad price")
    );
    assert_eq!(
        (optimized_bad.row, optimized_bad.col, optimized_bad.content),
        (normal_bad.row, normal_bad.col, normal_bad.content)
    );

    table.set(0, 1, Int(107));
    table.set(6, 3, Bool(true));
    let reloaded = fill_formula_template(source, &table).0;
    assert_eq!(
        reloaded,
        vec![
            (0, 107.),
            (1, 71.),
            (2, 72.),
            (3, 73.),
            (5, 99.),
            (6, 72.),
            (8, 107.)
        ]
    );
}

#[test]
fn lookup_fill_performance_matches_every_reference_row() {
    use data::CellValue;
    const ROWS: usize = 16_000;
    const CATALOG: usize = 1_536;
    const REQUEST_KEYS: usize = 97;
    let table = data::DataTable {
        columns: ["catalog SKU", "price cents", "request SKU"]
            .map(|name| data::ColumnInfo {
                name: name.into(),
                data_type: "Utf8".into(),
            })
            .to_vec(),
        rows: (0..ROWS)
            .map(|i| {
                let key = i % REQUEST_KEYS;
                let catalog_key = CATALOG - 1 - key * 11;
                vec![
                    if i < CATALOG {
                        CellValue::String(format!("SKU-{i:04}"))
                    } else {
                        CellValue::Null
                    },
                    CellValue::Int((i * 3) as i64),
                    CellValue::String(if key + 1 == REQUEST_KEYS {
                        "missing".into()
                    } else {
                        format!("SKU-{catalog_key:04}")
                    }),
                ]
            })
            .collect(),
        ..data::DataTable::empty()
    };
    let source = "XLOOKUP(C1,$A$1:$A$1536,$B$1:$B$1536,-1)";
    let template = FormulaRows::new(source);
    let keys = template.memo_inputs().unwrap();
    assert_eq!(keys, [(1, 2)]);
    let run = |inputs: &[(usize, usize)]| {
        let mut calls = 0;
        let start = std::time::Instant::now();
        let (values, bad) = fill_shifted_template(&template, &table, inputs, |formula, table| {
            calls += 1;
            data::evaluate_formula_with_diagnostics(formula, table)
        });
        let elapsed = start.elapsed().as_nanos();
        assert!(bad.is_none());
        (values, calls, elapsed)
    };
    let mut baseline = [0; 3];
    let mut candidate = [0; 3];
    for i in 0..3 {
        let (old, new) = if i % 2 == 0 {
            (run(&[]), run(&keys))
        } else {
            let new = run(&keys);
            (run(&[]), new)
        };
        assert_eq!(old.1, ROWS);
        assert_eq!(new.1, REQUEST_KEYS);
        assert_eq!(new.0, old.0);
        assert_eq!(new.0.len(), ROWS);
        for (row, value) in new.0 {
            let key = row % REQUEST_KEYS;
            let expected = if key + 1 == REQUEST_KEYS {
                -1.0
            } else {
                ((CATALOG - 1 - key * 11) * 3) as f64
            };
            assert_eq!(value, expected, "row {row}");
        }
        baseline[i] = old.2;
        candidate[i] = new.2;
    }
    baseline.sort_unstable();
    candidate.sort_unstable();
    println!(
        "lookup_fill_bench rows={ROWS} reference_calls={ROWS} memo_calls={REQUEST_KEYS} baseline_ns={} candidate_ns={} samples=3",
        baseline[1], candidate[1]
    );
}

#[test]
fn criteria_fill_cache_proves_each_moving_argument_is_a_bare_criterion() {
    for (source, expected) in [
        ("COUNTIF($A$1:$A$20,B1)", Some(vec![(1, 1)])),
        ("SUMMEWENN($A$1:$A$20;$B3;$C$1:$C$20)", Some(vec![(3, 1)])),
        (
            r#"COUNTIFS($A$1:$A$20,B1,$B$1:$B$20,"A1,(;)",$C$1:$C$20,D1)"#,
            Some(vec![(1, 1), (1, 3)]),
        ),
        ("ZÄHLENWENNS($A$1;$B1;$C$1;D1)", Some(vec![(1, 1), (1, 3)])),
        (
            "AVERAGEIFS($D$1:$D$8,$A$1:$A$8,B1,$C$1:$C$8,D1,$E$1:$E$8,F2)",
            Some(vec![(1, 1), (1, 3), (2, 5)]),
        ),
        ("SUMIFS($D$1:$D8,$A$1:$A$8,B1,$C$1:$C$8,D1)", None),
        ("COUNTIF(A$1:A20,B1)", None),
        ("COUNTIFS($A$1:$A$20,B1+0)", None),
        ("COUNTIFS($A$1:$A$20,B1)+COUNTIF($A$1:$A$20,B1)", None),
        (r#"COUNTIFS($A$1:$A$20,">="&B1)"#, None),
        ("COUNTIF($A$1:$A$20,IF(TRUE,B1,0))", None),
    ] {
        assert_eq!(FormulaRows::new(source).memo_inputs(), expected, "{source}");
    }
}

#[test]
fn criteria_fill_cache_keeps_loaded_blanks_distinct_from_missing_rows() {
    use data::CellValue::{Bool, Int, Null, String as Text};
    let table = data::DataTable {
        columns: ["category", "amount", "criterion"]
            .map(|name| data::ColumnInfo {
                name: name.into(),
                data_type: "Utf8".into(),
            })
            .to_vec(),
        rows: vec![
            vec![Null, Int(1), Null],
            vec![Int(0), Int(2), Null],
            vec![Text("0".into()), Int(4), Bool(false)],
            vec![Bool(false), Int(8), Int(0)],
            vec![
                Text("bad".into()),
                Text("not a number".into()),
                Text("bad".into()),
            ],
            vec![Text("other".into()), Int(16), Text("bad".into())],
        ],
        ..data::DataTable::empty()
    };
    let formula = "AVERAGEIF($A$1:$A$6,C2,$B$1:$B$6)";
    let template = FormulaRows::new(formula);
    let memo = template.memo_inputs().unwrap();
    assert_eq!(memo, [(2, 2)]);
    let run = |keys: &[(usize, usize)]| {
        let mut calls = 0;
        let (results, bad) = fill_shifted_template(&template, &table, keys, |shifted, table| {
            calls += 1;
            data::evaluate_formula_with_diagnostics(shifted, table)
        });
        assert!(bad.is_none());
        (results, calls)
    };
    let (uncached, unoptimized_calls) = run(&[]);
    let (cached, optimized_calls) = run(&memo);
    assert_eq!(uncached, [(0, 3.0), (1, 8.0), (2, 3.0)]);
    assert_eq!(cached, uncached);
    assert_eq!((unoptimized_calls, optimized_calls), (6, 5));
}

#[test]
fn criteria_fill_cache_distinguishes_typed_dates_from_equal_text() {
    use data::CellValue::{Date, DateTime, Int, String as Text};
    let table = data::DataTable {
        columns: ["source", "criterion"]
            .map(|name| data::ColumnInfo {
                name: name.into(),
                data_type: "Utf8".into(),
            })
            .to_vec(),
        rows: vec![
            vec![Date("2024-03-15".into()), Date("2024-03-15".into())],
            vec![Int(45_366), Text("2024-03-15".into())],
            vec![Int(45_366), Date("2024-03-15".into())],
            vec![Text("2024-03-15".into()), Text("2024-03-15".into())],
            vec![
                DateTime("2024-03-15 12:00:00".into()),
                DateTime("2024-03-15 12:00:00".into()),
            ],
            vec![
                data::CellValue::Float(45_366.5),
                Text("2024-03-15 12:00:00".into()),
            ],
        ],
        ..data::DataTable::empty()
    };
    let formula = "COUNTIF($A$1:$A$6,B1)";
    let template = FormulaRows::new(formula);
    let keys = template.memo_inputs().unwrap();
    let normal = fill_shifted_template(
        &template,
        &table,
        &[],
        data::evaluate_formula_with_diagnostics,
    );
    let mut calls = 0;
    let memo = fill_shifted_template(&template, &table, &keys, |formula, table| {
        calls += 1;
        data::evaluate_formula_with_diagnostics(formula, table)
    });
    assert_eq!(
        normal.0,
        [(0, 3.0), (1, 2.0), (2, 3.0), (3, 2.0), (4, 2.0), (5, 1.0)]
    );
    assert_eq!(memo.0, normal.0);
    assert!(memo.1.is_none());
    assert!(normal.1.is_none());
    assert_eq!(calls, 4);
}

#[test]
fn criteria_fill_bench_matches_each_group_total_using_the_real_evaluator() {
    use data::CellValue::{Int, String as Text};
    const ROWS: usize = 3_000;
    const GROUPS: usize = 12;
    let mut table = data::DataTable {
        columns: ["region", "status", "amount"]
            .map(|name| data::ColumnInfo {
                name: name.into(),
                data_type: "Utf8".into(),
            })
            .to_vec(),
        rows: (0..ROWS)
            .map(|i| {
                vec![
                    Text(["North", "South", "East", "West"][i % 4].into()),
                    Text(["Paid", "Open", "Refunded"][(i / 4) % 3].into()),
                    Int((i % 17 + 1) as i64),
                ]
            })
            .collect(),
        ..data::DataTable::empty()
    };
    table.set(23, 2, Int(200));
    let mut totals = [0.0; GROUPS];
    for row in 0..ROWS {
        let Some(Int(value)) = table.get(row, 2) else {
            panic!("missing amount")
        };
        totals[row % GROUPS] += *value as f64;
    }
    let formula = "SUMIFS($C$1:$C$3000,$A$1:$A$3000,A1,$B$1:$B$3000,B1)";
    let template = FormulaRows::new(formula);
    let keys = template.memo_inputs().unwrap();
    assert_eq!(keys, [(1, 0), (1, 1)]);
    let run = |inputs: &[(usize, usize)]| {
        let mut calls = 0;
        let start = std::time::Instant::now();
        let (results, bad) = fill_shifted_template(&template, &table, inputs, |shifted, table| {
            calls += 1;
            data::evaluate_formula_with_diagnostics(shifted, table)
        });
        let elapsed = start.elapsed().as_nanos();
        assert!(bad.is_none());
        (results, calls, elapsed)
    };
    let mut base = [0; 3];
    let mut memo = [0; 3];
    for i in 0..3 {
        let (old, new) = if i % 2 == 0 {
            (run(&[]), run(&keys))
        } else {
            let new = run(&keys);
            (run(&[]), new)
        };
        assert_eq!(old.1, ROWS);
        assert_eq!(new.1, GROUPS);
        assert_eq!(new.0, old.0);
        assert_eq!(new.0.len(), ROWS);
        for (row, value) in new.0 {
            assert_eq!(value, totals[row % GROUPS], "row {row}");
        }
        base[i] = old.2;
        memo[i] = new.2;
    }
    base.sort_unstable();
    memo.sort_unstable();
    println!(
        "criteria_fill_bench rows={ROWS} reference_calls={ROWS} memo_calls={GROUPS} baseline_ns={} candidate_ns={} samples=3",
        base[1], memo[1]
    );
}

#[test]
fn sumproduct_formula_column_uses_fixed_weights_and_shifted_prices() {
    let mut app = workbook_app();
    let tab = &mut app.tabs[0];
    tab.table.columns = ["price", "weight"]
        .map(|name| data::ColumnInfo {
            name: name.into(),
            data_type: "Int64".into(),
        })
        .to_vec();
    tab.table.rows = [(10, 2), (20, 3), (30, 7)]
        .into_iter()
        .map(|(price, weight)| vec![data::CellValue::Int(price), data::CellValue::Int(weight)])
        .collect();
    tab.new_col_name = "weighted next two prices".into();
    tab.new_col_formula = "=SUMPRODUCT(A1:A2,$B$1:$B$2)".into();
    tab.insert_col_at = Some(0);
    add_formula_column(&mut app);
    let tab = &mut app.tabs[0];
    for (row, expected) in [80, 130, 60].into_iter().enumerate() {
        assert_eq!(tab.table.get(row, 0), Some(&data::CellValue::Int(expected)));
    }
    assert!(tab.parse_error_banner.is_none());
    assert!(tab.table.undo());
    assert_eq!(tab.table.col_count(), 2);
    assert_eq!(tab.table.get(2, 0), Some(&data::CellValue::Int(30)));
}

#[test]
fn scientific_formula_fill_moves_cell_addresses_but_not_exponents() {
    let mut app = workbook_app();
    let tab = &mut app.tabs[0];
    tab.table.columns = ["price", "fee", "spare 1", "spare 2", "per-row adjustment"]
        .map(|name| data::ColumnInfo {
            name: name.into(),
            data_type: "Int64".into(),
        })
        .to_vec();
    tab.table.rows = [(100, 10, 1), (200, 30, 2), (300, 90, 3)]
        .into_iter()
        .map(|(price, fee, adjustment)| {
            vec![
                data::CellValue::Int(price),
                data::CellValue::Int(fee),
                data::CellValue::Null,
                data::CellValue::Null,
                data::CellValue::Int(adjustment),
            ]
        })
        .collect();
    let source = "ROUND(A1*(1+1e1%)+$B$1+E1+1E-1,2)";
    let template = FormulaRows::new(source);
    let mut shifted = String::new();
    assert_eq!(
        template.at(2, &mut shifted),
        Some("ROUND(A3*(1+1e1%)+$B$1+E3+1E-1,2)")
    );
    tab.new_col_name = "discounted price forecast".into();
    tab.new_col_formula = format!("={source}");
    tab.insert_col_at = Some(0);
    add_formula_column(&mut app);
    let tab = &mut app.tabs[0];
    for (row, expected) in [121.1, 232.1, 343.1].into_iter().enumerate() {
        assert_eq!(
            tab.table.get(row, 0),
            Some(&data::CellValue::Float(expected))
        );
    }
    assert!(tab.parse_error_banner.is_none());
    assert!(tab.table.undo());
    assert_eq!(tab.table.col_count(), 5);
    assert_eq!(tab.table.get(2, 0), Some(&data::CellValue::Int(300)));
}

#[test]
fn ifs_formula_column_combines_tests_and_keeps_threshold_anchored() {
    use data::CellValue::{Int, Null, String as Text};
    let mut app = workbook_app();
    let tab = &mut app.tabs[0];
    tab.table.columns = ["order amount", "status", "minimum"]
        .map(|name| data::ColumnInfo {
            name: name.into(),
            data_type: "Utf8".into(),
        })
        .to_vec();
    tab.table.rows = vec![
        vec![Int(120), Text("Paid".into()), Int(100)],
        vec![Int(400), Text("Pending".into()), Null],
        vec![Int(50), Text("Refunded".into()), Null],
        vec![Int(80), Text("Paid".into()), Null],
    ];
    tab.insert_col_at = Some(0);
    tab.new_col_name = "commission tiers".into();
    tab.new_col_formula =
        r#"=WENNS(UND(A1>=$C$1;B1="Paid");A1*10%;B1="Refunded";-A1;WAHR;0)"#.into();
    add_formula_column(&mut app);
    let tab = &mut app.tabs[0];
    for (row, expected) in [12, 0, -50, 0].into_iter().enumerate() {
        assert_eq!(tab.table.get(row, 0), Some(&Int(expected)));
    }
    assert!(tab.parse_error_banner.is_none());
    assert!(tab.table.undo());
    assert_eq!(tab.table.col_count(), 3);
    assert_eq!(tab.table.get(0, 0), Some(&Int(120)));
}

#[test]
fn extrema_fill_reuses_only_identical_typed_clock_and_elapsed_criteria() {
    use data::CellValue::{Duration, Int, Time};
    use data::times::{NANOS_PER_DAY, parse_time};
    let evening = Time(parse_time("18:00").unwrap());
    let night = Time(parse_time("22:00").unwrap());
    let morning = Time(parse_time("06:00").unwrap());
    let long = Duration(NANOS_PER_DAY + NANOS_PER_DAY / 8);
    let short = Duration(NANOS_PER_DAY / 4);
    let rows = [
        (6, &evening, &long, &evening, &long),
        (5, &night, &long, &night, &long),
        (-4, &night, &long, &morning, &short),
        (8, &evening, &short, &evening, &long),
        (100, &morning, &long, &night, &long),
        (-1, &night, &short, &morning, &short),
    ]
    .into_iter()
    .map(|(n, b, c, d, e)| vec![Int(n), b.clone(), c.clone(), d.clone(), e.clone()])
    .collect();
    let table = data::DataTable {
        columns: ["Orders", "Clock", "Elapsed", "Match clock", "Match length"]
            .map(|name| data::ColumnInfo {
                name: name.into(),
                data_type: "Utf8".into(),
            })
            .to_vec(),
        rows,
        ..data::DataTable::empty()
    };
    let formula = "MAXWENNS($A$1:$A$6;$B$1:$B$6;D1;$C$1:$C$6;E1)";
    let template = FormulaRows::new(formula);
    let keys = template.memo_inputs().unwrap();
    assert_eq!(keys, [(1, 3), (1, 4)]);
    assert!(
        FormulaRows::new("MAXIFS($A$1:$A6,$B$1:$B$6,D1)")
            .memo_inputs()
            .is_none()
    );
    let mut calls = 0;
    let cached = fill_shifted_template(&template, &table, &keys, |expr, table| {
        calls += 1;
        data::evaluate_formula_with_diagnostics(expr, table)
    });
    let uncached = fill_shifted_template(
        &template,
        &table,
        &[],
        data::evaluate_formula_with_diagnostics,
    );
    assert_eq!(cached.0, uncached.0);
    assert!(cached.1.is_none() && uncached.1.is_none());
    assert_eq!(
        cached.0,
        [(0, 6.0), (1, 5.0), (2, 0.0), (3, 6.0), (4, 5.0), (5, 0.0)]
    );
    assert_eq!(calls, 3);

    let mut app = workbook_app();
    app.tabs[0].table = table;
    app.tabs[0].new_col_name = "Maximum".into();
    app.tabs[0].new_col_formula = format!("={formula}");
    add_formula_column(&mut app);
    let table = &mut app.tabs[0].table;
    assert_eq!(table.col_count(), 6);
    for (row, value) in [6, 5, 0, 6, 5, 0].into_iter().enumerate() {
        assert_eq!(table.get(row, 5), Some(&Int(value)));
    }
    assert_eq!(table.undo_stack.len(), 1);
    table.undo();
    assert_eq!(table.col_count(), 5);
}

#[test]
fn preview_reads_three_original_rows_with_edits_and_a_nonnumeric_diagnostic() {
    use data::CellValue::{Int, String as Text};
    let mut app = workbook_app();
    let tab = &mut app.tabs[0];
    tab.table = numeric_grid([Int(2), Text("not numeric".into()), Int(5), Int(8)]);
    tab.table.set(0, 0, Int(3));
    tab.table.row_offset = 1000;
    tab.filtered_rows = vec![2, 3];
    let before = tab.table.undo_stack.len();

    tab.new_col_formula = "=A1+$A$1".into();
    let preview = FormulaPreview::read(&tab.new_col_formula, &tab.table).unwrap();
    assert_eq!(preview.values, [Some(6.0), None, Some(8.0)]);
    let bad = preview.first_bad.as_ref().unwrap();
    assert_eq!(
        (bad.row, bad.col, bad.content.as_str()),
        (1, 0, "not numeric")
    );
    assert!(preview.still_matches(tab));
    assert_eq!(tab.table.col_count(), 1);
    assert_eq!(tab.table.undo_stack.len(), before);
    assert_eq!(tab.table.get(0, 0), Some(&Int(3)));
    assert_eq!(tab.table.edits.len(), 1);
    assert_eq!(tab.filtered_rows, [2, 3]);

    tab.table.set(0, 0, Int(4));
    assert!(!preview.still_matches(tab));
    assert!(tab.new_col_preview.is_none());
}

#[test]
fn preview_refuses_empty_input_or_no_loaded_rows_and_keeps_zero_distinct() {
    let table = numeric_grid([data::CellValue::Int(7), data::CellValue::Int(3)]);
    for invalid in ["", "  ", "A1 * 2", " =  "] {
        assert!(FormulaPreview::read(invalid, &table).is_none(), "{invalid}");
    }
    assert!(FormulaPreview::read("=7", &data::DataTable::empty()).is_none());
    assert_eq!(
        FormulaPreview::read("=0", &table).unwrap().values,
        [Some(0.0), Some(0.0)]
    );
    assert_eq!(
        FormulaPreview::read("=1/0", &table).unwrap().values,
        [None, None]
    );
}

#[test]
fn keyboard_preview_reads_sample_then_dismisses_it_when_the_formula_changes() {
    use egui::accesskit::{Action, ActionRequest, TreeId};
    use octa::i18n::t;

    fn frame(app: &mut OctaApp, ctx: &egui::Context, events: Vec<egui::Event>) -> egui::FullOutput {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(960.0, 720.0),
            )),
            events,
            ..Default::default()
        };
        let mut out = ctx.run_ui(raw, |ui| render_add_column_dialog(app, ui.ctx()));
        out.textures_delta.clear();
        out
    }
    fn has(out: &egui::FullOutput, text: &str) -> bool {
        out.shapes.iter().any(|s| match &s.shape {
            egui::Shape::Text(shape) => {
                shape.galley.text() == text && s.clip_rect.intersects(shape.visual_bounding_rect())
            }
            _ => false,
        })
    }

    let mut app = workbook_app();
    let tab = &mut app.tabs[0];
    tab.table = numeric_grid([2, 4, 6, 101].map(data::CellValue::Int));
    tab.filter_dirty = false;
    tab.show_add_column_dialog = true;
    tab.new_col_formula = "=A1+$A$1".into();
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    frame(&mut app, &ctx, vec![]);
    let opened = frame(&mut app, &ctx, vec![]);
    let name = t("dialog.pv_preview");
    let tree = opened.platform_output.accesskit_update.as_ref().unwrap();
    let (button, _) = tree
        .nodes
        .iter()
        .find(|(_, node)| {
            node.label() == Some(name.as_str()) && node.supports_action(Action::Focus)
        })
        .expect("Preview is a named, focusable control");
    let key = egui::Event::Key {
        key: egui::Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    let focused = frame(
        &mut app,
        &ctx,
        vec![egui::Event::AccessKitActionRequest(ActionRequest {
            action: Action::Focus,
            target_tree: TreeId::ROOT,
            target_node: *button,
            data: None,
        })],
    );
    assert_eq!(
        focused
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .focus,
        *button
    );
    frame(&mut app, &ctx, vec![key]);
    let sample = frame(&mut app, &ctx, vec![]);
    let row = t("dialog.formula_row");
    assert!(has(&sample, &format!("{row} 1: 4")));
    assert!(has(&sample, &format!("{row} 3: 8")));
    assert!(!has(&sample, &format!("{row} 4: 103")));
    let tab = &app.tabs[0];
    assert!(tab.show_add_column_dialog);
    assert_eq!(tab.table.col_count(), 1);
    assert!(tab.table.undo_stack.is_empty());

    app.tabs[0].new_col_formula = "=A1+100".into();
    let cleared = frame(&mut app, &ctx, vec![]);
    assert!(!has(&cleared, &format!("{row} 1: 4")));
    assert!(app.tabs[0].new_col_preview.is_none());
    assert_eq!(app.tabs[0].table.col_count(), 1);
}

#[test]
fn percentile_fill_reuses_identical_requests_only_for_the_fixed_range() {
    use CellValue::{Float, Int};
    let percentiles = [0.95, 0.5, 1.0, 0.95, 0.5, 1.0];
    let table = data::DataTable {
        columns: ["Response ms", "Percentile"]
            .map(|name| ColumnInfo {
                name: name.into(),
                data_type: "Float64".into(),
            })
            .to_vec(),
        rows: [100, 120, 160, 200, 300, 500]
            .into_iter()
            .zip(percentiles)
            .map(|(ms, pct)| vec![Int(ms), Float(pct)])
            .collect(),
        ..data::DataTable::empty()
    };
    let formula = "QUANTIL.INKL($A$1:$A$6;B1)";
    let template = FormulaRows::new(formula);
    let keys = template.memo_inputs().unwrap();
    assert_eq!(keys, [(1, 1)]);
    for moving in [
        "QUANTIL.INKL($A$1:A6;B1)",
        "QUARTILE.EXKL(A1:$A$6;B1)",
        "PERCENTILE.INC($A$1:$A$6,B1+0)",
    ] {
        assert!(FormulaRows::new(moving).memo_inputs().is_none(), "{moving}");
    }
    assert_eq!(
        FormulaRows::new("QUARTILE.EXKL($A$1:$A$6;B1)").memo_inputs(),
        Some(vec![(1, 1)])
    );
    let mut calls = 0;
    let cached = fill_shifted_template(&template, &table, &keys, |expr, table| {
        calls += 1;
        data::evaluate_formula_with_diagnostics(expr, table)
    });
    let direct = fill_shifted_template(
        &template,
        &table,
        &[],
        data::evaluate_formula_with_diagnostics,
    );
    assert_eq!(cached.0, direct.0);
    assert!(cached.1.is_none() && direct.1.is_none());
    assert_eq!(
        cached.0,
        [
            (0, 450.0),
            (1, 180.0),
            (2, 500.0),
            (3, 450.0),
            (4, 180.0),
            (5, 500.0)
        ]
    );
    assert_eq!(calls, 3);

    let mut app = workbook_app();
    app.tabs[0].table = table;
    app.tabs[0].new_col_name = "Latency threshold".into();
    app.tabs[0].new_col_formula = format!("={formula}");
    add_formula_column(&mut app);
    let table = &mut app.tabs[0].table;
    assert_eq!(table.col_count(), 3);
    for (r, v) in [450, 180, 500, 450, 180, 500].into_iter().enumerate() {
        assert_eq!(table.get(r, 2), Some(&Int(v)));
    }
    assert_eq!(table.undo_stack.len(), 1);
    table.undo();
    assert_eq!(table.col_count(), 2);
}

#[test]
fn a_dotted_function_name_preserves_the_formula_column_relative_rows() {
    let mut buffer = String::new();
    for (base, shifted) in [
        ("PERCENTILE.EXC(A1:A$9,B1)", "PERCENTILE.EXC(A4:A$9,B4)"),
        ("QuArTiLe.InKl(A1:A3;$B$1)", "QuArTiLe.InKl(A4:A6;$B$1)"),
        ("NETWORKDAYS.INTL(A1,A$2)", "NETWORKDAYS.INTL(A4,A$2)"),
    ] {
        assert_eq!(FormulaRows::new(base).at(3, &mut buffer), Some(shifted));
    }
}

#[test]
fn running_dispersion_matches_the_shifted_evaluator_with_blanks_text_and_edits() {
    use CellValue::{Bool, Float, Int, Null, String as Text};
    let mut table = numeric_grid([
        Null,
        Int(6),
        Text(" 10 ".into()),
        Bool(true),
        Float(f64::NAN),
        Int(20),
        Int(32),
        Null,
    ]);
    table.set(6, 0, Int(26));
    table.row_offset = 1000;
    for formula in ["STABW.S(A$1:A1)", "VAR.P($A2:$A$1)", "VARIANZEN(A$4:A4)"] {
        let candidate = fill_running_aggregate(formula, &table).unwrap();
        let direct = fill_formula_template(formula, &table);
        assert!(direct.1.is_none(), "{formula}");
        assert_eq!(candidate, direct.0, "{formula}");
    }
    for invalid in ["VAR.P(A1:A4)", "VAR.P(A$3:B3)", "VAR.P(A$1:A1,2)"] {
        assert!(
            fill_running_aggregate(invalid, &table).is_none(),
            "{invalid}"
        );
    }
    assert!(
        fill_running_aggregate("STDEV.P(B$1:B1)", &table)
            .unwrap()
            .is_empty()
    );

    let mut app = workbook_app();
    app.tabs[0].table = numeric_grid([Int(96), Int(100), Int(104)]);
    app.tabs[0].new_col_name = "Running deviation".into();
    app.tabs[0].new_col_formula = "=STABW.S(A$1:A1)".into();
    app.tabs[0].insert_col_at = Some(0);
    add_formula_column(&mut app);
    let tab = &mut app.tabs[0];
    assert_eq!(tab.table.col_count(), 2);
    assert_eq!(tab.table.get(0, 0), Some(&Null));
    assert_eq!(tab.table.get(2, 0), Some(&Int(4)));
    assert_eq!(tab.table.get(2, 1), Some(&Int(104)));
    assert_eq!(tab.table.undo_stack.len(), 1);
    tab.table.undo();
    assert_eq!(tab.table.col_count(), 1);
}

#[test]
fn running_dispersion_handles_a_large_prefix_without_rescanning_it() {
    let rows = 25_000usize;
    let table =
        numeric_grid((0..rows).map(|r| CellValue::Int(if r % 2 == 0 { 1000 } else { 1002 })));
    let actual = fill_running_aggregate("STDEV.P(A$1:A1)", &table).unwrap();
    assert_eq!(actual.len(), rows);
    for index in [0, 1, 10, 1201, 19_500, rows - 1] {
        let n = index + 1;
        let hi = (n / 2) as f64 / n as f64;
        let expected = 2.0 * (hi * (1.0 - hi)).sqrt();
        let (r, got) = actual[index];
        assert_eq!(r, index);
        assert!(
            (got - expected).abs() < 1e-12,
            "{index}: {got} != {expected}"
        );
    }
}

#[test]
fn formula_examples_are_keyboard_searchable_cancellable_and_return_to_the_formula() {
    use egui::accesskit::{Action, ActionRequest, TreeId};
    use octa::i18n::{current_language, t};

    fn frame(app: &mut OctaApp, ctx: &egui::Context, events: Vec<egui::Event>) -> egui::FullOutput {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(920., 690.),
                )),
                events,
                ..Default::default()
            },
            |ui| render_add_column_dialog(app, ui.ctx()),
        );
        output.textures_delta.clear();
        output
    }

    fn key(key: egui::Key) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    let mut app = workbook_app();
    let tab = &mut app.tabs[0];
    tab.table = data::DataTable {
        columns: ["Other", "Measured", "Hidden"]
            .map(|name| ColumnInfo {
                name: name.into(),
                data_type: "Int64".into(),
            })
            .to_vec(),
        rows: [2, 4, 6, 8]
            .map(|n| vec![CellValue::Int(99), CellValue::Int(n), CellValue::Int(3)])
            .to_vec(),
        ..data::DataTable::empty()
    };
    tab.show_add_column_dialog = true;
    tab.filter_dirty = false;
    tab.hidden_columns.insert(2);
    tab.table_state.selected_cell = Some((0, 1));
    tab.new_col_formula = "=99".into();
    let formula_id = formula_input_id(egui::Id::new("octa_add_column_dialog"), tab);
    let ctx = egui::Context::default();
    ctx.enable_accesskit();

    frame(&mut app, &ctx, vec![]);
    let opened = frame(&mut app, &ctx, vec![]);
    let name = t("dialog.formula_examples");
    let (button_id, _) = opened
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .find(|(_, node)| node.label() == Some(&name) && node.supports_action(Action::Focus))
        .expect("formula examples button");
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::AccessKitActionRequest(ActionRequest {
            action: Action::Focus,
            target_tree: TreeId::ROOT,
            target_node: *button_id,
            data: None,
        })],
    );
    frame(&mut app, &ctx, vec![key(egui::Key::Enter)]);
    frame(&mut app, &ctx, vec![]);
    assert!(app.tabs[0].new_col_examples.is_some());
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::Text("does not exist".into())],
    );
    frame(&mut app, &ctx, vec![key(egui::Key::Enter)]);
    assert_eq!(app.tabs[0].new_col_formula, "=99");
    assert!(app.tabs[0].new_col_examples.is_some());
    frame(&mut app, &ctx, vec![key(egui::Key::Escape)]);
    assert_eq!(app.tabs[0].new_col_formula, "=99");
    assert!(app.tabs[0].new_col_examples.is_none());
    assert!(ctx.memory(|m| m.has_focus(formula_id)));

    app.open_formula_examples();
    frame(&mut app, &ctx, vec![]);
    let matched = frame(&mut app, &ctx, vec![egui::Event::Text("stabw.s".into())]);
    let expected = if current_language() == "de" {
        "=STABW.S(B$1:B1)"
    } else {
        "=STDEV.S(B$1:B1)"
    };
    let visible = matched.shapes.iter().any(|item| {
        matches!(&item.shape, egui::Shape::Text(shape) if shape.galley.text()==expected
            && item.clip_rect.intersects(shape.visual_bounding_rect()))
    });
    assert!(visible, "expected rendered example {expected}");
    assert_eq!(app.tabs[0].new_col_formula, "=99");
    frame(&mut app, &ctx, vec![key(egui::Key::Enter)]);
    let tab = &app.tabs[0];
    assert_eq!(tab.new_col_formula, expected);
    assert!(tab.new_col_examples.is_none());
    assert!(ctx.memory(|m| m.has_focus(formula_id)));
    assert_eq!(tab.table.col_count(), 3);
    assert!(tab.table.undo_stack.is_empty());
    let preview = FormulaPreview::read(&tab.new_col_formula, &tab.table).unwrap();
    assert_eq!(preview.values[0], None);
    assert_eq!(preview.values[2], Some(2.0));
}

#[test]
fn imported_number_preview_and_cached_fill_keep_errors_separate_from_zero() {
    use CellValue::{Float, Int, Null, String as Text};
    let inputs = ["1.250,50", "N/A", " ", "1.250,50", "N/A", " "];
    let mut app = workbook_app();
    let tab = &mut app.tabs[0];
    tab.table = numeric_grid(inputs.map(|value| Text(value.into())));
    tab.table.row_offset = 100;
    let formula = r#"ZAHLENWERT(A1;",";".")"#;
    tab.new_col_formula = format!("={formula}");
    tab.new_col_name = "Parsed EUR".into();
    tab.new_col_type = "Float64".into();

    let sample = FormulaPreview::read(&tab.new_col_formula, &tab.table).unwrap();
    assert_eq!(sample.values, [Some(1250.5), None, Some(0.0)]);
    assert_eq!(sample.first_bad.as_ref().unwrap().row, 1);
    assert_eq!(tab.table.col_count(), 1);
    assert!(tab.table.undo_stack.is_empty());

    let template = FormulaRows::new(formula);
    let keys = template.memo_inputs().unwrap();
    assert_eq!(keys, [(1, 0)]);
    assert!(
        FormulaRows::new(r#"NUMBERVALUE(A1,B1,",")"#)
            .memo_inputs()
            .is_none(),
        "the moving separator must not be memoised from the value alone"
    );
    let mut calls = 0;
    let (cached, error) = fill_shifted_template(&template, &tab.table, &keys, |expr, table| {
        calls += 1;
        data::evaluate_formula_with_diagnostics(expr, table)
    });
    assert_eq!(calls, 3);
    let (direct, direct_error) = fill_shifted_template(
        &template,
        &tab.table,
        &[],
        data::evaluate_formula_with_diagnostics,
    );
    assert_eq!(cached, direct);
    assert_eq!(error.unwrap().content, direct_error.unwrap().content);
    assert_eq!(cached, [(0, 1250.5), (2, 0.0), (3, 1250.5), (5, 0.0)]);

    add_formula_column(&mut app);
    let table = &mut app.tabs[0].table;
    assert_eq!(table.get(0, 1), Some(&Float(1250.5)));
    assert_eq!(table.get(1, 1), Some(&Null));
    assert_eq!(table.get(2, 1), Some(&Int(0)));
    assert_eq!(table.undo_stack.len(), 1);
    table.undo();
    assert_eq!(table.col_count(), 1);
    assert_eq!(table.get(1, 0), Some(&Text("N/A".into())));
}

#[test]
fn paired_statistics_use_the_same_fixed_sample_in_preview_and_fill() {
    let mut app = workbook_app();
    let tab = &mut app.tabs[0];
    tab.table = data::DataTable {
        columns: ["Outdoor", "Consumption"]
            .map(|name| ColumnInfo {
                name: name.into(),
                data_type: "Int64".into(),
            })
            .to_vec(),
        rows: (0..80_000)
            .map(|x| vec![CellValue::Int(2 * x), CellValue::Int(5 * x + 2)])
            .collect(),
        ..data::DataTable::empty()
    };
    tab.new_col_name = "Consumption per unit".into();
    tab.new_col_type = "Float64".into();
    tab.new_col_formula = "=STEIGUNG($B$1:$B$80000;$A$1:$A$80000)".into();
    assert!(
        FormulaRows::new(&tab.new_col_formula[1..])
            .relative
            .is_empty()
    );
    let preview = FormulaPreview::read(&tab.new_col_formula, &tab.table).unwrap();
    assert!(preview.first_bad.is_none());
    for value in preview.values {
        assert!((value.unwrap() - 2.5).abs() < 1e-10);
    }
    add_formula_column(&mut app);
    let tab = &mut app.tabs[0];
    for row in [0, 32_000, 79_999] {
        let Some(CellValue::Float(value)) = tab.table.get(row, 2) else {
            panic!("missing numeric result for {row}");
        };
        assert!((value - 2.5).abs() < 1e-10, "{value}");
    }
    tab.table.undo();
    assert_eq!(tab.table.col_count(), 2);
}
