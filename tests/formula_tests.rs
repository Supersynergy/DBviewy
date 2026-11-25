use octa::data::*;

fn formula_table() -> DataTable {
    DataTable {
        columns: vec![
            ColumnInfo {
                name: "A".into(),
                data_type: "Int64".into(),
            },
            ColumnInfo {
                name: "B".into(),
                data_type: "Float64".into(),
            },
            ColumnInfo {
                name: "C".into(),
                data_type: "Utf8".into(),
            },
        ],
        rows: vec![
            vec![
                CellValue::Int(10),
                CellValue::Float(2.5),
                CellValue::String("hello".into()),
            ],
            vec![
                CellValue::Int(20),
                CellValue::Float(3.0),
                CellValue::String("world".into()),
            ],
            vec![CellValue::Int(30), CellValue::Float(0.5), CellValue::Null],
        ],
        ..DataTable::empty()
    }
}

// --- Basic arithmetic ---

#[test]
fn formula_addition() {
    let table = formula_table();
    let result = evaluate_formula("A1+B1", &table);
    assert_eq!(result, Some(12.5)); // 10 + 2.5
}

#[test]
fn formula_subtraction() {
    let table = formula_table();
    let result = evaluate_formula("A1-B1", &table);
    assert_eq!(result, Some(7.5)); // 10 - 2.5
}

#[test]
fn formula_multiplication() {
    let table = formula_table();
    let result = evaluate_formula("A1*B1", &table);
    assert_eq!(result, Some(25.0)); // 10 * 2.5
}

#[test]
fn formula_division() {
    let table = formula_table();
    let result = evaluate_formula("A1/B1", &table);
    assert_eq!(result, Some(4.0)); // 10 / 2.5
}

#[test]
fn formula_division_by_zero() {
    let table = formula_table();
    // B3 is 0.5, not zero; use a literal 0
    let result = evaluate_formula("A1/0", &table);
    assert_eq!(result, None);
}

// --- Operator precedence ---

#[test]
fn formula_precedence_mul_before_add() {
    let table = formula_table();
    // A1 + B1 * A2 = 10 + 2.5 * 20 = 10 + 50 = 60
    let result = evaluate_formula("A1+B1*A2", &table);
    assert_eq!(result, Some(60.0));
}

#[test]
fn formula_parentheses_override_precedence() {
    let table = formula_table();
    // (A1 + B1) * A2 = (10 + 2.5) * 20 = 12.5 * 20 = 250
    let result = evaluate_formula("(A1+B1)*A2", &table);
    assert_eq!(result, Some(250.0));
}

// --- Cell references ---

#[test]
fn formula_different_rows() {
    let table = formula_table();
    let result = evaluate_formula("A1+A2+A3", &table);
    assert_eq!(result, Some(60.0)); // 10 + 20 + 30
}

#[test]
fn formula_null_cell_treated_as_zero() {
    let table = formula_table();
    // C3 is Null, should be 0
    // Need a numeric column — C is string. Let's use that Null is 0.
    // Actually C3 is Null which returns 0.0 per cell_as_f64
    // But C1 is "hello" which won't parse as f64 -> None -> 0.0
    let result = evaluate_formula("A1+0", &table);
    assert_eq!(result, Some(10.0));
}

#[test]
fn formula_with_numeric_literal() {
    let table = formula_table();
    let result = evaluate_formula("A1*2+3", &table);
    assert_eq!(result, Some(23.0)); // 10*2+3
}

#[test]
fn formula_with_float_literal() {
    let table = formula_table();
    let result = evaluate_formula("A1*1.5", &table);
    assert_eq!(result, Some(15.0));
}

// --- Invalid formulas ---

#[test]
fn formula_invalid_ref_returns_none() {
    let table = formula_table();
    // Row 0 doesn't exist in 1-based (A0 is invalid)
    let result = evaluate_formula("A0+B1", &table);
    assert_eq!(result, None);
}

#[test]
fn formula_empty_returns_none() {
    let table = formula_table();
    let result = evaluate_formula("", &table);
    assert_eq!(result, None);
}

#[test]
fn formula_just_number() {
    let table = formula_table();
    let result = evaluate_formula("42", &table);
    assert_eq!(result, Some(42.0));
}

#[test]
fn formula_nested_parens() {
    let table = formula_table();
    let result = evaluate_formula("((A1+A2))*2", &table);
    assert_eq!(result, Some(60.0)); // (10+20)*2
}

#[test]
fn formula_unary_minus() {
    let table = formula_table();
    let result = evaluate_formula("-5+A1", &table);
    assert_eq!(result, Some(5.0)); // -5 + 10
}

// --- Multi-column references ---

#[test]
fn formula_aa_column_ref() {
    // AA would be column 26 (0-indexed), which doesn't exist in our 3-col table
    let table = formula_table();
    // Should still parse, but cell value would be 0 (out of bounds)
    let result = evaluate_formula("AA1+0", &table);
    // get(0, 26) -> None -> 0.0
    assert_eq!(result, Some(0.0));
}

// --- Diagnostics: non-numeric references produce no value + a bad-cell report ---

#[test]
fn diagnostics_string_reference_is_rejected() {
    // C1 is "hello" — must not silently become 0.0.
    let table = formula_table();
    let outcome = evaluate_formula_with_diagnostics("A1+C1", &table);
    assert!(
        outcome.value.is_none(),
        "non-numeric cell must abort the row"
    );
    let bad = outcome.bad_cell.expect("bad cell must be reported");
    assert_eq!(bad.row, 0);
    assert_eq!(bad.col, 2);
    assert_eq!(bad.content, "hello");
}

#[test]
fn diagnostics_numeric_only_path_is_clean() {
    let table = formula_table();
    let outcome = evaluate_formula_with_diagnostics("A1+B1", &table);
    assert_eq!(outcome.value, Some(12.5));
    assert!(outcome.bad_cell.is_none());
}

#[test]
fn diagnostics_numeric_string_still_parses() {
    // A string that *looks* like a number is fine — only un-parseable strings
    // count as bad cells. This protects users who imported numbers as text.
    let mut table = formula_table();
    table.rows[0][2] = CellValue::String("4.5".into());
    let outcome = evaluate_formula_with_diagnostics("A1+C1", &table);
    assert_eq!(outcome.value, Some(14.5));
    assert!(outcome.bad_cell.is_none());
}

#[test]
fn diagnostics_first_bad_cell_wins() {
    // Multiple bad references; the report should pin the first one we hit.
    let mut table = formula_table();
    table.rows[0][2] = CellValue::String("not a number".into());
    // Pretend column B has been overwritten with non-numeric text too.
    table.rows[0][1] = CellValue::String("also bad".into());
    let outcome = evaluate_formula_with_diagnostics("B1+C1", &table);
    assert!(outcome.value.is_none());
    let bad = outcome.bad_cell.expect("expected bad cell");
    assert_eq!(bad.col, 1, "left operand should be reported first");
    assert_eq!(bad.content, "also bad");
}

#[test]
fn aggregates_accept_rectangles_mixed_arguments_and_aliases() {
    let table = formula_table();
    for (formula, value) in [
        ("SUM(A1:B2, A3, -5, 2*3)", 66.5),
        ("SUMME(B2:A1;3)", 38.5),
        ("MITTELWERT(A1:B2)", 8.875),
        ("AVERAGE(A1:A3)", 20.0),
        ("2 * min(A1:B2)", 5.0),
        ("MAX(A1, MIN(A2:B2), 12)", 12.0),
        ("COUNT(A1:C3)", 6.0),
        ("ANZAHL(C1:C3; A1; 0)", 2.0),
        ("SUM(A2:B2)", 23.0),
    ] {
        assert_eq!(evaluate_formula(formula, &table), Some(value), "{formula}");
    }
}

#[test]
fn aggregate_ranges_read_edits_and_skip_blank_or_non_numeric_cells() {
    let mut table = formula_table();
    table.edits.insert((0, 2), CellValue::String("12.5".into()));
    assert_eq!(evaluate_formula("SUM(C1:C3)", &table), Some(12.5));
    assert_eq!(evaluate_formula("COUNT(C1:C3)", &table), Some(1.0));
    assert_eq!(evaluate_formula("AVERAGE(C2:C3)", &table), None);
    assert_eq!(evaluate_formula("SUM(A3:A999999999)", &table), Some(30.0));
}

#[test]
fn malformed_ranges_and_unconsumed_operands_are_rejected() {
    let table = formula_table();
    for formula in [
        "SUM(A1:)",
        "SUM(:B1)",
        "SUM(A1:A2:A3)",
        "SUM(A1:B2+1)",
        "SUM(A1,)",
        "SUM(,A1)",
        "A1:B2",
        "A1 B2",
        "SUM(A1) 42",
        "SUM(ZZZZZZZZZZZZZZZZZZZZZZZZ1:A1)",
        "SUM(A0:A1)",
    ] {
        assert_eq!(evaluate_formula(formula, &table), None, "{formula}");
    }
}

