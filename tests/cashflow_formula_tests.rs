use octa::data::{
    CellValue as C, ColumnInfo, DataTable, evaluate_formula as eval,
    evaluate_formula_with_diagnostics,
};

fn table(rows: Vec<Vec<C>>) -> DataTable {
    let width = rows.first().unwrap().len();
    DataTable {
        columns: (0..width)
            .map(|index| ColumnInfo {
                name: format!("input-{index}"),
                data_type: "Any".into(),
            })
            .collect(),
        rows,
        ..DataTable::empty()
    }
}

fn near(table: &DataTable, formula: &str, expected: f64) {
    let got = eval(formula, table).unwrap_or_else(|| panic!("no result for {formula}"));
    assert!(
        (got - expected).abs() <= 2e-12 * expected.abs().max(1.0),
        "{formula}: {got} vs {expected}"
    );
}

fn schedule() -> DataTable {
    table(
        [
            (-10_000, "2008-01-01"),
            (2750, "2008-03-01"),
            (4250, "2008-10-30"),
            (3250, "2009-02-15"),
            (2750, "2009-04-01"),
        ]
        .into_iter()
        .map(|(amount, date)| vec![C::Int(amount), C::Date(date.into())])
        .collect(),
    )
}

#[test]
fn periodic_cash_flows_begin_at_end_of_period_one() {
    let data = table(vec![
        vec![C::Int(-100)],
        vec![C::Int(110)],
        vec![C::Int(121)],
    ]);
    for (expr, expected) in [
        ("NPV(10%,A2:A3)", 200.0),
        ("A1+NBW(10%;A2:A3)", 100.0),
        ("NPV(10%,A1:A3)", 90.909_090_909_090_9),
        ("NPV(10%,11*10,(11^2))", 200.0),
        ("NPV(-50%,100,110)", 640.0),
        ("npv(0,A1:A3)", 131.0),
    ] {
        near(&data, expr, expected);
    }
}

#[test]
fn blanks_and_numeric_text_do_not_advance_a_period_but_zero_does() {
    let data = table(vec![
        vec![C::Int(110)],
        vec![C::String("120".into())],
        vec![C::Null],
        vec![C::Bool(true)],
        vec![C::Int(0)],
        vec![C::Float(133.1)],
    ]);
    near(&data, "NPV(10%,A1:A6)", 200.0);
    near(&data, "NPV(10%,A1,\"120\",TRUE,A5:A6)", 200.0);
    near(&data, "NPV(0,A2:A4)", 0.0);
    near(&data, "NPV(10%,A1:A100000000)", 200.0);
    let rows = table(vec![
        vec![C::Int(16), C::Int(8)],
        vec![C::Int(4), C::Int(2)],
    ]);
    near(&rows, "NPV(100%,A1:B2)", 10.625);
    near(&rows, "NPV(100%,B2:A1)", 10.625);
    near(&rows, "NPV(100%,A1:A2,B1:B2)", 10.125);
}

#[test]
fn periodic_value_checks_runtime_rate_and_argument_limits() {
    let data = schedule();
    for expr in [
        "NPV()",
        "NPV(0)",
        "NPV(10%,)",
        "NPV(-100%,100)",
        "NPV(-150%,100)",
        "NPV(1/0,100)",
        "NPV(10%,1+)",
        "NPV(10%,A1:A5+1)",
    ] {
        assert_eq!(eval(expr, &data), None, "{expr}");
    }
    let accepted = format!("NPV(0{})", ",1".repeat(254));
    let rejected = format!("NPV(0{})", ",1".repeat(255));
    near(&data, &accepted, 254.0);
    assert_eq!(eval(&rejected, &data), None);
}

#[test]
fn dated_payments_match_independent_day_count_and_decimal_discounting() {
    let data = schedule();
    // Independent Decimal evaluation at 50 digits, with Gregorian day counts.
    // The same nine-percent example is given in the Excel XNPV documentation.
    for (expr, want) in [
        ("XNPV(9%,A1:A5,B1:B5)", 2086.647_602_031_536_6),
        ("XKAPITALWERT(-9%;A1:A5;B1:B5)", 4097.080_210_678_558),
        ("XNPV(0,A1:A5,B1:B5)", 3000.0),
        ("XNPV(1e-16,A1:A5,B1:B5)", 2999.999_999_999_999),
    ] {
        near(&data, expr, want);
    }
    let mut unordered = schedule();
    unordered.rows.swap(1, 4);
    near(&unordered, "XNPV(9%,A1:A5,B1:B5)", 2086.647_602_031_536_6);
    unordered.rows.swap(0, 2);
    assert_eq!(eval("XNPV(9%,A1:A5,B1:B5)", &unordered), None);
}

