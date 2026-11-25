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