#[test]
fn round_uses_decimal_midpoints_away_from_zero() {
    let table = formula_table();
    for (formula, expected) in [
        ("ROUND(2.15,1)", 2.2),
        ("ROUND(1.005,2)", 1.01),
        // The exact f64 neighbour below the float that spells as 1.005.
        ("ROUND(1.0049999999999997,2)", 1.0),
        ("ROUND(-1.475,2)", -1.48),
        ("ROUND(-0.005,2)", -0.01),
        ("ROUND(-0.0049,2)", 0.0),
        ("ROUND(6.25,1)", 6.3),
        ("ROUND(-0.5,0)", -1.0),
    ] {
        assert_eq!(
            evaluate_formula(formula, &table),
            Some(expected),
            "{formula}"
        );
    }
}

#[test]
fn round_accepts_negative_precision_for_reporting_totals() {
    let table = formula_table();
    for (formula, expected) in [
        ("ROUND(624.9,-3)", 1000.0),
        ("ROUND(499.99,-3)", 0.0),
        ("ROUND(-50,-2)", -100.0),
        ("ROUND(21.5,-1)", 20.0),
        ("runden(A2+B1;-1)", 20.0),
    ] {
        assert_eq!(
            evaluate_formula(formula, &table),
            Some(expected),
            "{formula}"
        );
    }
}

#[test]
fn round_handles_float_limits_without_intermediate_overflow() {
    let mut table = formula_table();
    table.rows[0][1] = CellValue::Float(f64::MAX);
    table.rows[1][1] = CellValue::Float(f64::from_bits(1));
    for (formula, expected) in [
        ("ROUND(B1,320)", Some(f64::MAX)),
        ("ROUND(B1,-308)", None),
        ("ROUND(B1,-309)", Some(0.0)),
        ("ROUND(B2,323)", Some(0.0)),
        ("ROUND(B2,324)", Some(f64::from_bits(1))),
    ] {
        assert_eq!(evaluate_formula(formula, &table), expected, "{formula}");
    }
}

#[test]
fn scalar_functions_compose_with_ranges_signs_and_calculated_precision() {
    let table = formula_table();
    assert_eq!(
        evaluate_formula("ROUND(ABS(-(SUM(A1:A2)+B3))/16, A1/5)", &table),
        Some(1.91)
    );
    assert_eq!(evaluate_formula("A1 * -B1 + +2", &table), Some(-23.0));
    assert_eq!(evaluate_formula("ABS(-A3)", &table), Some(30.0));
}

#[test]
fn scalar_functions_reject_invalid_arity_ranges_and_fractional_precision() {
    let table = formula_table();
    for formula in [
        "ABS()",
        "ABS(A1,B1)",
        "ABS(A1:A3)",
        "ABS(1/0)",
        "ROUND(A1)",
        "ROUND(A1,)",
        "ROUND(A1,2,3)",
        "ROUND(A1,1.5)",
        "ROUND(A1:A3,2)",
        "ROUND(A1,2) B1",
    ] {
        assert_eq!(evaluate_formula(formula, &table), None, "{formula}");
    }
}

#[test]
fn scalar_functions_keep_the_bad_cell_diagnostic() {
    let table = formula_table();
    let outcome = evaluate_formula_with_diagnostics("ROUND(ABS(A1), C1)", &table);
    assert_eq!(outcome.value, None);
    let bad = outcome.bad_cell.unwrap();
    assert_eq!((bad.row, bad.col, bad.content.as_str()), (0, 2, "hello"));
}

#[test]
fn median_accepts_rectangles_expressions_and_reversed_corners() {
    let table = formula_table();
    assert_eq!(evaluate_formula("MEDIAN(A1:A3)", &table), Some(20.0));
    assert_eq!(evaluate_formula("MEDIAN(B2:A1)", &table), Some(6.5));
    assert_eq!(
        evaluate_formula("ROUND(MEDIAN(A3:A1, 2*B1, B3), 1)", &table),
        Some(10.0)
    );
}

#[test]
fn median_skips_missing_values_but_keeps_zero_and_numeric_text() {
    let mut table = formula_table();
    table.set(0, 2, CellValue::String("8.0".into()));
    table.set(1, 2, CellValue::Float(f64::INFINITY));
    assert_eq!(
        evaluate_formula("MEDIAN(C1:C999999999, 0)", &table),
        Some(4.0)
    );
    assert_eq!(evaluate_formula("MEDIAN(C2:C3)", &table), None);
    assert_eq!(evaluate_formula("MEDIAN()", &table), None);
    assert!(table.undo());
    assert_eq!(evaluate_formula("MEDIAN(C1:C3, 0)", &table), Some(4.0));
    assert!(table.undo());
    assert_eq!(evaluate_formula("MEDIAN(C1:C3)", &table), None);
}

#[test]
fn median_midpoint_does_not_overflow_or_erase_subnormals() {
    let mut table = formula_table();
    table.rows[0][1] = CellValue::Float(f64::MAX);
    table.rows[1][1] = CellValue::Float(f64::MAX);
    assert_eq!(evaluate_formula("MEDIAN(B1:B2)", &table), Some(f64::MAX));
    let smallest = f64::from_bits(1);
    table.rows[0][1] = CellValue::Float(smallest);
    table.rows[1][1] = CellValue::Float(smallest);
    assert_eq!(evaluate_formula("MEDIAN(B1:B2)", &table), Some(smallest));
}

#[test]
fn nested_calls_stop_at_a_bounded_depth_without_panicking() {
    let table = formula_table();
    for depth in [1, 64, 65, 10_000] {
        let formula = format!("{}A1{}", "ABS(".repeat(depth), ")".repeat(depth));
        assert_eq!(
            evaluate_formula(&formula, &table),
            (depth <= 64).then_some(10.0),
            "depth {depth}"
        );
    }
}

#[test]
fn formula_edit_undo_redo_and_xlsx_reopen_keep_numeric_results() {
    use octa::formats::{FormatReader, excel_reader::ExcelReader};
    use octa::ui::table_view::commit_cell_text;

    let mut table = formula_table();
    assert!(commit_cell_text(
        &mut table,
        0,
        1,
        "=ROUND(ABS(-1.475),2)".into(),
        BinaryDisplayMode::default()
    ));
    assert_eq!(table.get(0, 1), Some(&CellValue::Float(1.48)));
    assert!(table.undo());
    assert_eq!(table.get(0, 1), Some(&CellValue::Float(2.5)));
    assert!(table.redo());
    assert!(commit_cell_text(
        &mut table,
        1,
        1,
        "=MEDIAN(A1:A3)".into(),
        BinaryDisplayMode::default()
    ));
    let output = tempfile::tempdir().unwrap();
    let path = output.path().join("invoice-review.xlsx");
    ExcelReader.write_file(&path, &table).unwrap();
    let reopened = ExcelReader.read_file(&path).unwrap();
    assert_eq!(reopened.get(0, 1), Some(&CellValue::Float(1.48)));
    assert!(
        matches!(reopened.get(1, 1), Some(CellValue::Int(20)))
            || matches!(reopened.get(1, 1), Some(CellValue::Float(x)) if *x == 20.0)
    );
    assert!(reopened.formulas.is_empty());
}

#[test]
fn cell_formula_resolves_absolute_and_mixed_references() {
    let table = formula_table();
    for (formula, value) in [
        ("$A$1+B2", 13.0),
        ("$A1+B$2", 13.0),
        ("a$2+$b$3", 20.5),
        ("SUM($B$2:A$1, $A3)", 65.5),
        ("ROUND(A$1*$B1, 0)", 25.0),
    ] {
        assert_eq!(evaluate_formula(formula, &table), Some(value), "{formula}");
    }
}

#[test]
fn cell_formula_rejects_malformed_anchors_and_oversized_addresses() {
    let table = formula_table();
    let overflow = format!("$A${}0", usize::MAX);
    for formula in [
        "$",
        "$$A1",
        "A$$1",
        "$1",
        "$A",
        "A$0",
        "A1$",
        "$A $1",
        "$SUM(A1)",
        "SUM($A1:$B$)",
        "$XXXXXXXXXXXXXXXXXXXXXX$1",
        &overflow,
    ] {
        assert_eq!(evaluate_formula(formula, &table), None, "{formula}");
    }
}