#[test]
fn dated_payments_use_typed_dates_serials_and_equal_day_payments() {
    let data = table(vec![
        vec![C::Int(-100), C::String("2024-01-01".into())],
        vec![C::Int(22), C::DateTime("2024-12-31T23:30:00-03:00".into())],
        vec![C::Int(33), C::Float(45_657.75)],
        vec![C::Int(5), C::Date("2024-01-01".into())],
    ]);
    near(&data, "XNPV(10%,A1:A4,B1:B4)", -45.0);
    near(&data, "XNPV(10%,A4:A1,B4:B1)", -45.0);
    let crossed = table(vec![
        vec![C::Int(-100), C::Int(110), C::Date("2024-01-01".into())],
        vec![C::Null, C::Null, C::Date("2024-12-31".into())],
    ]);
    near(&crossed, "XNPV(10%,A1:B1,C1:C2)", 0.0);
}

#[test]
fn invalid_schedule_shapes_or_cells_never_shift_payments() {
    let data = schedule();
    for expr in [
        "XNPV(9%,A1:A4,B1:B5)",
        "XNPV(9%,A1:A100000000,B1:B100000000)",
        "XNPV(9%,A1:A5,Z1:Z5)",
        "XNPV(9%,A1:B2,A2:B3)",
        "XNPV(9%,A2:A5,B2:B5)",
        "XNPV(9%,A1,B1)",
        "XNPV(9%,A1:A5)",
        "XNPV(-100%,A1:A5,B1:B5)",
    ] {
        assert_eq!(eval(expr, &data), None, "{expr}");
    }
    for (col, bad) in [
        (0, C::Null),
        (0, C::String("4250".into())),
        (0, C::Float(f64::INFINITY)),
        (1, C::Null),
        (1, C::Float(60.0)),
        (1, C::String("not a date".into())),
    ] {
        let mut data = schedule();
        data.set(2, col, bad);
        let out = evaluate_formula_with_diagnostics("XNPV(9%,A1:A5,B1:B5)", &data);
        assert_eq!(out.value, None);
        assert_eq!(
            out.bad_cell.map(|cell| (cell.row, cell.col)),
            Some((2, col))
        );
    }
}

#[test]
fn dated_and_periodic_values_read_edits_and_work_inside_lazy_formulas() {
    let mut data = schedule();
    data.set(1, 0, C::Int(3750));
    near(&data, "XNPV(0,A1:A5,B1:B5)", 4000.0);
    near(
        &data,
        "IF(TRUE,NPV(0,A2:A5),XNPV(-1,Z1:Z10000,B1:B10000))",
        14_000.0,
    );
    near(&data, "IFERROR(XNPV(1/0,A1:A5,B1:B5),17)", 17.0);
    near(&data, "IFERROR(NPV(-100%,1),17)", 17.0);
    assert_eq!(eval("IFERROR(XNPV(9%,A1:A4+,B1:B5),17)", &data), None);
    assert_eq!(eval("IF(TRUE,1,NPV(-1,1+))", &data), None);
}

#[test]
fn nearly_cancelling_amounts_and_extreme_factors_do_not_round_to_false_zero() {
    let data = table(vec![
        vec![C::Float(1e300), C::Date("2024-01-01".into())],
        vec![C::Int(1), C::Date("2024-01-01".into())],
        vec![C::Float(-1e300), C::Date("2024-01-01".into())],
    ]);
    near(&data, "XNPV(12%,A1:A3,B1:B3)", 1.0);
    near(&data, "NPV(0,A1:A3)", 1.0);
    let got = eval("NPV(1e200,0,A1)", &data).unwrap();
    assert!((got / 1e-100 - 1.0).abs() < 1e-12, "{got}");
    assert!(eval("NPV(-99.99999%,A1,A1)", &data).is_none());
}
