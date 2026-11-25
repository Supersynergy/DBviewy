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
        let actual = fill_running_sum(formula, &table).unwrap();
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
        "COUNT(A$1:A1)",
        "SUM(A$0:A1)",
        "SUM(A$$1:A1)",
        "SUM(A$1:A 1)",
        "SUM(\u{00a0}A$1:A1)",
        "SUM(XXXXXXXXXXXXXXXXXXXXXX$1:XXXXXXXXXXXXXXXXXXXXXX1)",
    ] {
        assert!(fill_running_sum(formula, &table).is_none(), "{formula}");
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
            let result = fill_running_sum(formula, &table).unwrap();
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
            FormulaRows::new(source).repeated_lookup_key(),
            expected,
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
        None,
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
    let key = template.repeated_lookup_key();
    assert_eq!(key, Some((1, 2)));
    let run = |lookup| {
        let mut calls = 0;
        let start = std::time::Instant::now();
        let (values, bad) = fill_shifted_template(&template, &table, lookup, |formula, table| {
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
            (run(None), run(key))
        } else {
            let new = run(key);
            (run(None), new)
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