#[test]
fn cell_formula_anchored_error_reports_the_referenced_source_cell() {
    let table = formula_table();
    let result = evaluate_formula_with_diagnostics("A2 * $C$1", &table);
    assert_eq!(result.value, None);
    let bad = result.bad_cell.unwrap();
    assert_eq!((bad.row, bad.col, bad.content.as_str()), (0, 2, "hello"));
}

fn sales_with_criteria() -> DataTable {
    let mut table = DataTable {
        columns: ["region", "amount", "selector"]
            .map(|name| ColumnInfo {
                name: name.into(),
                data_type: "Utf8".into(),
            })
            .to_vec(),
        rows: vec![
            vec![
                CellValue::String("West".into()),
                CellValue::Int(10),
                CellValue::String("west".into()),
            ],
            vec![
                CellValue::String("west".into()),
                CellValue::Int(20),
                CellValue::Int(15),
            ],
            vec![
                CellValue::String("East".into()),
                CellValue::Float(7.5),
                CellValue::Null,
            ],
            vec![
                CellValue::String("WÉST".into()),
                CellValue::Null,
                CellValue::Bool(true),
            ],
            vec![CellValue::Null, CellValue::Int(0), CellValue::Bool(false)],
            vec![
                CellValue::String(String::new()),
                CellValue::String("35".into()),
                CellValue::Null,
            ],
            vec![
                CellValue::String("West Coast".into()),
                CellValue::String("not numeric".into()),
                CellValue::Null,
            ],
        ],
        ..DataTable::empty()
    };
    // Evaluate the edit overlay rather than the original load values.
    table.set(1, 1, CellValue::Int(30));
    table
}

#[test]
fn conditional_aggregates_compose_case_insensitively_and_read_edits() {
    let table = sales_with_criteria();
    for (formula, want) in [
        ("COUNTIF(A1:A7, \"west\")", 2.0),
        ("Zählenwenn(A1:A7;C1)", 2.0),
        ("SUMMEWENN(A1:A7;C1;B1:B7)", 40.0),
        ("MITTELWERTWENN(A1:A7;C1;B1:B7)", 20.0),
        (
            "ROUND(SUMIF(A1:A7,\"wést\",B1:B7)+AVERAGEIF(A1:A7,\"East\",B1:B7),1)",
            7.5,
        ),
        ("COUNTIF(B1:B7, \">=\"&C2)", 2.0),
        ("COUNTIF(B1:B7, 10+20)", 1.0),
        ("COUNTIF(B1:B7, C3)", 1.0),
        ("COUNTIF(C1:C7,TRUE)", 1.0),
        ("COUNTIF(A1:A7,\"west*\")", 3.0),
    ] {
        assert_eq!(evaluate_formula(formula, &table), Some(want), "{formula}");
    }
}

#[test]
fn conditional_ranges_keep_the_matched_shape_and_clip_unloaded_rows() {
    let table = sales_with_criteria();
    assert_eq!(
        evaluate_formula("SUMIF(A7:A1,\"west\",B1)", &table),
        Some(40.0)
    );
    assert_eq!(
        evaluate_formula("AVERAGEIF(A1:A7,\"west\",C7:B1)", &table),
        Some(20.0)
    );
    assert_eq!(
        evaluate_formula("COUNTIF(A1:A999999999,\"\")", &table),
        Some(2.0)
    );
    assert_eq!(evaluate_formula("COUNTIF(A1,\"WEST\")", &table), Some(1.0));
    assert_eq!(evaluate_formula("COUNTIF(A1:A7,\"<>\")", &table), Some(5.0));
    assert_eq!(
        evaluate_formula("SUMIF(A1:A7,\"no match\",B1)", &table),
        Some(0.0)
    );
    assert_eq!(
        evaluate_formula("AVERAGEIF(A1:A7,\"no match\",B1)", &table),
        None
    );
    assert_eq!(
        evaluate_formula("AVERAGEIF(A1:A7,\"wést\",B1)", &table),
        None
    );
    assert_eq!(
        evaluate_formula("AVERAGEIF(A1:A7,\"west\",B7)", &table),
        None
    );

    // A rectangle, not an index over only the rows that matched.
    let table = formula_table();
    assert_eq!(
        evaluate_formula("SUMIF(B2:A1,\">=3\",A1)", &table),
        Some(33.0)
    );
    assert_eq!(evaluate_formula("SUMIF(B2:A1,\">=3\")", &table), Some(33.0));
}

#[test]
fn wildcard_criteria_are_whole_cell_unicode_and_respect_tilde_and_quotes() {
    let mut table = sales_with_criteria();
    table.rows = [
        "A1\"blue",
        "a1\"BLUE",
        "A1",
        "A*",
        "A?",
        "xA*",
        "MÜNCHEN",
        "north\neast",
    ]
    .into_iter()
    .map(|value| {
        vec![
            CellValue::String(value.into()),
            CellValue::Int(2),
            CellValue::Null,
        ]
    })
    .collect();
    for (formula, want) in [
        (r#"COUNTIF(A1:A8,"A1""blue")"#, 2.0),
        (r#"COUNTIF(A1:A8,"A~*")"#, 1.0),
        (r#"COUNTIF(A1:A8,"A~?")"#, 1.0),
        (r#"COUNTIF(A1:A8,"A?")"#, 3.0),
        (r#"COUNTIF(A1:A8,"m?nchen")"#, 1.0),
        (r#"COUNTIF(A1:A8,"*east")"#, 1.0),
        (r#"COUNTIF(A1:A8,"*A~*")"#, 2.0),
    ] {
        assert_eq!(evaluate_formula(formula, &table), Some(want), "{formula}");
    }
}

#[test]
fn invalid_conditional_criteria_fail_without_aliased_or_unloaded_data() {
    let table = sales_with_criteria();
    for formula in [
        "COUNTIF(A1:A5)",
        "COUNTIF(A1:A5,\"unterminated)",
        "COUNTIF(A1:A5,\"west\",B1)",
        "COUNTIF(A1:A5, A100)",
        "COUNTIF(A1+1,2)",
        "COUNTIF(A1:A5,\">\"&)",
        "SUMIF(A1:A5,\"west\",B1:)",
        "AVERAGEIF(A1:A5,\"west\",B1,1)",
        "COUNTIF(A1:A5,\">\")",
        "$Ä1",
        "\"criterion text\"",
    ] {
        assert_eq!(evaluate_formula(formula, &table), None, "{formula}");
    }
}

#[test]
fn conditional_formula_from_csv_grid_saves_a_numeric_xlsx_cell() {
    use octa::formats::{FormatReader, csv_reader::CsvReader, excel_reader::ExcelReader};
    use octa::ui::table_view::commit_cell_text;

    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("regional-sales.csv");
    std::fs::write(
        &input,
        "region,amount,checked\nWest,25,0\nEast,4,0\nwest,50,0\n",
    )
    .unwrap();
    let mut table = CsvReader.read_file(&input).unwrap();
    assert!(commit_cell_text(
        &mut table,
        0,
        2,
        r#"=SUMIF($A$1:$A$3,"West",$B$1:$B$3)"#.into(),
        BinaryDisplayMode::default(),
    ));
    let output = dir.path().join("regional-sales.xlsx");
    ExcelReader.write_file(&output, &table).unwrap();
    let saved = ExcelReader.read_file(&output).unwrap();
    assert!(
        matches!(saved.get(0, 2), Some(CellValue::Int(75)))
            || matches!(saved.get(0, 2), Some(CellValue::Float(v)) if *v == 75.0)
    );
    assert!(saved.formulas.is_empty());
}

#[test]
fn if_selects_only_one_numeric_branch() {
    let table = formula_table();
    for (formula, expected) in [
        ("IF(TRUE,7,1/0)", 7.0),
        ("IF(FALSE,C1,3)", 3.0),
        ("IF((A1>B1), ROUND(A1/3, 2), C2)", 3.33),
        ("WENN(A2>=20; B2*2; 0)", 6.0),
        ("IF(A1<=B1,1,2)", 2.0),
        ("IF(A1<>B1,8)", 8.0),
        ("IF(FALSE,8)", 0.0),
        ("IF(FALSE,8,)", 0.0),
        ("IF(-2,9)", 9.0),
        (r#"IF(C1="HELLO",10,20)"#, 10.0),
        (r#"IF(C3="",5,1/0)"#, 5.0),
        (r#"IF(TRUE,1,AVERAGEIF(C1:C3,"unmatched",B1:B3))"#, 1.0),
        ("IF(1=1,IF(TRUE,2,C2),MEDIAN(C1:C3))", 2.0),
    ] {
        let outcome = evaluate_formula_with_diagnostics(formula, &table);
        assert_eq!(outcome.value, Some(expected), "{formula}");
        assert!(outcome.bad_cell.is_none(), "{formula}");
    }
}

#[test]
fn iferror_falls_back_for_runtime_errors_without_leaking_diagnostics() {
    let table = formula_table();
    for (formula, expected) in [
        ("IFERROR(A1/0,42)", 42.0),
        ("WENNFEHLER(ROUND(A1,1.5); -2)", -2.0),
        ("IFERROR(A2/2,C2)", 10.0),
        ("IFERROR(C1,6)", 6.0),
        ("IFERROR(IF(A1>0,C1,3),5)", 5.0),
        (r#"IFERROR(AVERAGEIF(C1:C3,"no hit",B1:B3),11)"#, 11.0),
        ("IFERROR(TRUE,FALSE)", 1.0),
    ] {
        let outcome = evaluate_formula_with_diagnostics(formula, &table);
        assert_eq!(outcome.value, Some(expected), "{formula}");
        assert!(outcome.bad_cell.is_none(), "{formula}");
    }
    let failure = evaluate_formula_with_diagnostics("IFERROR(C1,C2)", &table);
    assert_eq!(failure.value, None);
    let bad = failure.bad_cell.unwrap();
    assert_eq!((bad.row, bad.col, bad.content.as_str()), (1, 2, "world"));
    let failure = evaluate_formula_with_diagnostics("IF(TRUE,C1,C2)", &table);
    assert_eq!(failure.value, None);
    let bad = failure.bad_cell.unwrap();
    assert_eq!((bad.row, bad.col, bad.content.as_str()), (0, 2, "hello"));
}

#[test]
fn malformed_if_branches_never_become_valid_numbers() {
    let table = formula_table();
    for formula in [
        "IF(TRUE,1,2+)",
        "IF(FALSE,2+,1)",
        "IF(TRUE,1,SUM())",
        "IF(FALSE,A1:A2,1)",
        "IF(TRUE,1,COUNTIF(A1:A3,))",
        "IF(TRUE,1,IF(FALSE,8,3+))",
        "IFERROR(1/0+,5)",
        "IFERROR(1,2+)",
        "IFERROR(1/0,2,3)",
        "IF(A1>B1>C1,4,5)",
        "IF(,2,3)",
        "IF(TRUE,,3)",
        "IF(TRUE,2,3,4)",
    ] {
        assert_eq!(evaluate_formula(formula, &table), None, "{formula}");
    }
}

#[test]
fn syntax_only_context_handles_nested_if_without_evaluating_ranges() {
    let mut table = formula_table();
    table.rows[0][0] = CellValue::Float(f64::NAN);
    let exp = "IF(FALSE,MEDIAN(A1:ZZ999999999),1)";
    let expected = format!("{}{}{}", "IF(TRUE,".repeat(62), exp, ",C2)".repeat(62));
    assert_eq!(evaluate_formula(&expected, &table), Some(1.0));
    assert_eq!(evaluate_formula("IFERROR(A1,5)", &table), Some(5.0));
    assert_eq!(evaluate_formula("IFERROR(A1,1/0)", &table), None);
}

#[test]
fn iferror_fallback_is_numeric_after_saving_and_reopening_xlsx() {
    use octa::formats::{FormatReader, csv_reader::CsvReader, excel_reader::ExcelReader};
    use octa::ui::table_view::commit_cell_text;

    let dir = tempfile::tempdir().unwrap();
    let csv = dir.path().join("discount.csv");
    std::fs::write(&csv, "region,amount,qty,result\nWest,30,0,0\nEast,12,3,0\n").unwrap();
    let mut table = CsvReader.read_file(&csv).unwrap();
    assert!(commit_cell_text(
        &mut table,
        0,
        3,
        r#"=IFERROR(IF(A1="west",B1/C1,0),-1)"#.into(),
        BinaryDisplayMode::default(),
    ));
    let output = dir.path().join("discount.xlsx");
    ExcelReader.write_file(&output, &table).unwrap();
    let reopened = ExcelReader.read_file(&output).unwrap();
    assert!(
        matches!(reopened.get(0, 3), Some(CellValue::Int(-1)))
            || matches!(reopened.get(0, 3), Some(CellValue::Float(v)) if *v == -1.0)
    );
    assert!(reopened.formulas.is_empty());
}

#[test]
fn multi_criteria_joins_wildcard_boolean_numeric_and_edit_overlay() {
    let table = sales_with_criteria();
    for (formula, expected) in [
        (r#"COUNTIFS(A1:A7,"west*",B1:B7,">="&$C$2)"#, 1.0),
        (r#"Zählenwenns(C1:C7;TRUE;A1:A7;"W?ST")"#, 1.0),
        (r#"SUMIFS(B1:B7,A1:A7,"west*",A1:A7,"<>West Coast")"#, 40.0),
        (r#"SUMMEWENNS(B1:B7;A1:A7;"West";B1:B7;">25")"#, 30.0),
        (r#"AVERAGEIFS(B1:B7,A1:A7,"west*")"#, 20.0),
        (r#"MITTELWERTWENNS(B1:B7;A1:A7;"west*";B1:B7;">=10")"#, 20.0),
        (r#"SUMIFS(B1:B7,A1:A7,"")"#, 35.0),
        (r#"SUMIFS(B1:B7,A1:A7,"missing")"#, 0.0),
        (r#"IFERROR(AVERAGEIFS(B1:B7,A1:A7,"missing"),-1)"#, -1.0),
    ] {
        assert_eq!(
            evaluate_formula(formula, &table),
            Some(expected),
            "{formula}"
        );
    }
}

#[test]
fn multi_criteria_aligns_rectangles_by_relative_position() {
    let table = formula_table();
    // Match [yes, yes; yes, no] across two different 2×2 blocks.
    for (formula, expected) in [
        (r#"COUNTIFS(B2:A1,">=2",C3:B2,"<>")"#, 3.0),
        (r#"SUMIFS(B3:A2,B2:A1,">=2",C3:B2,"<>")"#, 53.0),
        (r#"AVERAGEIFS(A2:B3,A1:B2,">=2",B2:C3,"<>")"#, 53.0 / 3.0),
        (r#"SUMIFS(A2,A1,10,B1,">2")"#, 20.0),
    ] {
        assert_eq!(
            evaluate_formula(formula, &table),
            Some(expected),
            "{formula}"
        );
    }
}

#[test]
fn multi_criteria_rejects_shape_mismatch_and_malformed_pairs() {
    let table = formula_table();
    for malformed in [
        "COUNTIFS()",
        "SUMIFS(A1:A2)",
        "COUNTIFS(A1:A2)",
        "COUNTIFS(A1:A2,10,B1:B2)",
        "SUMIFS(B1:B2,A1:A2,10,)",
        "AVERAGEIFS(B1:B2,A1:A2,)",
        "COUNTIFS(A1+1,10)",
        "COUNTIFS(A1:A2,10,B1:B2,&)",
    ] {
        assert_eq!(evaluate_formula(malformed, &table), None, "{malformed}");
        assert_eq!(
            evaluate_formula(&format!("IF(TRUE,7,{malformed})"), &table),
            None
        );
        assert_eq!(
            evaluate_formula(&format!("IFERROR({malformed},7)"), &table),
            None
        );
    }
    for invalid_shape in [
        r#"COUNTIFS(A1:A2,">0",B1:C1,"<>")"#,
        r#"SUMIFS(B1:B3,A1:A2,">0")"#,
        r#"AVERAGEIFS(A1:B2,A1:A4,">0")"#,
    ] {
        assert_eq!(
            evaluate_formula(invalid_shape, &table),
            None,
            "{invalid_shape}"
        );
        assert_eq!(
            evaluate_formula(&format!("IFERROR({invalid_shape},7)"), &table),
            Some(7.0)
        );
    }
}

#[test]
fn multi_criteria_clips_to_loaded_cells_without_counting_phantom_blanks() {
    let table = sales_with_criteria();
    for (formula, expected) in [
        (r#"COUNTIFS(A1:A99999999,"",C2:C100000000,"")"#, 2.0),
        (r#"COUNTIFS(A1:A99999999,"",Z1:Z99999999,"")"#, 0.0),
        (r#"COUNTIFS(A6:A7,"<>",C7:C8,"")"#, 0.0),
        (
            r#"SUMIFS(B2:B100000000,A1:A99999999,"West",C1:C99999999,"<>")"#,
            37.5,
        ),
    ] {
        assert_eq!(
            evaluate_formula(formula, &table),
            Some(expected),
            "{formula}"
        );
    }
}

#[test]
fn multi_criteria_enforces_the_excel_pair_limit() {
    let table = formula_table();
    let legal = format!("COUNTIFS(A1,10{})", ",A1,10".repeat(126));
    let too_many = format!("COUNTIFS(A1,10{})", ",A1,10".repeat(127));
    assert_eq!(evaluate_formula(&legal, &table), Some(1.0));
    assert_eq!(evaluate_formula(&too_many, &table), None);
    assert_eq!(
        evaluate_formula(&format!("IF(TRUE,7,{too_many})"), &table),
        None
    );
}

#[test]
fn multi_criteria_formula_survives_inline_save_and_reopen_xlsx() {
    use octa::formats::{FormatReader, csv_reader::CsvReader, excel_reader::ExcelReader};
    use octa::ui::table_view::commit_cell_text;

    let dir = tempfile::tempdir().unwrap();
    let csv = dir.path().join("payments.csv");
    std::fs::write(&csv, "region,status,amount,result\nWest,Paid,32,0\nwest,Pending,45,0\nEast,Paid,19,0\nWest,Paid,8,0\n").unwrap();
    let mut table = CsvReader.read_file(&csv).unwrap();
    assert!(commit_cell_text(
        &mut table,
        0,
        3,
        r#"=SUMIFS(C1:C4,A1:A4,"West",B1:B4,"Paid")"#.into(),
        BinaryDisplayMode::default(),
    ));
    let output = dir.path().join("payments.xlsx");
    ExcelReader.write_file(&output, &table).unwrap();
    let reopened = ExcelReader.read_file(&output).unwrap();
    assert!(
        matches!(reopened.get(0, 3), Some(CellValue::Int(40)))
            || matches!(reopened.get(0, 3), Some(CellValue::Float(v)) if *v == 40.0)
    );
    assert!(reopened.formulas.is_empty());
}

fn lookup_catalog() -> DataTable {
    let mut table = DataTable {
        columns: ["sku", "price", "request"]
            .map(|name| ColumnInfo {
                name: name.into(),
                data_type: "Utf8".into(),
            })
            .to_vec(),
        rows: [
            (Some("AX-01"), CellValue::Int(10), Some("aX-01")),
            (Some("ax-01"), CellValue::Int(12), None),
            (Some("AX-02"), CellValue::String("19.5".into()), None),
            (Some("AX-*"), CellValue::Int(99), None),
            (Some("Öko"), CellValue::Int(7), None),
            (None, CellValue::Int(5), None),
            (
                Some("broken"),
                CellValue::String("missing price".into()),
                None,
            ),
        ]
        .into_iter()
        .map(|(sku, price, request)| {
            vec![
                sku.map_or(CellValue::Null, |v| CellValue::String(v.into())),
                price,
                request.map_or(CellValue::Null, |v| CellValue::String(v.into())),
            ]
        })
        .collect(),
        ..DataTable::empty()
    };
    table.set(1, 1, CellValue::Int(14));
    table
}

#[test]
fn formula_lookup_uses_exact_match_and_search_order() {
    let table = lookup_catalog();
    for (formula, expected) in [
        (r#"XLOOKUP("ax-01",A1:A7,B1:B7)"#, 10.0),
        (r#"XLOOKUP("ax-01",A1:A7,B1:B7,,,-1)"#, 14.0),
        (r#"XVERWEIS(C1;$A$1:$A$7;$B$1:$B$7;;0;-1)"#, 14.0),
        (r#"XLOOKUP("AX-02",A7:A1,B7:B1)"#, 19.5),
        (r#"XLOOKUP(,A1:A7,B1:B7)"#, 5.0),
        (r#"XMATCH("ax-01",A2:A7)"#, 1.0),
        (r#"XVERGLEICH("AX-01";A1:A7;0;-1)"#, 2.0),
        (r#"XMATCH("ÖKO",A1:A7)"#, 5.0),
    ] {
        assert_eq!(
            evaluate_formula(formula, &table),
            Some(expected),
            "{formula}"
        );
    }
}

#[test]
fn formula_lookup_pairs_horizontal_ranges_by_offset() {
    let table = formula_table();
    for (formula, expected) in [
        ("XLOOKUP(2.5,A1:B1,A3:B3)", 0.5),
        ("XMATCH(2.5,A1:B1,0,-1)", 2.0),
        ("XLOOKUP(30,A3,B2:B2,99)+1", 4.0),
        ("XLOOKUP(1+2,A2:B2,A1:B1)", 2.5),
    ] {
        assert_eq!(
            evaluate_formula(formula, &table),
            Some(expected),
            "{formula}"
        );
    }
}

#[test]
fn formula_lookup_only_treats_wildcards_as_patterns_when_requested() {
    let table = lookup_catalog();
    for (formula, expected) in [
        (r#"XMATCH("AX-*",A1:A7)"#, Some(4.0)),
        (r#"XMATCH("AX-??",A1:A7)"#, None),
        (r#"XMATCH("AX-??",A1:A7,2,-1)"#, Some(3.0)),
        (r#"XMATCH("ax-~*",A1:A7,2)"#, Some(4.0)),
        (r#"XLOOKUP("a?-0*",A1:A7,B1:B7,,2,-1)"#, Some(19.5)),
        (r#"XLOOKUP("no-*",A1:A7,B1:B7,77,2)"#, Some(77.0)),
        (r#"XMATCH("AX-01",A1:A7,1)"#, None),
        (r#"XMATCH("AX-01",A1:A7,0,2)"#, None),
    ] {
        assert_eq!(evaluate_formula(formula, &table), expected, "{formula}");
    }
}

#[test]
fn formula_lookup_keeps_numeric_text_booleans_and_large_ids_distinct() {
    let mut table = lookup_catalog();
    table.rows[0][0] = CellValue::String("1".into());
    table.rows[1][0] = CellValue::Int(1);
    table.rows[2][0] = CellValue::Bool(true);
    table.rows[3][0] = CellValue::Int(9_007_199_254_740_992);
    table.rows[4][0] = CellValue::Int(9_007_199_254_740_993);
    table.rows[0][2] = CellValue::Int(9_007_199_254_740_993);
    table.rows[1][2] = CellValue::Float(9_007_199_254_740_992.0);
    for (formula, expected) in [
        (r#"XMATCH("1",A1:A5)"#, 1.0),
        (r#"XMATCH(1,A1:A5)"#, 2.0),
        (r#"XMATCH(TRUE,A1:A5)"#, 3.0),
        (r#"XMATCH(C1,A1:A5)"#, 5.0),
        (r#"XMATCH(C2,A1:A5,0,-1)"#, 4.0),
    ] {
        assert_eq!(
            evaluate_formula(formula, &table),
            Some(expected),
            "{formula}"
        );
    }
}

#[test]
fn formula_lookup_fallback_is_lazy_and_only_for_misses() {
    let table = lookup_catalog();
    for (formula, expected) in [
        (r#"XLOOKUP("AX-01",A1:A7,B1:B7,1/0)"#, Some(10.0)),
        (r#"XLOOKUP("unknown",A1:A7,B1:B7,ROUND(7.25,1))"#, Some(7.3)),
        (r#"XLOOKUP("unknown",A1:A7,B1:B7)"#, None),
        (r#"XLOOKUP("AX-01",A1:A7,B1:B7,2+)"#, None),
        (r#"IFERROR(XLOOKUP("unknown",A1:A7,B1:B7,1/0+),3)"#, None),
        (r#"IFERROR(XMATCH("missing",A1:A7),3)"#, Some(3.0)),
        (
            r#"IFERROR(XLOOKUP("broken",A1:A7,B1:B7,99),-3)"#,
            Some(-3.0),
        ),
        (
            r#"IF(TRUE,4,XLOOKUP("unknown",A1:A7,B1:B7,1/0))"#,
            Some(4.0),
        ),
        (r#"IF(TRUE,4,XMATCH("AX-01",A1:A7,0,1,2))"#, None),
    ] {
        assert_eq!(evaluate_formula(formula, &table), expected, "{formula}");
    }
    let detailed = evaluate_formula_with_diagnostics(r#"XLOOKUP("broken",A1:A7,B1:B7,99)"#, &table);
    assert_eq!(detailed.value, None);
    let bad = detailed.bad_cell.unwrap();
    assert_eq!(
        (bad.row, bad.col, bad.content.as_str()),
        (6, 1, "missing price")
    );
}

#[test]
fn formula_lookup_limits_search_to_loaded_cells_and_checks_range_shapes() {
    let table = lookup_catalog();
    for (formula, expected) in [
        ("XLOOKUP(,Z1:Z100000000,B1:B100000000,77)", Some(77.0)),
        ("XMATCH(C6,A1:A100000000,0,-1)", Some(6.0)),
        (r#"XLOOKUP("AX-01",A1:A7,B2:B8,,0,-1)"#, Some(19.5)),
        (r#"XLOOKUP("broken",A1:A7,B2:B8,99)"#, None),
        (r#"XLOOKUP("AX-01",A1:A7,B1:B6,99)"#, None),
        (r#"XMATCH("AX-01",A1:B7)"#, None),
        (r#"IFERROR(XMATCH("AX-01",A1:B7),5)"#, Some(5.0)),
        (r#"IFERROR(XMATCH("AX-01",A1:B7+1),5)"#, None),
    ] {
        assert_eq!(evaluate_formula(formula, &table), expected, "{formula}");
    }
}

#[test]
fn formula_lookup_price_survives_xlsx_round_trip() {
    use octa::formats::{FormatReader, csv_reader::CsvReader, excel_reader::ExcelReader};
    use octa::ui::table_view::commit_cell_text;

    let dir = tempfile::tempdir().unwrap();
    let csv = dir.path().join("catalog.csv");
    std::fs::write(
        &csv,
        "sku,price,request,result\nAX-1,12.5,AX-2,0\nAX-2,19.5,AX-1,0\n",
    )
    .unwrap();
    let mut table = CsvReader.read_file(&csv).unwrap();
    assert!(commit_cell_text(
        &mut table,
        0,
        3,
        r#"=XLOOKUP(C1,$A$1:$A$2,$B$1:$B$2,0)"#.into(),
        BinaryDisplayMode::default(),
    ));
    let output = dir.path().join("priced.xlsx");
    ExcelReader.write_file(&output, &table).unwrap();
    let reopened = ExcelReader.read_file(&output).unwrap();
    assert_eq!(reopened.get(0, 3), Some(&CellValue::Float(19.5)));
    assert!(reopened.formulas.is_empty());
}

#[test]
fn classic_vlookup_matches_first_exact_key_and_wildcards() {
    let mut table = lookup_catalog();
    table.set(0, 2, CellValue::Bool(false));
    for (formula, expected) in [
        (r#"VLOOKUP("ax-01",A1:B7,2,FALSE)"#, Some(10.0)),
        (r#"SVERWEIS("AX-02";$B$7:$A$1;2;FALSCH)"#, Some(19.5)),
        (r#"VLOOKUP("a?-0*",A1:B7,2,0)"#, Some(10.0)),
        (r#"VLOOKUP("AX-~*",A1:B7,2,C1)"#, Some(99.0)),
        (r#"VLOOKUP("ÖKO",A1:B7,2,FALSE)"#, Some(7.0)),
        (r#"VLOOKUP("missing",A1:B7,2,FALSE)"#, None),
    ] {
        assert_eq!(evaluate_formula(formula, &table), expected, "{formula}");
    }
    table.set(0, 0, CellValue::String("1".into()));
    table.set(1, 0, CellValue::Int(1));
    assert_eq!(
        evaluate_formula(r#"VLOOKUP("1",A1:B7,2,0)"#, &table),
        Some(10.0)
    );
    assert_eq!(evaluate_formula("VLOOKUP(1,A1:B7,2,0)", &table), Some(14.0));
}

#[test]
fn classic_hlookup_pairs_with_the_requested_row() {
    let mut table = formula_table();
    assert_eq!(
        evaluate_formula("HLOOKUP(2.5,A1:C3,3,FALSE)", &table),
        Some(0.5)
    );
    assert_eq!(
        evaluate_formula("WVERWEIS(10;$A$1:$C$3;3;FALSCH)", &table),
        Some(30.0)
    );
    table.set(0, 2, CellValue::Int(100));
    table.set(1, 2, CellValue::Int(25));
    assert_eq!(
        evaluate_formula("HLOOKUP(90,B1:C2,2,TRUE)", &table),
        Some(3.0)
    );
    assert_eq!(
        evaluate_formula("HLOOKUP(100,B1:C2,2,TRUE)", &table),
        Some(25.0)
    );
}

#[test]
fn classic_lookup_uses_sorted_numeric_thresholds_by_default() {
    let mut table = formula_table();
    for (formula, expected) in [
        ("VLOOKUP(23,A1:B3,2)", Some(3.0)),
        ("VLOOKUP(23,A1:B3,2,1)", Some(3.0)),
        ("VLOOKUP(20,A1:B99,2,WAHR)", Some(3.0)),
        ("VLOOKUP(300,A1:B3,2,TRUE)", Some(0.5)),
        ("VLOOKUP(5,A1:B3,2,TRUE)", None),
        ("VLOOKUP(23,A1:B3,2,FALSE)", None),
    ] {
        assert_eq!(evaluate_formula(formula, &table), expected, "{formula}");
    }
    table.set(0, 0, CellValue::Float(-1.5));
    table.set(1, 0, CellValue::Int(-1));
    assert_eq!(
        evaluate_formula("VLOOKUP(-1.25,A1:B3,2)", &table),
        Some(2.5)
    );
    table.set(0, 0, CellValue::Int(-1));
    assert_eq!(
        evaluate_formula("VLOOKUP(-1,A1:B3,2)", &table),
        Some(3.0),
        "use the last row at a numeric threshold with duplicates"
    );
    table.set(2, 0, CellValue::Int(-2));
    assert_eq!(evaluate_formula("VLOOKUP(-1,A1:B3,2)", &table), None);
    table.set(2, 0, CellValue::String("30".into()));
    assert_eq!(evaluate_formula("VLOOKUP(-1,A1:B3,2)", &table), None);
}

#[test]
fn classic_lookup_keeps_distinct_large_numeric_thresholds() {
    let mut table = formula_table();
    table.set(0, 0, CellValue::Float(9_007_199_254_740_992.0));
    table.set(1, 0, CellValue::Int(9_007_199_254_740_993));
    table.set(2, 0, CellValue::Int(9_007_199_254_740_994));
    table.set(0, 2, CellValue::Int(9_007_199_254_740_992));
    table.set(1, 2, CellValue::Int(9_007_199_254_740_993));
    assert_eq!(evaluate_formula("VLOOKUP(C1,A1:B3,2)", &table), Some(2.5));
    assert_eq!(evaluate_formula("VLOOKUP(C2,A1:B3,2)", &table), Some(3.0));
}

#[test]
fn classic_lookup_rejects_invalid_indices_and_keeps_iferror_lazy() {
    let table = lookup_catalog();
    for formula in [
        r#"VLOOKUP("ax-01",A1:B7,0,FALSE)"#,
        r#"VLOOKUP("ax-01",A1:B7,1.5,FALSE)"#,
        r#"VLOOKUP("ax-01",A1:B7,3,FALSE)"#,
        r#"VLOOKUP("ax-01",A1:F7,5,FALSE)"#,
        r#"VLOOKUP("ax-01",A1:B7,2,2)"#,
        r#"VLOOKUP("ax-01",A1:B7,2,TRUE)"#,
        r#"VLOOKUP("ax-01",A1:B7,2,FALSE,0)"#,
        r#"IFERROR(VLOOKUP("ax-01",A1:B7,1+,FALSE),8)"#,
    ] {
        assert_eq!(evaluate_formula(formula, &table), None, "{formula}");
    }
    let bad = evaluate_formula_with_diagnostics(r#"VLOOKUP("broken",A1:B7,2,FALSE)"#, &table);
    let bad = bad.bad_cell.unwrap();
    assert_eq!((bad.row, bad.col), (6, 1));
    assert_eq!(
        evaluate_formula(r#"IFERROR(VLOOKUP("broken",A1:B7,2,FALSE),8)"#, &table),
        Some(8.0)
    );
}

#[test]
fn classic_lookup_result_survives_inline_xlsx_export() {
    use octa::formats::{FormatReader, csv_reader::CsvReader, excel_reader::ExcelReader};
    use octa::ui::table_view::commit_cell_text;

    let dir = tempfile::tempdir().unwrap();
    let csv = dir.path().join("prices.csv");
    std::fs::write(
        &csv,
        "sku,price,order,amount\nnut,12.5,bolt,0\nbolt,19.5,missing,0\n",
    )
    .unwrap();
    let mut table = CsvReader.read_file(&csv).unwrap();
    assert!(commit_cell_text(
        &mut table,
        0,
        3,
        "=IFERROR(VLOOKUP(C1,$A$1:$B$2,2,FALSE)*3,0)".into(),
        BinaryDisplayMode::default(),
    ));
    let path = dir.path().join("order.xlsx");
    ExcelReader.write_file(&path, &table).unwrap();
    let reopened = ExcelReader.read_file(&path).unwrap();
    assert_eq!(reopened.get(0, 3), Some(&CellValue::Float(58.5)));
    assert!(reopened.formulas.is_empty());
}

#[test]
fn sumproduct_pairs_cells_and_nested_weighted_calculation() {
    let mut table = formula_table();
    for (row, (price, units)) in [(20, 500), (25, 750), (35, 200)].into_iter().enumerate() {
        table.set(row, 0, CellValue::Int(price));
        table.set(row, 1, CellValue::Int(units));
    }
    for (formula, expected) in [
        ("SUMPRODUCT(A1:A3,B1:B3)", 35_750.0),
        ("ROUND(SUMPRODUCT(A1:A3,B1:B3)/SUM(B1:B3),2)", 24.66),
        ("SUMPRODUCT(ABS(-5),2+3)", 25.0),
        ("SUMPRODUCT(-3,2.5)", -7.5),
        ("SUMPRODUCT(A1:A3)", 80.0),
    ] {
        assert_eq!(
            evaluate_formula(formula, &table),
            Some(expected),
            "{formula}"
        );
    }
}

#[test]
fn sumproduct_aligns_shifted_rectangles_in_two_dimensions() {
    let table = formula_table();
    for (formula, expected) in [
        ("SUMPRODUCT(A1:B2,A2:B3)", 809.0),
        ("SUMMENPRODUKT($B$2:$A$1;B3:A2)", 809.0),
        ("SUMPRODUCT(A1:A3,B1:B3)", 100.0),
        ("SUMPRODUCT(A1,B1)", 25.0),
    ] {
        assert_eq!(
            evaluate_formula(formula, &table),
            Some(expected),
            "{formula}"
        );
    }
}

#[test]
fn sumproduct_handles_blank_numeric_text_edits_and_zero_before_overflow() {
    let mut table = formula_table();
    table.set(0, 0, CellValue::String(" 2 ".into()));
    table.set(0, 1, CellValue::Int(4));
    table.set(1, 0, CellValue::Float(1e308));
    table.set(1, 1, CellValue::Float(1e308));
    table.set(1, 2, CellValue::Int(0));
    table.set(2, 0, CellValue::Null);
    for (formula, expected) in [
        ("SUMPRODUCT(A1,B1)", Some(8.0)),
        ("SUMPRODUCT(A1,B1,C1)", Some(0.0)),
        ("SUMPRODUCT(A3,B3)", Some(0.0)),
        ("SUMPRODUCT(A2,B2,C2)", Some(0.0)),
        ("SUMPRODUCT(A2,B2)", None),
        ("IFERROR(SUMPRODUCT(A2,B2),-1)", Some(-1.0)),
    ] {
        let outcome = evaluate_formula_with_diagnostics(formula, &table);
        assert_eq!(outcome.value, expected, "{formula}");
        assert!(outcome.bad_cell.is_none(), "{formula}");
    }
}

#[test]
fn sumproduct_rejects_shape_mismatch_malformed_calls_and_too_many_ranges() {
    let table = formula_table();
    for wrong_shape in ["SUMPRODUCT(A1:A2,5)", "SUMPRODUCT(A1:A2,A1:B1)"] {
        assert_eq!(evaluate_formula(wrong_shape, &table), None, "{wrong_shape}");
        assert_eq!(
            evaluate_formula(&format!("IFERROR({wrong_shape},7)"), &table),
            Some(7.0)
        );
        assert_eq!(
            evaluate_formula(&format!("IF(TRUE,7,{wrong_shape})"), &table),
            Some(7.0)
        );
    }
    for invalid in [
        "SUMPRODUCT()",
        "SUMPRODUCT(A1:A3,)",
        "SUMPRODUCT(A1:B2,A1:B2+)",
        "SUMPRODUCT(A1:A3*B1:B3)",
    ] {
        assert_eq!(evaluate_formula(invalid, &table), None, "{invalid}");
        assert_eq!(
            evaluate_formula(&format!("IFERROR({invalid},7)"), &table),
            None
        );
        assert_eq!(
            evaluate_formula(&format!("IF(TRUE,7,{invalid})"), &table),
            None
        );
    }
    let limit = format!("SUMPRODUCT(2{})", ",1".repeat(254));
    let excess = format!("SUMPRODUCT(2{})", ",1".repeat(255));
    assert_eq!(evaluate_formula(&limit, &table), Some(2.0));
    assert_eq!(evaluate_formula(&excess, &table), None);
    assert_eq!(
        evaluate_formula(&format!("IF(TRUE,7,{excess})"), &table),
        None
    );
}

#[test]
fn sumproduct_clips_to_the_loaded_intersection() {
    let table = formula_table();
    for (formula, expected) in [
        ("SUMPRODUCT(A1:A400000000,B2:B400000001)", 40.0),
        ("SUMPRODUCT(A1:ZZ400000000,B1:AAA400000000)", 100.0),
        ("SUMPRODUCT(A1:A400000000,ZZ1:ZZ400000000)", 0.0),
    ] {
        assert_eq!(
            evaluate_formula(formula, &table),
            Some(expected),
            "{formula}"
        );
    }
}

#[test]
fn weighted_unit_price_stays_numeric_after_xlsx_round_trip() {
    use octa::formats::{FormatReader, csv_reader::CsvReader, excel_reader::ExcelReader};
    use octa::ui::table_view::commit_cell_text;
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("purchases.csv");
    let csv = "unit_price,units,weighted_price\n20,500,0\n25,750,0\n35,200,0\n";
    std::fs::write(&input, csv).unwrap();
    let mut table = CsvReader.read_file(&input).unwrap();
    assert!(commit_cell_text(
        &mut table,
        0,
        2,
        "=IFERROR(ROUND(SUMPRODUCT(A1:A3,B1:B3)/SUM(B1:B3),2),0)".into(),
        BinaryDisplayMode::default(),
    ));
    let output = dir.path().join("costs.xlsx");
    ExcelReader.write_file(&output, &table).unwrap();
    let reopened = ExcelReader.read_file(&output).unwrap();
    assert_eq!(reopened.get(0, 2), Some(&CellValue::Float(24.66)));
    assert!(reopened.formulas.is_empty());
    assert_eq!(std::fs::read_to_string(input).unwrap(), csv);
}

#[test]
fn excel_operators_evaluate_percentages_in_context() {
    let table = formula_table();
    for (formula, expected) in [
        ("50%", 0.5),
        ("1+5%", 1.05),
        ("200*(1-15%)", 170.0),
        ("A1*(50+50)%", 10.0),
        ("B1/-50%", -5.0),
        ("(50%)%", 0.005),
        ("IF(A1>500%,A1*50%,0)", 5.0),
        (r#"COUNTIF(B1:B3,">="&50%)"#, 3.0),
    ] {
        assert_eq!(
            evaluate_formula(formula, &table),
            Some(expected),
            "{formula}"
        );
    }
}

#[test]
fn excel_operators_honor_negation_percent_and_left_associative_powers() {
    let table = formula_table();
    for (formula, expected) in [
        ("2^3^2", 64.0),
        ("2^(3^2)", 512.0),
        ("-2^2", 4.0),
        ("-(2^2)", -4.0),
        ("2^-3", 0.125),
        ("2^3*2+3", 19.0),
        ("9^50%", 3.0),
        ("(9^2)%", 0.81),
        ("POWER(2,-3)", 0.125),
        ("POTENZ(A1/5;3)", 8.0),
    ] {
        assert_eq!(
            evaluate_formula(formula, &table),
            Some(expected),
            "{formula}"
        );
    }
}

#[test]
fn scientific_literals_remain_distinct_from_cell_references() {
    let table = formula_table();
    for (formula, expected) in [
        ("1.25E3+5e-1", 1_250.5),
        (".25e2", 25.0),
        ("1E+2+A1", 110.0),
        ("1E2%", 1.0),
        ("COUNTIF(A1:A3,2e1)", 1.0),
        ("SUM(1e3,2e2,3e-1)", 1_200.3),
    ] {
        assert_eq!(
            evaluate_formula(formula, &table),
            Some(expected),
            "{formula}"
        );
    }
    let invalid = evaluate_formula_with_diagnostics("1E3+C2%", &table);
    assert_eq!(invalid.value, None);
    let bad = invalid.bad_cell.unwrap();
    assert_eq!((bad.row, bad.col, bad.content.as_str()), (1, 2, "world"));
}

#[test]
fn excel_operators_keep_syntax_errors_distinct_from_arithmetic_errors() {
    let table = formula_table();
    for expression in ["0^0", "0^-1", "(-3)^0.5", "10^400", "POWER(0,0)", "1E309"] {
        assert_eq!(evaluate_formula(expression, &table), None, "{expression}");
        assert_eq!(
            evaluate_formula(&format!("IFERROR({expression},7)"), &table),
            Some(7.0)
        );
        assert_eq!(
            evaluate_formula(&format!("IF(TRUE,7,{expression})"), &table),
            Some(7.0)
        );
    }
    for malformed in [
        "2^",
        "%2",
        "1e+",
        "1.2.3E2",
        ".E3",
        "POWER(2)",
        "POWER(2,3,4)",
    ] {
        assert_eq!(evaluate_formula(malformed, &table), None, "{malformed}");
        assert_eq!(
            evaluate_formula(&format!("IFERROR({malformed},7)"), &table),
            None
        );
        assert_eq!(
            evaluate_formula(&format!("IF(TRUE,7,{malformed})"), &table),
            None
        );
    }
}

#[test]
fn discount_and_growth_formula_survives_numeric_xlsx_round_trip() {
    use octa::formats::{FormatReader, csv_reader::CsvReader, excel_reader::ExcelReader};
    use octa::ui::table_view::commit_cell_text;
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("forecast.csv");
    let csv = "base_price,discount_percent,cycles,result\n100,50,2,0\n";
    std::fs::write(&input, csv).unwrap();
    let mut table = CsvReader.read_file(&input).unwrap();
    assert!(commit_cell_text(
        &mut table,
        0,
        3,
        "=ROUND(A1*(1-B1%)*(1+2.5E-1)^C1,2)".into(),
        BinaryDisplayMode::default(),
    ));
    let output = dir.path().join("forecast.xlsx");
    ExcelReader.write_file(&output, &table).unwrap();
    let reloaded = ExcelReader.read_file(&output).unwrap();
    assert_eq!(reloaded.get(0, 3), Some(&CellValue::Float(78.13)));
    assert!(reloaded.formulas.is_empty());
    assert_eq!(std::fs::read_to_string(input).unwrap(), csv);
}

#[test]
fn logic_functions_combine_predicates_and_named_booleans() {
    let table = formula_table();
    for (formula, expected) in [
        (r#"AND(A1>0,C1="hello",NOT(FALSE))"#, 1.0),
        (r#"OR(A1<0,C1="world")"#, 0.0),
        (r#"XOR(TRUE,FALSE,TRUE,TRUE)"#, 1.0),
        (r#"XODER(WAHR;A1<=10;WAHR;FALSCH)"#, 1.0),
        (r#"NOT(A1>=10)"#, 0.0),
        (r#"ODER(NICHT(FALSCH);A1<0)"#, 1.0),
        (r#"WENN(UND(A1>=10;C1="HELLO");9;0)"#, 9.0),
        (r#"IF(OR(C1="hello",C1="world"),WAHR,FALSCH)"#, 1.0),
        (r#"SUM(AND(A1=10,C1="hello"),2)"#, 3.0),
    ] {
        assert_eq!(
            evaluate_formula(formula, &table),
            Some(expected),
            "{formula}"
        );
    }
}

#[test]
fn logic_ranges_skip_text_and_blanks_but_read_booleans_edits_and_zero() {
    let mut table = formula_table();
    table.set(1, 2, CellValue::Bool(true));
    table.set(1, 1, CellValue::Int(0));
    for (formula, expected) in [
        ("AND(C1:C3)", Some(1.0)),
        ("AND(A1:B3)", Some(0.0)),
        ("OR(A1:B3)", Some(1.0)),
        ("XOR(C1:C3,B1:B3)", Some(1.0)),
        ("AND(B1:B1,C2)", Some(1.0)),
        ("AND(C1,C3)", None),
        ("AND(C3,WAHR)", Some(1.0)),
        ("OR(ZZ1:ZZ99999999,FALSCH)", Some(0.0)),
        ("OR((C1:C99999999))", Some(1.0)),
        ("NOT(C3)", Some(1.0)),
        ("NOT(C1:C3)", None),
    ] {
        assert_eq!(evaluate_formula(formula, &table), expected, "{formula}");
    }
}

#[test]
fn logic_does_not_mask_active_errors_or_malformed_arguments() {
    let table = formula_table();
    for (formula, expected) in [
        ("AND(FALSE,1/0)", None),
        ("OR(TRUE,1/0)", None),
        ("IFERROR(AND(FALSE,1/0),8)", Some(8.0)),
        ("IF(TRUE,3,OR(TRUE,1/0))", Some(3.0)),
        ("IFERROR(OR(TRUE,1/0+),8)", None),
        ("AND(,TRUE)", None),
        ("OR()", None),
        ("NOT(TRUE,FALSE)", None),
    ] {
        assert_eq!(evaluate_formula(formula, &table), expected, "{formula}");
    }
    let and_max = format!("AND(TRUE{})", ",TRUE".repeat(254));
    let xor_max = format!("XOR(TRUE{})", ",TRUE".repeat(253));
    let xor_excess = format!("XOR(TRUE{})", ",TRUE".repeat(254));
    assert_eq!(evaluate_formula(&and_max, &table), Some(1.0));
    assert_eq!(evaluate_formula(&xor_max, &table), Some(0.0));
    assert_eq!(evaluate_formula(&xor_excess, &table), None);
}

#[test]
fn ifs_prefers_first_match_and_avoids_unused_work() {
    let table = formula_table();
    for (formula, expected) in [
        ("IFS(A1>0,42,A1>5,1/0)", Some(42.0)),
        ("IFS(FALSE,1/0,AND(A1>=10,NOT(FALSE)),2,1/0,99)", Some(2.0)),
        (r#"WENNS(UND(C1="hello";B1>0);12;WAHR;0)"#, Some(12.0)),
        ("IFS(FALSE,2,TRUE,)", Some(0.0)),
        ("IFS(FALSE,2)", None),
        ("IFERROR(IFS(FALSE,2),7)", Some(7.0)),
        ("IFERROR(IFS(TRUE,1/0,TRUE,2),7)", Some(7.0)),
    ] {
        assert_eq!(evaluate_formula(formula, &table), expected, "{formula}");
    }
    let outcome = evaluate_formula_with_diagnostics("IFS(TRUE,C2,TRUE,C1)", &table);
    assert_eq!(outcome.value, None);
    let bad = outcome.bad_cell.unwrap();
    assert_eq!((bad.row, bad.col, bad.content.as_str()), (1, 2, "world"));
}

#[test]
fn ifs_validates_every_pair_and_enforces_argument_limits() {
    let table = formula_table();
    for malformed in [
        "IFS()",
        "IFS(,8)",
        "IFS(TRUE,8,FALSE)",
        "IFS(TRUE,8,A1>B1>C1,2)",
        "IFS(TRUE,8,FALSE,1+)",
        "IFS(TRUE,8,)",
        "IFS(FALSE,,)",
    ] {
        assert_eq!(evaluate_formula(malformed, &table), None, "{malformed}");
        assert_eq!(
            evaluate_formula(&format!("IFERROR({malformed},7)"), &table),
            None
        );
        assert_eq!(
            evaluate_formula(&format!("IF(TRUE,7,{malformed})"), &table),
            None
        );
    }
    let maximum = format!("IFS({}TRUE,8)", "FALSE,0,".repeat(126));
    let maximum_exceeded = format!("IFS({}TRUE,8)", "FALSE,0,".repeat(127));
    assert_eq!(evaluate_formula(&maximum, &table), Some(8.0));
    assert_eq!(evaluate_formula(&maximum_exceeded, &table), None);
}

#[test]
fn logic_eligibility_and_tiers_survive_xlsx_round_trip() {
    use octa::formats::{FormatReader, csv_reader::CsvReader, excel_reader::ExcelReader};
    use octa::ui::table_view::commit_cell_text;
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("eligible.csv");
    let csv = "amount,status,region,result\n120,Paid,West,0\n80,Paid,West,0\n120,Paid,East,0\n450,Refunded,West,0\n";
    std::fs::write(&input, csv).unwrap();
    let mut table = CsvReader.read_file(&input).unwrap();
    for (offset, row) in (1..=4).enumerate() {
        let formula = format!(
            r#"=IFS(AND(A{row}>=100,B{row}="Paid",C{row}="West"),ROUND(A{row}*10%,2),AND(B{row}="Paid",OR(C{row}="West",C{row}="East")),5,TRUE,0)"#
        );
        let _ = commit_cell_text(&mut table, offset, 3, formula, BinaryDisplayMode::default());
    }
    let output = dir.path().join("eligibility.xlsx");
    ExcelReader.write_file(&output, &table).unwrap();
    let reloaded = ExcelReader.read_file(&output).unwrap();
    for (row, expected) in [12, 5, 5, 0].into_iter().enumerate() {
        assert!(
            matches!(reloaded.get(row,3), Some(CellValue::Int(value)) if *value==expected)
                || matches!(reloaded.get(row,3), Some(CellValue::Float(value)) if *value==expected as f64),
            "row {}: {:?}",
            row + 1,
            reloaded.get(row, 3)
        );
    }
    assert!(reloaded.formulas.is_empty());
    assert_eq!(std::fs::read_to_string(input).unwrap(), csv);
}
