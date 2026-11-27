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

#[test]
fn completeness_distinguishes_null_empty_text_zero_and_unloaded_cells() {
    use CellValue::{Binary, Bool, Date, DateTime, Float, Int, Null, String as Text};
    let mut table = DataTable {
        rows: vec![
            vec![Null, Text("".into()), Int(0)],
            vec![Bool(false), Date("2024-03-01".into()), Text(" ".into())],
            vec![Binary(vec![0, 1]), Float(0.0)],
            vec![
                Text("#N/A".into()),
                DateTime("2024-03-01 06:30:00".into()),
                Null,
            ],
        ],
        row_offset: 2000,
        total_rows: Some(2_000_000),
        ..formula_table()
    };
    for (formula, count) in [
        ("COUNTA(C4:A1)", 9.0),
        ("ANZAHL2 (A1:C4)", 9.0),
        ("COUNTBLANK(C4:A1)", 4.0),
        ("ANZAHLLEEREZELLEN(A1:ZZ9999999999)", 4.0),
        ("COUNTBLANK(A999999998:A9999999999)", 0.0),
        ("COUNTA(Z1:ZX9999999999)", 0.0),
        ("COUNTBLANK(C3)", 1.0),
        ("ISTLEER((A1))", 1.0),
        ("ISBLANK(B1)", 0.0),
        ("ISBLANK(C1)", 0.0),
        ("ISBLANK(C3)", 1.0),
        ("ISBLANK(0)", 0.0),
        (r#"ISBLANK("")"#, 0.0),
        (r#"COUNTA(A1; ""; WAHR; 2+5; B1)"#, 4.0),
    ] {
        assert_eq!(evaluate_formula(formula, &table), Some(count), "{formula}");
    }
    assert_eq!(evaluate_formula("ISBLANK(A9000)", &table), None);
    assert_eq!(evaluate_formula("ISBLANK(Z1)", &table), None);
    assert_eq!(evaluate_formula("COUNT(A1:C4)", &table), Some(2.0));
    table.set(0, 0, Text("received".into()));
    table.set(0, 1, Null);
    assert_eq!(evaluate_formula("ISBLANK(A1)", &table), Some(0.0));
    assert_eq!(evaluate_formula("ISBLANK(B1)", &table), Some(1.0));
    assert_eq!(evaluate_formula("COUNTBLANK(A1:C4)", &table), Some(3.0));
    assert_eq!(evaluate_formula("COUNTA(A1:C4)", &table), Some(9.0));
}

#[test]
fn completeness_validates_arguments_even_in_an_inactive_branch() {
    let table = formula_table();
    for formula in [
        "COUNTA()",
        "COUNTA(A1,)",
        "COUNTBLANK(A1,A2)",
        "COUNTBLANK(A1:)",
        "COUNTBLANK(2)",
        "ISBLANK()",
        "ISBLANK(A1:A2)",
        "ISBLANK(A1,A2)",
        "IF(TRUE,7,COUNTBLANK(A1,A2))",
    ] {
        assert_eq!(evaluate_formula(formula, &table), None, "{formula}");
    }
    assert_eq!(
        evaluate_formula("IF(TRUE,7,COUNTA(1/0))", &table),
        Some(7.0)
    );
    assert_eq!(
        evaluate_formula("IFERROR(ISBLANK(A9000),8)", &table),
        Some(8.0)
    );
    let max = format!("COUNTA({})", vec!["0"; 255].join(","));
    let rejected = format!("COUNTA({})", vec!["0"; 256].join(","));
    assert_eq!(evaluate_formula(&max, &table), Some(255.0));
    assert_eq!(evaluate_formula(&rejected, &table), None);
}

#[test]
fn completeness_report_keeps_zero_false_and_missing_distinct_in_xlsx() {
    use octa::formats::{FormatReader, csv_reader::CsvReader, excel_reader::ExcelReader};
    use octa::ui::table_view::commit_cell_text;
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("approvals.csv");
    std::fs::write(&source,
        "Order,Approval,Recorded,Missing,Review\nINV-1,FALSE,0,0,0\nINV-2,,0,0,0\nINV-3,TBD,0,0,0\nINV-4,0,0,0,0\n"
    ).unwrap();
    let mut table = CsvReader.read_file(&source).unwrap();
    for (row, col, formula) in [
        (0, 2, "=COUNTA(B1:B4)"),
        (0, 3, "=COUNTBLANK(B1:B4)"),
        (1, 4, "=IF(ISBLANK(B2),1,0)"),
    ] {
        assert!(commit_cell_text(
            &mut table,
            row,
            col,
            formula.into(),
            BinaryDisplayMode::default()
        ));
    }
    let output = dir.path().join("approval-report.xlsx");
    ExcelReader.write_file(&output, &table).unwrap();
    let report = ExcelReader.read_file(&output).unwrap();
    assert_eq!(evaluate_formula("C1", &report), Some(3.0));
    assert_eq!(evaluate_formula("D1", &report), Some(1.0));
    assert_eq!(evaluate_formula("E2", &report), Some(1.0));
    assert_eq!(evaluate_formula("COUNTBLANK(B1:B4)", &report), Some(1.0));
    assert_eq!(evaluate_formula("ISBLANK(B1)", &report), Some(0.0));
    assert_eq!(evaluate_formula("ISBLANK(B4)", &report), Some(0.0));
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
fn calendar_parts_accept_typed_dates_serials_and_iso_text() {
    let mut table = formula_table();
    table.set(0, 0, CellValue::Date("2024-02-29".into()));
    table.set(0, 1, CellValue::Float(45_292.8));
    table.set(
        0,
        2,
        CellValue::DateTime("2024-03-01T00:15:00+02:00".into()),
    );
    table.set(1, 0, CellValue::String("2025-12-31".into()));
    table.set(1, 1, CellValue::DateTime("2025-03-14 16:30:04.123".into()));
    for (formula, want) in [
        ("JAHR(A1)", 2024.0),
        ("MONAT((A1))", 2.0),
        ("TAG(A1)", 29.0),
        ("DAY(B1)", 1.0),
        ("MONTH(C1)", 3.0),
        ("MONTH(A2)", 12.0),
        ("DAY(B2)", 14.0),
        (r#"YEAR("2024-01-20")"#, 2024.0),
        ("MONTH(DATE(2024,2,29))+DAY(DATE(2024,2,29))", 31.0),
    ] {
        assert_eq!(evaluate_formula(formula, &table), Some(want), "{formula}");
    }
}

#[test]
fn calendar_date_rolls_months_days_and_short_years() {
    let table = formula_table();
    for (formula, want) in [
        ("DATUM(124;2;29)", 45_351.0),
        ("DATE(2024,14,1)", 45_689.0),
        ("DATE(2025,1,0)", 45_657.0),
        ("DATE(2025,0,31)", 45_657.0),
        ("DATE(2024,1.9,1)", 45_292.0),
        ("DATE(2024,2,32)", 45_354.0),
    ] {
        assert_eq!(evaluate_formula(formula, &table), Some(want), "{formula}");
    }
}

#[test]
fn calendar_serial_boundaries_skip_fictitious_excel_day() {
    let table = formula_table();
    for (formula, want) in [
        ("DATE(1900,1,1)", Some(1.0)),
        ("DATE(1900,2,28)", Some(59.0)),
        ("DATE(1900,2,29)", Some(61.0)),
        ("DATE(1900,3,1)", Some(61.0)),
        ("DATE(9999,12,31)", Some(2_958_465.0)),
        ("YEAR(2958465.75)", Some(9999.0)),
        ("DAYS(61,59)", Some(1.0)),
        ("WEEKDAY(1,2)", Some(1.0)),
        ("YEAR(0)", None),
        ("DAY(60.5)", None),
        ("YEAR(2958466)", None),
    ] {
        assert_eq!(evaluate_formula(formula, &table), want, "{formula}");
    }
}

#[test]
fn calendar_weekday_numbering_and_day_differences() {
    let mut table = formula_table();
    table.set(0, 0, CellValue::Date("2024-02-29".into()));
    for (mode, want) in [
        (1, 5),
        (2, 4),
        (3, 3),
        (11, 4),
        (12, 3),
        (13, 2),
        (14, 1),
        (15, 7),
        (16, 6),
        (17, 5),
    ] {
        let expr = format!("WEEKDAY(A1,{mode})");
        assert_eq!(
            evaluate_formula(&expr, &table),
            Some(f64::from(want)),
            "{expr}"
        );
    }
    assert_eq!(evaluate_formula("WOCHENTAG(A1)", &table), Some(5.0));
    assert_eq!(
        evaluate_formula("TAGE(DATUM(2024;3;1);A1)", &table),
        Some(1.0)
    );
    assert_eq!(
        evaluate_formula(r#"DAYS("2024-02-28",A1)"#, &table),
        Some(-1.0)
    );
}

#[test]
fn calendar_rejects_invalid_dates_and_preserves_lazy_control() {
    let mut table = formula_table();
    table.set(0, 0, CellValue::String("12/11/2024".into()));
    table.set(0, 1, CellValue::Null);
    table.set(0, 2, CellValue::Bool(true));
    for formula in [
        "YEAR(A1)",
        "YEAR(B1)",
        "YEAR(C1)",
        "YEAR(TRUE)",
        r#"DAY("2024-02-30")"#,
        r#"DAY("2024-2-09")"#,
        r#"DAY("12/11/2024")"#,
        "YEAR(1,2)",
        "DATE(2024,2,3,4)",
        "DATE(2024,1E100,3)",
        "DATE(2024,2,1E100)",
        "DATE(-1,2,3)",
        "DAYS(45292)",
        "WEEKDAY(45292,9)",
        "WEEKDAY(45292,12.5)",
        "IF(TRUE,7,DATE(2024,2,))",
    ] {
        assert_eq!(evaluate_formula(formula, &table), None, "{formula}");
    }
    let bad = evaluate_formula_with_diagnostics("YEAR(A1)", &table)
        .bad_cell
        .unwrap();
    assert_eq!(
        (bad.row, bad.col, bad.content.as_str()),
        (0, 0, "12/11/2024")
    );
    assert_eq!(
        evaluate_formula(r#"IF(TRUE,7,YEAR("2024-02-30"))"#, &table),
        Some(7.0)
    );
    assert_eq!(evaluate_formula("IFERROR(DAY(60),9)", &table), Some(9.0));
    assert_eq!(evaluate_formula("IFERROR(YEAR(A1),9)", &table), Some(9.0));
}

#[test]
fn calendar_invoice_xlsx_round_trip_keeps_dates_and_numeric_age() {
    use octa::formats::{FormatReader, excel_reader::ExcelReader};
    use octa::ui::table_view::commit_cell_text;
    use rust_xlsxwriter::{ExcelDateTime, Format, Workbook};

    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("invoices.xlsx");
    let mut book = Workbook::new();
    let sheet = book.add_worksheet().set_name("Invoices").unwrap();
    sheet.write_string(0, 0, "Invoice date").unwrap();
    sheet.write_string(0, 1, "Age on 31 March").unwrap();
    let date = ExcelDateTime::parse_from_str("2024-02-29").unwrap();
    sheet
        .write_datetime_with_format(1, 0, &date, &Format::new().set_num_format("yyyy-mm-dd"))
        .unwrap();
    sheet.write_number(1, 1, 0).unwrap();
    book.save(&source).unwrap();
    let mut table = ExcelReader.read_file(&source).unwrap();
    assert_eq!(
        table.get(0, 0),
        Some(&CellValue::DateTime("2024-02-29 00:00:00".into()))
    );
    assert!(commit_cell_text(
        &mut table,
        0,
        1,
        "=DAYS(DATE(2024,3,31),A1)".into(),
        BinaryDisplayMode::default(),
    ));
    assert_eq!(evaluate_formula("B1", &table), Some(31.0));
    let report = dir.path().join("invoice-age.xlsx");
    ExcelReader.write_file(&report, &table).unwrap();
    let reopened = ExcelReader.read_file(&report).unwrap();
    assert!(matches!(
        reopened.get(0, 0),
        Some(CellValue::Date(_) | CellValue::DateTime(_))
    ));
    assert_eq!(evaluate_formula("DAY(A1)", &reopened), Some(29.0));
    assert_eq!(evaluate_formula("B1", &reopened), Some(31.0));
}

#[test]
fn time_builds_excel_day_fractions_and_wraps_overflow() {
    let table = formula_table();
    for (formula, expected) in [
        ("TIME(12,0,0)", 0.5),
        ("ZEIT(27;0;0)", 0.125),
        ("TIME(0,750,0)", 45_000.0 / 86_400.0),
        ("TIME(0,0,2000)", 2_000.0 / 86_400.0),
        ("TIME(23,59,60)", 0.0),
        ("TIME(A1+2,0.5,0.8)", 0.5),
        ("TIME(32767,32767,32767)", 36787.0 / 86_400.0),
    ] {
        assert_eq!(
            evaluate_formula(formula, &table),
            Some(expected),
            "{formula}"
        );
    }
    for formula in [
        "TIME(-1,0,0)",
        "TIME(0,32768,0)",
        "TIME(0,0,1E300)",
        "TIME(1,2)",
        "TIME(,2,0)",
    ] {
        assert_eq!(evaluate_formula(formula, &table), None, "{formula}");
    }
}

#[test]
fn clock_parts_use_local_time_and_support_elapsed_and_serials() {
    use octa::data::times::{NANOS_PER_SEC, parse_time};
    let mut table = formula_table();
    table.set(0, 0, CellValue::Time(parse_time("23:45:06.998").unwrap()));
    table.set(
        0,
        1,
        CellValue::DateTime("2026-11-26T00:01:02.234+13:00".into()),
    );
    table.set(0, 2, CellValue::Duration(98_109 * NANOS_PER_SEC));
    table.set(1, 0, CellValue::Float(45_000.0 + 86_399.0 / 86_400.0));
    table.set(1, 1, CellValue::Date("2026-11-26".into()));
    for (formula, expected) in [
        ("STUNDE(($A$1))", 23.0),
        ("MINUTE(A1)", 45.0),
        ("SEKUNDE(A1)", 6.0),
        ("HOUR(B1)", 0.0),
        ("MINUTE(B1)", 1.0),
        ("SECOND(B1)", 2.0),
        ("HOUR(C1)", 3.0),
        ("MINUTE(C1)", 15.0),
        ("SECOND(C1)", 9.0),
        ("SECOND(A2)", 59.0),
        ("HOUR(B2)", 0.0),
        ("HOUR(0.9999999999999999)", 23.0),
        ("SECOND(0.9999999999999999)", 59.0),
        ("SECOND(TIME(0,0,59)-0.00000000001)", 58.0),
    ] {
        assert_eq!(
            evaluate_formula(formula, &table),
            Some(expected),
            "{formula}"
        );
    }
    for second in 0..60 {
        let formula = format!("SECOND(TIME(23,59,{second}))");
        assert_eq!(
            evaluate_formula(&formula, &table),
            Some(f64::from(second)),
            "{formula}"
        );
    }
}

#[test]
fn timevalue_reads_clock_text_but_refuses_duration_and_bad_references() {
    use octa::data::times::{NANOS_PER_DAY, parse_time};
    let mut table = formula_table();
    table.set(0, 0, CellValue::String(" 6:45 PM ".into()));
    table.set(0, 1, CellValue::Time(NANOS_PER_DAY - 1));
    table.set(0, 2, CellValue::Duration(NANOS_PER_DAY + 1));
    table.set(1, 0, CellValue::Duration(-1));
    table.set(
        1,
        1,
        CellValue::DateTime("2026-11-26 01:23:45.678+0200".into()),
    );
    for (formula, clock) in [
        ("TIMEVALUE(A1)", "18:45"),
        ("TIMEVALUE(B1)", "23:59:59.999999999"),
        ("ZEITWERT(B2)", "01:23:45.678"),
        (r#"TIMEVALUE(("12:00 AM"))"#, "00:00"),
        (r#"TIMEVALUE("2026-11-26T01:23:45.678")"#, "01:23:45.678"),
    ] {
        let expected = parse_time(clock).unwrap() as f64 / NANOS_PER_DAY as f64;
        assert!(
            (evaluate_formula(formula, &table).unwrap() - expected).abs() < 1e-15,
            "{formula}"
        );
    }
    let rejected = evaluate_formula_with_diagnostics("TIMEVALUE(C1)", &table);
    assert!(rejected.value.is_none());
    assert_eq!(rejected.bad_cell.map(|c| (c.row, c.col)), Some((0, 2)));
    for formula in [
        "TIMEVALUE(0.5)",
        "TIMEVALUE(A9000)",
        "HOUR(A2)",
        "SECOND(-0.1)",
        "HOUR(TRUE)",
        r#"TIMEVALUE("24:00")"#,
        r#"TIMEVALUE("2026-11-26")"#,
        r#"TIMEVALUE("2026-11-26T23:59:60Z")"#,
        r#"TIMEVALUE("2026-02-30T00:01:00Z")"#,
        "HOUR(A1:B1)",
        "MINUTE(A1,B1)",
    ] {
        assert_eq!(evaluate_formula(formula, &table), None, "{formula}");
    }
}

#[test]
fn mod_keeps_the_divisor_sign_and_validates_lazy_branches() {
    let table = formula_table();
    for (formula, expected) in [
        ("MOD(3,2)", 1.0),
        ("MOD(-3,2)", 1.0),
        ("MOD(3,-2)", -1.0),
        ("REST(-3;-2)", -1.0),
        ("MOD(-6,2)", 0.0),
        ("MOD(1E300,2)", 0.0),
        ("MOD(-1E-20,1)", 1.0_f64.next_down()),
        ("MOD(1E-20,-1)", (-1.0_f64).next_up()),
        ("IFERROR(MOD(2,0),42)", 42.0),
        ("IF(TRUE,7,TIMEVALUE(A100))", 7.0),
        ("IF(TRUE,7,HOUR(MOD(2,0)))", 7.0),
    ] {
        assert_eq!(
            evaluate_formula(formula, &table),
            Some(expected),
            "{formula}"
        );
    }
    for formula in [
        "MOD(3,0)",
        "MOD(3,-0)",
        "MOD(3,)",
        "MOD(3,2,1)",
        "IF(TRUE,7,TIMEVALUE())",
        "IF(TRUE,7,MINUTE(A1:B1))",
        "IF(TRUE,7,TIME(12,,0))",
        "IF(TRUE,7,MOD(1,))",
    ] {
        assert_eq!(evaluate_formula(formula, &table), None, "{formula}");
    }
}

#[test]
fn overnight_shift_xlsx_round_trip_keeps_clocks_and_numeric_hours() {
    use octa::data::times::parse_time;
    use octa::formats::{FormatReader, excel_reader::ExcelReader};
    use octa::ui::table_view::commit_cell_text;
    use rust_xlsxwriter::{Format, Workbook};

    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("shifts.xlsx");
    let mut book = Workbook::new();
    let sheet = book.add_worksheet().set_name("Shifts").unwrap();
    sheet.write_string(0, 0, "Start").unwrap();
    sheet.write_string(0, 1, "Finish").unwrap();
    sheet.write_string(0, 2, "Hours").unwrap();
    let clock = Format::new().set_num_format("hh:mm:ss");
    sheet
        .write_number_with_format(1, 0, 22.5 / 24.0, &clock)
        .unwrap();
    sheet
        .write_number_with_format(1, 1, 6.25 / 24.0, &clock)
        .unwrap();
    sheet.write_number(1, 2, 0).unwrap();
    book.save(&source).unwrap();

    let clocks = [
        CellValue::Time(parse_time("22:30").unwrap()),
        CellValue::Time(parse_time("06:15").unwrap()),
    ];
    let mut table = ExcelReader.read_file(&source).unwrap();
    for (col, value) in clocks.iter().enumerate() {
        assert_eq!(table.get(0, col), Some(value));
    }
    assert!(commit_cell_text(
        &mut table,
        0,
        2,
        "=24*MOD(TIMEVALUE(B1)-TIMEVALUE(A1),1)".into(),
        BinaryDisplayMode::default(),
    ));
    assert!((evaluate_formula("C1", &table).unwrap() - 7.75).abs() < 1e-12);
    let report = dir.path().join("shift-hours.xlsx");
    ExcelReader.write_file(&report, &table).unwrap();
    let reopened = ExcelReader.read_file(&report).unwrap();
    for (col, value) in clocks.iter().enumerate() {
        assert_eq!(reopened.get(0, col), Some(value));
    }
    assert!(
        matches!(reopened.get(0, 2), Some(CellValue::Float(hours)) if (hours - 7.75).abs() < 1e-12)
    );
}

#[test]
fn networkdays_counts_both_ends_and_ignores_duplicate_and_weekend_holidays() {
    let mut table = formula_table();
    table.set(0, 0, CellValue::Date("2024-03-04".into()));
    table.set(0, 1, CellValue::DateTime("2024-03-08 19:45:00".into()));
    table.set(0, 2, CellValue::Date("2024-03-06".into()));
    table.set(1, 2, CellValue::String("2024-03-06".into()));
    table.set(2, 2, CellValue::Date("2024-03-09".into()));
    for (formula, expected) in [
        ("NETWORKDAYS(A1,B1)", 5.0),
        ("NETWORKDAYS(A1,A1)", 1.0),
        ("NETTOARBEITSTAGE(A1;B1;$C$1:$C$999999)", 4.0),
        ("NETWORKDAYS(B1,A1,C3:C1)", -4.0),
        (r#"NETWORKDAYS(A1,B1,"2024-03-04")"#, 4.0),
        (r#"NETWORKDAYS("2024-03-09","2024-03-10")"#, 0.0),
    ] {
        assert_eq!(
            evaluate_formula(formula, &table),
            Some(expected),
            "{formula}"
        );
    }
}

#[test]
fn workday_finds_signed_deadlines_without_counting_the_start() {
    let mut table = formula_table();
    table.set(0, 2, CellValue::Date("2024-03-11".into()));
    for (formula, expected) in [
        ("DAY(WORKDAY(DATE(2024,3,8),1,C1))", 12.0),
        ("TAG(ARBEITSTAG(DATUM(2024;3;12);-1;C1))", 8.0),
        ("DAY(WORKDAY(DATE(2024,3,9),0,C1))", 9.0),
        ("DAY(WORKDAY(DATE(2024,3,9),1.9,C1))", 12.0),
        ("DAY(WORKDAY(DATE(2024,3,10),-1.9,C1))", 8.0),
        ("WORKDAY(DATE(1900,1,1),1000000)", 1_400_002.0),
    ] {
        assert_eq!(
            evaluate_formula(formula, &table),
            Some(expected),
            "{formula}"
        );
    }
}

#[test]
fn workday_intl_accepts_weekend_codes_and_monday_first_masks() {
    let mut table = formula_table();
    table.set(0, 2, CellValue::String("0000110".into()));
    for (formula, expected) in [
        (r#"NETWORKDAYS.INTL("2024-03-08","2024-03-10",7)"#, 1.0),
        (
            r#"NETTOARBEITSTAGE.INTL("2024-03-08";"2024-03-10";C1)"#,
            1.0,
        ),
        (r#"NETWORKDAYS.INTL("2024-03-08","2024-03-10",11)"#, 2.0),
        (r#"NETWORKDAYS.INTL("2024-03-08","2024-03-10",17)"#, 2.0),
        (r#"NETWORKDAYS.INTL("2024-03-10","2024-03-11",2)"#, 0.0),
        (r#"NETWORKDAYS.INTL("2024-03-10","2024-03-11",12)"#, 1.0),
        (
            r#"NETWORKDAYS.INTL("2024-03-04","2024-03-08","1111111")"#,
            0.0,
        ),
        (r#"NETWORKDAYS.INTL("2024-03-08","2024-03-10",)"#, 1.0),
        (r#"DAY(workday.intl("2024-03-08",1,"0000110"))"#, 10.0),
        (r#"TAG(ARBEITSTAG.INTL("2024-03-08";1;7))"#, 10.0),
    ] {
        assert_eq!(
            evaluate_formula(formula, &table),
            Some(expected),
            "{formula}"
        );
    }
}

#[test]
fn workday_refuses_bad_calendars_serials_and_out_of_range_offsets() {
    let mut table = formula_table();
    table.set(0, 2, CellValue::String("not a date".into()));
    for formula in [
        "WORKDAY(DATE(9999,12,31),1)",
        "WORKDAY(DATE(1900,1,1),-1)",
        "WORKDAY(DATE(2024,3,1),1E90)",
        "WORKDAY(60,1)",
        r#"WORKDAY.INTL("2024-03-08",0,"1111111")"#,
        r#"WORKDAY.INTL("2024-03-08",1,"000011")"#,
        r#"WORKDAY.INTL("2024-03-08",1,"00001x0")"#,
        r#"WORKDAY.INTL("2024-03-08",1,8)"#,
        r#"WORKDAY.INTL("2024-03-08",1,11.5)"#,
        r#"NETWORKDAYS("2024-03-04","2024-03-08",C1)"#,
        r#"IFERROR(WORKDAY.INTL("2024-03-08",1+),99)"#,
        r#"IFERROR(WORKDAY.INTL("2024-03-08",1,7,1,2),99)"#,
    ] {
        assert_eq!(evaluate_formula(formula, &table), None, "{formula}");
    }
    assert_eq!(
        evaluate_formula("IFERROR(WORKDAY(60,1),99)", &table),
        Some(99.0)
    );
    assert_eq!(
        evaluate_formula(
            r#"IF(TRUE,17,WORKDAY.INTL("2024-03-08",1,"bad weekend"))"#,
            &table
        ),
        Some(17.0)
    );
    let result =
        evaluate_formula_with_diagnostics(r#"NETWORKDAYS("2024-03-04","2024-03-08",C1)"#, &table);
    assert_eq!(result.bad_cell.map(|bad| (bad.row, bad.col)), Some((0, 2)));
}

#[test]
fn workday_and_networkdays_follow_dates_in_an_xlsx_delivery_report() {
    use octa::formats::{FormatReader, excel_reader::ExcelReader};
    use octa::ui::table_view::commit_cell_text;
    use rust_xlsxwriter::{ExcelDateTime, Format, Workbook};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("delivery.xlsx");
    let mut book = Workbook::new();
    let sheet = book.add_worksheet();
    for (col, name) in ["Ship date", "Holiday", "End date serial", "Workdays"]
        .into_iter()
        .enumerate()
    {
        sheet.write_string(0, col as u16, name).unwrap();
    }
    let date_format = Format::new().set_num_format("yyyy-mm-dd");
    for (col, date) in [(0, "2024-03-08"), (1, "2024-03-11")] {
        let date = ExcelDateTime::parse_from_str(date).unwrap();
        sheet
            .write_datetime_with_format(1, col, &date, &date_format)
            .unwrap();
    }
    sheet.write_number(1, 2, 0).unwrap();
    sheet.write_number(1, 3, 0).unwrap();
    book.save(&path).unwrap();
    let mut table = ExcelReader.read_file(&path).unwrap();
    for (col, formula) in [(2, "=WORKDAY(A1,2,B1)"), (3, "=NETWORKDAYS(A1,C1,B1)")] {
        assert!(commit_cell_text(
            &mut table,
            0,
            col,
            formula.into(),
            BinaryDisplayMode::default()
        ));
    }
    let export = dir.path().join("delivery-report.xlsx");
    ExcelReader.write_file(&export, &table).unwrap();
    let reopened = ExcelReader.read_file(&export).unwrap();
    assert_eq!(evaluate_formula("DAY(C1)", &reopened), Some(13.0));
    assert_eq!(evaluate_formula("D1", &reopened), Some(3.0));
    assert_eq!(evaluate_formula("DAY(B1)", &reopened), Some(11.0));
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

fn rank_scores() -> DataTable {
    use CellValue::{Bool, Float, Int, Null, String as Text};
    DataTable {
        columns: vec![
            ColumnInfo {
                name: "Score".into(),
                data_type: "Float64".into(),
            },
            ColumnInfo {
                name: "Second".into(),
                data_type: "Float64".into(),
            },
        ],
        rows: vec![
            vec![Int(90), Int(30)],
            vec![Text(" 75 ".into()), Int(40)],
            vec![Float(75.0), Int(50)],
            vec![Int(60), Int(50)],
            vec![Null, Int(60)],
            vec![Text("not yet measured".into()), Int(70)],
            vec![Float(f64::INFINITY), Bool(true)],
            vec![Float(-0.0), Float(0.0)],
        ],
        ..DataTable::empty()
    }
}

#[test]
fn rank_functions_distinguish_shared_places_and_average_ties() {
    let table = rank_scores();
    for (formula, rank) in [
        ("RANK.EQ(A2,A1:A4)", 2.0),
        ("RANK.AVG(A2,A1:A4)", 2.5),
        ("RANK(60,A4:A1)", 4.0),
        ("RANG(A1;A1:A7)", 1.0),
        ("RANG.GLEICH(A2;A1:A4;-1)", 2.0),
        ("RANG.MITTELW(A3;A1:A4;0.5)", 2.5),
        ("RANK.AVG(90,A1:A4,1)", 4.0),
        ("RANK.AVG(-0,A1:B8,1)", 1.5),
        ("RANK.EQ(0,A8:B8)", 1.0),
        ("RANK.EQ(50,B1:B100000000)", 3.0),
        ("RANK.AVG(75,A1:A100000000)", 2.5),
    ] {
        assert_eq!(evaluate_formula(formula, &table), Some(rank), "{formula}");
    }
}

#[test]
fn order_statistics_choose_a_position_without_eliminating_duplicates() {
    let table = rank_scores();
    for (formula, expected) in [
        ("LARGE(A1:A4,2)", 75.0),
        ("LARGE(A1:A4,3)", 75.0),
        ("SMALL(A1:A4,1)", 60.0),
        ("SMALL(A1:A4,4)", 90.0),
        ("kgrößte(A7:A1;2+1)", 75.0),
        ("KKLEINSTE(A1:A7;2)", 75.0),
        ("LARGE(A1:B8,4)", 70.0),
        ("SMALL(B6:B1,3)", 50.0),
        ("LARGE(B1:B999999999,1)", 70.0),
        ("SMALL(A1,1)", 90.0),
    ] {
        assert_eq!(
            evaluate_formula(formula, &table),
            Some(expected),
            "{formula}"
        );
    }
}

#[test]
fn rank_and_nth_require_a_valid_position_and_loaded_numeric_values() {
    let table = rank_scores();
    for formula in [
        "LARGE(A1:A4,0)",
        "LARGE(A1:A4,-1)",
        "LARGE(A1:A4,4.5)",
        "LARGE(A1:A7,5)",
        "LARGE(A1:A4,1e300)",
        "SMALL(A5:A7,1)",
        "SMALL(B400:B500,1)",
        "SMALL(A1:A4,)",
        "SMALL(A1:A2,1,2)",
        "SMALL(A1:A4+2,1)",
        "RANK.EQ(80,A1:A7)",
        "RANK.EQ(A1,A2:A7)",
        "RANK.EQ(90,)",
        "RANK.EQ(90,A1:A7,)",
        "RANK.EQ(90,A1:A7,0,1)",
        "RANK.AVG(A7,A1:A7)",
        "RANK.AVG(90,A1:A7,A7)",
        "IF(1,7,RANK.EQ(90,A1:A7,0,1))",
    ] {
        assert_eq!(evaluate_formula(formula, &table), None, "{formula}");
    }
    for (formula, expected) in [
        ("IFERROR(RANK.EQ(80,A1:A7),0)", 0.0),
        ("IF(1,7,LARGE(A400:A500,0))", 7.0),
        ("RANK.EQ(LARGE(A1:A4,2),A1:A4,0)", 2.0),
    ] {
        assert_eq!(
            evaluate_formula(formula, &table),
            Some(expected),
            "{formula}"
        );
    }
    let bad = evaluate_formula_with_diagnostics("RANK.EQ(A6,A1:A4)", &table)
        .bad_cell
        .unwrap();
    assert_eq!(
        (bad.row, bad.col, bad.content.as_str()),
        (5, 0, "not yet measured")
    );
}

#[test]
fn rank_and_nth_results_reopen_as_numeric_workbook_cells() {
    use octa::formats::{FormatReader, excel_reader::ExcelReader};
    use octa::ui::table_view::commit_cell_text;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ranking-report.xlsx");
    let mut table = rank_scores();
    table.rows.truncate(4);
    for col in 2..6 {
        table.insert_column(col, format!("Result {col}"), "Float64".into());
    }
    let fields = [
        ("=RANG.GLEICH(A2;$A$1:$A$4)", 2.0),
        ("=RANG.MITTELW(A2;$A$1:$A$4)", 2.5),
        ("=KGRÖSSTE($A$1:$A$4;3)", 75.0),
        ("=KKLEINSTE($A$1:$A$4;1)", 60.0),
    ];
    for (i, (formula, _)) in fields.into_iter().enumerate() {
        assert!(commit_cell_text(
            &mut table,
            0,
            i + 2,
            formula.into(),
            BinaryDisplayMode::default()
        ));
    }
    ExcelReader.write_file(&path, &table).unwrap();
    let reopened = ExcelReader.read_file(&path).unwrap();
    for (i, (_, expected)) in fields.into_iter().enumerate() {
        assert_eq!(reopened.get(0, i + 2), Some(&CellValue::Float(expected)));
    }
    assert_eq!(reopened.get(1, 0), Some(&CellValue::String(" 75 ".into())));
}

#[test]
fn text_length_preserves_spaces_escaped_quotes_and_unicode_characters() {
    let mut table = formula_table();
    table.rows[0][2] = CellValue::String(" 001\t".into());
    table.rows[1][2] = CellValue::String("ä😀e\u{301}✈\u{fe0f}".into());
    table.rows[2][0] = CellValue::Bool(false);
    for (formula, expected) in [
        ("LEN(C1)", 5.0),
        ("länge(C2)", 6.0),
        ("LEN(C3)", 0.0),
        (r#"LEN("a""b ")"#, 4.0),
        ("LEN(A3)", 5.0),
        ("LEN(B2)", 1.0),
        ("LEN(-0.0)", 1.0),
        ("LÄNGE((A1+B2))", 2.0),
    ] {
        assert_eq!(
            evaluate_formula(formula, &table),
            Some(expected),
            "{formula}"
        );
    }
}

#[test]
fn text_exact_is_case_sensitive_and_keeps_text_numbers_and_null_distinct() {
    let mut table = formula_table();
    table.rows[0][2] = CellValue::String("0007".into());
    table.rows[0][0] = CellValue::Int(9_007_199_254_740_993);
    for (formula, expected) in [
        (r#"IDENTISCH(C1;"0007")"#, 1.0),
        ("EXACT(C1,7)", 0.0),
        (r#"EXACT(7.0,"7")"#, 1.0),
        (r#"EXACT(A1,"9007199254740993")"#, 1.0),
        (r#"EXACT(A1,"9007199254740992")"#, 0.0),
        (r#"EXACT("Sku","SKU")"#, 0.0),
        (r#"EXACT("Sku ","Sku")"#, 0.0),
        (r#"EXACT("é","é")"#, 0.0),
        (r#"EXACT(C3,"")"#, 1.0),
        ("EXACT(C3,0)", 0.0),
        (r#"EXACT(WAHR,"TRUE")"#, 1.0),
    ] {
        assert_eq!(
            evaluate_formula(formula, &table),
            Some(expected),
            "{formula}"
        );
    }
}

#[test]
fn text_find_reports_original_one_based_offsets_and_literal_wildcards() {
    let table = formula_table();
    for (formula, expected) in [
        (r#"FIND("M","Miriam McGovern")"#, 1.0),
        (r#"FIND("M","Miriam McGovern",3)"#, 8.0),
        (r#"FINDEN("-";"ü😀-A-B";4)"#, 5.0),
        (r#"FIND("😀","a😀a",2)"#, 2.0),
        (r#"FIND("?","a*b?c")"#, 4.0),
        (r#"FIND("*","a*b?c")"#, 2.0),
        (r#"FIND("","äbc",2)"#, 2.0),
        (r#"FIND("","")"#, 1.0),
        (r#"FIND("d","äbcdef",1+2)"#, 4.0),
        (r#"FIND("/","ID"&"/"&A2)"#, 3.0),
    ] {
        assert_eq!(
            evaluate_formula(formula, &table),
            Some(expected),
            "{formula}"
        );
    }
    for formula in [
        r#"FIND("a","ABC")"#,
        r#"FIND("a","abc",0)"#,
        r#"FIND("a","abc",-1)"#,
        r#"FIND("a","abc",1.25)"#,
        r#"FIND("a","abc",1e300)"#,
        r#"FIND("a","😀a",3)"#,
        r#"FIND("","abc",4)"#,
        r#"FIND("a","")"#,
    ] {
        assert_eq!(evaluate_formula(formula, &table), None, "{formula}");
    }
}

#[test]
fn text_checks_validate_arity_and_support_lazy_branches_and_concatenation() {
    let table = formula_table();
    for (formula, expected) in [
        (r#"LEN(C1&("/"&(A1+1)))"#, 8.0),
        (r#"EXACT("ID"&A2,"ID20")"#, 1.0),
        (r#"IF(EXACT(C1,"hello"),LEN(C1),100)"#, 5.0),
        (r#"IFERROR(FIND("/",C1),0)"#, 0.0),
        (r#"IF(1,7,FIND("x",Z99,0))"#, 7.0),
        (r#"IF(1,7,LEN("left"&Z99))"#, 7.0),
    ] {
        assert_eq!(
            evaluate_formula(formula, &table),
            Some(expected),
            "{formula}"
        );
    }
    for formula in [
        "LEN()",
        "LEN(A1,B1)",
        "LEN(A1:A2)",
        "LEN(A0)",
        "LEN(1/0)",
        "LEN(Z99)",
        "LEN(A1&)",
        "LEN(&A1)",
        "EXACT(A1,)",
        "EXACT(A1,B1,A2)",
        "FIND(A1)",
        "FIND(A1,B1,)",
        "FIND(A1,B1,A2,B2)",
        "IF(1,7,LEN(A1&))",
    ] {
        assert_eq!(evaluate_formula(formula, &table), None, "{formula}");
    }
}

#[test]
fn text_checks_report_non_scalar_cells_without_expanding_binary_contents() {
    let mut table = formula_table();
    table.rows[0][2] = CellValue::Binary(vec![0xff; 4_096]);
    table.rows[1][2] = CellValue::Nested("{\"order\":7}".into());
    table.rows[2][2] = CellValue::Float(f64::INFINITY);
    for (r, expected) in [(0, "binary"), (1, "structured data"), (2, "inf")] {
        let out = evaluate_formula_with_diagnostics(&format!("LEN(C{})", r + 1), &table);
        assert!(out.value.is_none());
        let cell = out.bad_cell.unwrap();
        assert_eq!(
            (cell.row, cell.col, cell.content.as_str()),
            (r, 2, expected)
        );
    }
    let out = evaluate_formula_with_diagnostics("IFERROR(LEN(C1),0)", &table);
    assert_eq!(out.value, Some(0.0));
    assert!(out.bad_cell.is_none());
}

#[test]
fn text_checks_save_numeric_results_and_preserve_text_ids_in_xlsx() {
    use octa::formats::{FormatReader, excel_reader::ExcelReader};
    use octa::ui::table_view::commit_cell_text;
    use rust_xlsxwriter::Workbook;
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("imported-codes.xlsx");
    let mut book = Workbook::new();
    let sheet = book.add_worksheet();
    let pairs = [("SKU-01", "SKU-01"), ("0007", "7"), ("é-09 ", "É-09")];
    for (c, label) in ["Printed code", "Expected code", "Length", "Exact", "Hyphen"]
        .into_iter()
        .enumerate()
    {
        sheet.write_string(0, c as u16, label).unwrap();
    }
    for (r, (code, expected)) in pairs.into_iter().enumerate() {
        sheet.write_string(r as u32 + 1, 0, code).unwrap();
        sheet.write_string(r as u32 + 1, 1, expected).unwrap();
        for c in 2..5 {
            sheet.write_number(r as u32 + 1, c, 0).unwrap();
        }
    }
    book.save(&source).unwrap();
    let mut table = ExcelReader.read_file(&source).unwrap();
    for r in 0..3 {
        let n = r + 1;
        let formulae = [
            format!("=LÄNGE(A{n})"),
            format!("=IDENTISCH(A{n};B{n})"),
            format!(r#"=WENNFEHLER(FINDEN("-";A{n});0)"#),
        ];
        for (c, formula) in formulae.into_iter().enumerate() {
            assert!(commit_cell_text(
                &mut table,
                r,
                c + 2,
                formula,
                BinaryDisplayMode::default()
            ));
        }
    }
    let output = dir.path().join("checked-codes.xlsx");
    ExcelReader.write_file(&output, &table).unwrap();
    let table = ExcelReader.read_file(&output).unwrap();
    let expected = [[6.0, 1.0, 4.0], [4.0, 0.0, 0.0], [5.0, 0.0, 2.0]];
    for (r, pair) in pairs.into_iter().enumerate() {
        assert_eq!(table.get(r, 0), Some(&CellValue::String(pair.0.into())));
        assert_eq!(table.get(r, 1), Some(&CellValue::String(pair.1.into())));
        for (c, v) in expected[r].into_iter().enumerate() {
            assert_eq!(table.get(r, c + 2), Some(&CellValue::Float(v)));
        }
    }
}

#[test]
fn directed_rounding_distinguishes_away_from_and_towards_zero() {
    let table = formula_table();
    for (formula, expected) in [
        ("ROUNDUP(3.2,0)", 4.0),
        ("ROUNDUP(76.9,0)", 77.0),
        ("ROUNDUP(3.14159,3)", 3.142),
        ("ROUNDUP(-3.14159,1)", -3.2),
        ("ROUNDUP(31415.92654,-2)", 31500.0),
        ("ROUNDDOWN(3.14159,3)", 3.141),
        ("ROUNDDOWN(-3.14159,1)", -3.1),
        ("ROUNDDOWN(31415.92654,-2)", 31400.0),
        ("AUFRUNDEN(-21.5;-1)", -30.0),
        ("ABRUNDEN(-21.5;-1)", -20.0),
        ("TRUNC(-8.9)", -8.0),
        ("TRUNC(1299.9,-2)", 1200.0),
        ("KÜRZEN(-4.38;1)", -4.3),
        ("kürzen(8.9)", 8.0),
        ("zählenwenn(A1:A3;20)", 1.0),
    ] {
        assert_eq!(
            evaluate_formula(formula, &table),
            Some(expected),
            "{formula}"
        );
    }
}

#[test]
fn directed_rounding_respects_exact_decimal_boundaries_and_tiny_remainders() {
    let table = formula_table();
    for (formula, expected) in [
        ("ROUNDUP(1.07,2)", 1.07),
        ("ROUNDDOWN(1.15,2)", 1.15),
        ("ROUNDUP(-1.1500000000000001,2)", -1.16),
        ("ROUNDDOWN(1.1499999999999997,2)", 1.14),
        ("ROUNDUP(0.000001,2)", 0.01),
        ("ROUNDUP(1.000001,2)", 1.01),
        ("ROUNDDOWN(0.000001,2)", 0.0),
        ("ROUNDUP(1200,-2)", 1200.0),
        ("ROUNDDOWN(-1200,-2)", -1200.0),
        ("ROUNDUP(-0.1,-2)", -100.0),
        ("ROUNDUP(-0.0,-999)", 0.0),
    ] {
        assert_eq!(
            evaluate_formula(formula, &table),
            Some(expected),
            "{formula}"
        );
    }
}

#[test]
fn directed_rounding_bounds_subnormal_and_overflowing_steps() {
    let mut table = formula_table();
    table.rows[0][1] = CellValue::Float(f64::MAX);
    table.rows[1][1] = CellValue::Float(f64::from_bits(1));
    for (formula, expected) in [
        ("ROUNDUP(B1,320)", Some(f64::MAX)),
        ("ROUNDUP(B1,-308)", None),
        ("ROUNDUP(B2,-309)", None),
        ("ROUNDDOWN(B1,-308)", Some(1e308)),
        ("TRUNC(B1,-309)", Some(0.0)),
        ("ROUNDUP(B2,323)", Some(1e-323)),
        ("ROUNDDOWN(B2,323)", Some(0.0)),
        ("TRUNC(B2,324)", Some(f64::from_bits(1))),
    ] {
        assert_eq!(evaluate_formula(formula, &table), expected, "{formula}");
    }
}

#[test]
fn directed_rounding_validates_arguments_and_composes_with_lazy_formulas() {
    let table = formula_table();
    for formula in [
        "ROUNDUP(A1)",
        "ROUNDDOWN(A1)",
        "ROUNDUP(A1,)",
        "ROUNDDOWN(A1,1.5)",
        "TRUNC()",
        "TRUNC(A1,)",
        "TRUNC(A1,1,2)",
        "TRUNC(A1:B2)",
        "TRUNC(A1,1/0)",
        "ROUNDUP(C1,0)",
    ] {
        assert_eq!(evaluate_formula(formula, &table), None, "{formula}");
    }
    for (formula, expected) in [
        ("ROUNDUP(SUM(A1:A3)/7,A1/10)", 8.6),
        ("IFERROR(ROUNDUP(1,-309),ROUNDDOWN(5.6,0))", 5.0),
        ("IF(0,TRUNC(B99),7)", 7.0),
    ] {
        assert_eq!(
            evaluate_formula(formula, &table),
            Some(expected),
            "{formula}"
        );
    }
    assert_eq!(evaluate_formula("IF(0,ROUNDUP(B99),7)", &table), None);
}

#[test]
fn directed_rounding_keeps_calculated_values_numeric_on_xlsx_reopen() {
    use octa::formats::{FormatReader, excel_reader::ExcelReader};
    use octa::ui::table_view::commit_cell_text;

    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("rounded-quantities.xlsx");
    let mut table = DataTable {
        columns: ["Source", "Boxes", "Spend", "Units"]
            .map(|name| ColumnInfo {
                name: name.into(),
                data_type: "Float64".into(),
            })
            .to_vec(),
        rows: vec![vec![
            CellValue::Float(-127.951),
            CellValue::Float(0.0),
            CellValue::Float(0.0),
            CellValue::Float(0.0),
        ]],
        ..DataTable::empty()
    };
    for (c, expr) in ["=ROUNDUP(ABS(A1)/12,0)", "=ABRUNDEN(A1;2)", "=KÜRZEN(A1)"]
        .into_iter()
        .enumerate()
    {
        assert!(commit_cell_text(
            &mut table,
            0,
            1 + c,
            expr.into(),
            BinaryDisplayMode::default()
        ));
    }
    ExcelReader.write_file(&output, &table).unwrap();
    let saved = ExcelReader.read_file(&output).unwrap();
    for (c, expected) in [-127.951, 11.0, -127.95, -127.0].into_iter().enumerate() {
        assert_eq!(saved.get(0, c), Some(&CellValue::Float(expected)));
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
fn conditional_date_criteria_keep_clock_time_and_leave_date_text_alone() {
    use CellValue::{Date, DateTime, Float, Int, Null, String as Text};
    // 15 March 2024 in Excel's 1900 calendar.
    let table = DataTable {
        rows: vec![
            vec![Date("2024-03-15".into()), Date("2024-03-15".into())],
            vec![
                DateTime("2024-03-15T12:00:00+05:30".into()),
                DateTime("2024-03-15 12:00:00".into()),
            ],
            vec![Text("2024-03-15".into()), Text("2024-03-15".into())],
            vec![Int(45_366), Date("1900-02-29".into())],
            vec![Float(45_366.5), Date("1899-12-31".into())],
            vec![
                DateTime("2024-03-15 00:15:00+0530".into()),
                DateTime("2024-03-15 00:00:00.500".into()),
            ],
            vec![DateTime("2024-03-15T00:00:00.500".into()), Null],
            vec![DateTime("1900-02-29 12:00:00".into()), Null],
        ],
        ..formula_table()
    };
    for (formula, count) in [
        ("COUNTIF(A1:A8,DATE(2024,3,15))", 2.0),
        ("COUNTIF(A1:A8,B1)", 2.0),
        ("COUNTIF(A1:A8,B2)", 2.0),
        (r#"COUNTIF(A1:A8,"="&B2)"#, 2.0),
        (r#"COUNTIF(A1:A8,">"&B1)"#, 4.0),
        ("COUNTIF(A1:A8,B6)", 1.0),
        ("COUNTIF(A1:A8,B3)", 2.0),
        (r#"COUNTIF(A1:A8,">="&DATE(2024,3,15))"#, 6.0),
        (r#"COUNTIF(A1:A8,"<>")"#, 8.0),
        ("IFERROR(COUNTIF(A1:A8,B4),99)", 99.0),
        ("IFERROR(COUNTIF(A1:A8,B5),77)", 77.0),
    ] {
        assert_eq!(evaluate_formula(formula, &table), Some(count), "{formula}");
    }
    assert_eq!(evaluate_formula("COUNTIF(A1:A8,B4)", &table), None);
    assert_eq!(evaluate_formula("COUNTIF(A1:A8,A8)", &table), None);

    let boundary = DataTable {
        rows: vec![
            vec![DateTime("2024-03-31 23:59:59.999999999".into())],
            vec![DateTime("2024-04-01T00:00:00.000000001".into())],
            vec![DateTime("2024-03-31T23:45:00-03:00".into())],
        ],
        ..formula_table()
    };
    for (formula, count) in [
        (r#"COUNTIF(A1:A3,"<"&DATE(2024,4,1))"#, 2.0),
        ("COUNTIF(A1:A3,DATE(2024,4,1))", 0.0),
        (r#"COUNTIF(A1:A3,">"&DATE(2024,4,1))"#, 1.0),
    ] {
        assert_eq!(
            evaluate_formula(formula, &boundary),
            Some(count),
            "{formula}"
        );
    }
}

#[test]
fn xlsx_month_end_totals_include_the_last_evening_but_not_next_month() {
    use octa::formats::{FormatReader, excel_reader::ExcelReader};
    use octa::ui::table_view::commit_cell_text;
    use rust_xlsxwriter::{ExcelDateTime, Format, Workbook};

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("invoices.xlsx");
    let mut book = Workbook::new();
    let sheet = book.add_worksheet();
    for (col, name) in ["Issued", "Amount", "Status", "March total", "Paid invoices"]
        .into_iter()
        .enumerate()
    {
        sheet.write_string(0, col as u16, name).unwrap();
    }
    let timestamp = Format::new().set_num_format("yyyy-mm-dd hh:mm:ss");
    for (i, (when, amount, status)) in [
        ("2024-02-29T23:59:59", 900, "Paid"),
        ("2024-03-01T00:00:00", 240, "Paid"),
        ("2024-03-31T23:59:59", 60, "Paid"),
        ("2024-04-01T00:00:00", 1200, "Paid"),
        ("2024-03-10T12:00:00", 400, "Overdue"),
        ("2024-03-26", 700, "Paid"),
    ]
    .into_iter()
    .enumerate()
    {
        let row = i as u32 + 1;
        if i == 5 {
            sheet.write_string(row, 0, when).unwrap();
        } else {
            let date = ExcelDateTime::parse_from_str(when).unwrap();
            sheet
                .write_datetime_with_format(row, 0, &date, &timestamp)
                .unwrap();
        }
        sheet.write_number(row, 1, amount).unwrap();
        sheet.write_string(row, 2, status).unwrap();
    }
    sheet.write_number(1, 3, 0).unwrap();
    sheet.write_number(1, 4, 0).unwrap();
    book.save(&path).unwrap();

    let mut table = ExcelReader.read_file(&path).unwrap();
    assert!(matches!(table.get(2, 0), Some(CellValue::DateTime(_))));
    assert_eq!(
        table.get(5, 0),
        Some(&CellValue::String("2024-03-26".into()))
    );
    table.set(2, 1, CellValue::Int(65));
    let criteria = r#"A1:A6,">="&DATE(2024,3,1),A1:A6,"<"&DATE(2024,4,1),C1:C6,"Paid""#;
    assert_eq!(
        evaluate_formula(&format!("AVERAGEIFS(B1:B6,{criteria})"), &table),
        Some(152.5)
    );
    for (col, formula) in [
        (3, format!("=SUMIFS(B1:B6,{criteria})")),
        (4, format!("=COUNTIFS({criteria})")),
    ] {
        assert!(commit_cell_text(
            &mut table,
            0,
            col,
            formula,
            BinaryDisplayMode::default()
        ));
    }

    let report = dir.path().join("march-report.xlsx");
    ExcelReader.write_file(&report, &table).unwrap();
    let reopened = ExcelReader.read_file(&report).unwrap();
    assert_eq!(evaluate_formula("D1", &reopened), Some(305.0));
    assert_eq!(evaluate_formula("E1", &reopened), Some(2.0));
    assert_eq!(
        evaluate_formula(&format!("SUMIFS(B1:B6,{criteria})"), &reopened),
        Some(305.0)
    );
    assert_eq!(evaluate_formula("DAY(A3)", &reopened), Some(31.0));
    assert_eq!(evaluate_formula("DAY(A4)", &reopened), Some(1.0));
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
fn lookup_nearest_numeric_mode_is_directional_without_sorting() {
    let mut table = formula_table();
    table.set(0, 0, CellValue::Int(30));
    table.set(1, 0, CellValue::Int(10));
    table.set(2, 0, CellValue::Int(10));
    for (formula, expected) in [
        ("XMATCH(25,A1:A3,-1,1)", Some(2.0)),
        ("XMATCH(25,A1:A3,-1,-1)", Some(3.0)),
        ("XMATCH(25,A1:A3,1,-1)", Some(1.0)),
        ("XMATCH(10,A1:A3,1,-1)", Some(3.0)),
        ("XVERWEIS(25;A1:A3;B1:B3;;-1;-1)", Some(0.5)),
        ("XLOOKUP(35,A1:A3,B1:B3,42,1)", Some(42.0)),
        ("XLOOKUP(5,A1:A3,B1:B3,42,-1)", Some(42.0)),
        ("XLOOKUP(25,A1:A3,B1:B3,1/0,1)", Some(2.5)),
    ] {
        assert_eq!(evaluate_formula(formula, &table), expected, "{formula}");
    }
    table.set(2, 0, CellValue::String("not a threshold".into()));
    assert_eq!(evaluate_formula("XMATCH(5,A1:A3,1)", &table), Some(2.0));
}

#[test]
fn lookup_binary_modes_follow_sorted_numeric_ranges() {
    let mut table = formula_table();
    for (formula, expected) in [
        ("XMATCH(10,A1:A3,0,2)", Some(1.0)),
        ("XMATCH(20,A1:A3,0,2)", Some(2.0)),
        ("XMATCH(30,A1:A3,0,2)", Some(3.0)),
        ("XMATCH(23,A1:A3,-1,2)", Some(2.0)),
        ("XMATCH(23,A1:A3,1,2)", Some(3.0)),
        ("XMATCH(23,A1:A3,0,2)", None),
        ("XMATCH(-2,A1:A3,-1,2)", None),
        ("XMATCH(99,A1:A3,1,2)", None),
        ("XLOOKUP(23,A1:A3,B1:B3,99,-1,2)", Some(3.0)),
        ("XLOOKUP(23,A1:A3,B1:B3,99,0,2)", Some(99.0)),
    ] {
        assert_eq!(evaluate_formula(formula, &table), expected, "{formula}");
    }
    table.set(1, 0, CellValue::Int(10));
    assert_eq!(evaluate_formula("XMATCH(10,A1:A3,0,2)", &table), Some(1.0));
    assert_eq!(evaluate_formula("XMATCH(22,A1:A3,-1,2)", &table), Some(1.0));
    table.set(0, 2, CellValue::Int(-1));
    table.set(1, 2, CellValue::Int(40));
    for (formula, expected) in [
        ("XVERGLEICH(2.5;A1:C1;0;-2)", Some(2.0)),
        ("XMATCH(3,A1:C1,1,-2)", Some(1.0)),
        ("XMATCH(3,A1:C1,-1,-2)", Some(2.0)),
        ("XMATCH(-2,A1:C1,-1,-2)", None),
        ("XMATCH(99,A1:C1,1,-2)", None),
        ("XLOOKUP(-1,A1:C1,A2:C2,99,0,-2)", Some(40.0)),
    ] {
        assert_eq!(evaluate_formula(formula, &table), expected, "{formula}");
    }
}

#[test]
fn lookup_binary_retains_numeric_precision_and_loaded_bounds() {
    let mut table = formula_table();
    table.set(0, 0, CellValue::Float(9_007_199_254_740_992.0));
    table.set(1, 0, CellValue::Int(9_007_199_254_740_993));
    table.set(2, 0, CellValue::Int(9_007_199_254_740_994));
    table.set(0, 2, CellValue::Int(9_007_199_254_740_993));
    assert_eq!(
        evaluate_formula("XLOOKUP(C1,A1:A1000000,B1:B1000000,8,0,2)", &table),
        Some(3.0)
    );
    assert_eq!(
        evaluate_formula("XMATCH(9007199254740992,A1:A1000000,0,2)", &table),
        Some(1.0)
    );
    assert_eq!(evaluate_formula("XMATCH(0,D1:D1000000,-1,2)", &table), None);
}

#[test]
fn lookup_binary_does_not_apply_not_found_value_to_invalid_modes_or_keys() {
    let catalogue = lookup_catalog();
    for formula in [
        r#"XLOOKUP("ax-01",A1:A7,B1:B7,8,0,2)"#,
        r#"XLOOKUP("ax-*",A1:A7,B1:B7,8,2,2)"#,
        r#"XLOOKUP("ax-01",A1:A7,B1:B7,8,1,1)"#,
        "XLOOKUP(10,A1:A7,B1:B7,8,0,-2)",
        "XLOOKUP(10,A1:A7,B1:B6,8,0,2)",
    ] {
        assert_eq!(evaluate_formula(formula, &catalogue), None, "{formula}");
    }
    assert_eq!(
        evaluate_formula("IFERROR(XLOOKUP(10,A1:A7,B1:B7,8,0,-2),9)", &catalogue),
        Some(9.0)
    );
}

#[test]
fn lookup_binary_large_inventory_agrees_with_unindexed_lookup() {
    let mut table = formula_table();
    table.rows = (0..32_000)
        .map(|i| vec![CellValue::Int(10_000 + 2 * i), CellValue::Int(5 + i % 99)])
        .collect();
    table.columns.truncate(2);
    for key in [9_999, 10_000, 19_999, 27_458, 36_200, 73_998, 74_000] {
        for mode in [-1, 0, 1] {
            let lookup = |search| format!("XLOOKUP({key},A1:A32000,B1:B32000,-1,{mode},{search})");
            assert_eq!(
                evaluate_formula(&lookup(2), &table),
                evaluate_formula(&lookup(1), &table),
                "inventory SKU {key}, mode {mode}"
            );
        }
    }
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
fn index_reads_one_based_positions_in_rectangles_and_vectors() {
    let table = formula_table();
    for (formula, want) in [
        ("INDEX(A1:B3,2,2)", 3.0),
        ("INDEX(B3:A1,2,2)", 3.0),
        ("INDEX(B1:B3,2)", 3.0),
        ("INDEX(B1:B3,2,0)", 3.0),
        ("INDEX(A1:B1,2)", 2.5),
        ("INDEX(A1:B1,,2)", 2.5),
        ("INDEX(A1:B1,0,2)", 2.5),
        ("INDEX(A1,0,0)", 10.0),
        ("INDEX(A1:B3,XMATCH(20,A1:A3),2)+1", 4.0),
        ("INDEX(B1:B3,MATCH(20,A1:A3,0))", 3.0),
    ] {
        assert_eq!(evaluate_formula(formula, &table), Some(want), "{formula}");
    }
}

#[test]
fn match_returns_typed_exact_positions_and_resolves_wildcards() {
    let mut table = lookup_catalog();
    for (formula, want) in [
        (r#"MATCH("ax-01",A1:A7,0)"#, 1.0),
        (r#"MATCH("AX-??",A2:A7,0)"#, 1.0),
        (r#"VERGLEICH("AX-~*";$A$1:$A$7;0)"#, 4.0),
        (r#"MATCH("ÖKO",A1:A7,FALSE)"#, 5.0),
    ] {
        assert_eq!(evaluate_formula(formula, &table), Some(want), "{formula}");
    }
    table.set(0, 0, CellValue::String("1".into()));
    table.set(1, 0, CellValue::Int(1));
    table.set(2, 0, CellValue::Bool(true));
    assert_eq!(evaluate_formula(r#"MATCH("1",A1:A3,0)"#, &table), Some(1.0));
    assert_eq!(evaluate_formula("MATCH(1,A1:A3,0)", &table), Some(2.0));
    assert_eq!(evaluate_formula("MATCH(TRUE,A1:A3,0)", &table), Some(3.0));
}

#[test]
fn match_validates_classic_numeric_threshold_order_in_both_directions() {
    let mut table = formula_table();
    for (formula, want) in [
        ("MATCH(25,A1:A1000000)", Some(2.0)),
        ("MATCH(5,A1:A3,1)", None),
        ("MATCH(25,A1:A3,-1)", None),
        ("MATCH(500,A1:A3,1)", Some(3.0)),
        ("MATCH(20,A1:A3,1)", Some(2.0)),
    ] {
        assert_eq!(evaluate_formula(formula, &table), want, "{formula}");
    }
    table.set(0, 0, CellValue::Int(30));
    table.set(2, 0, CellValue::Int(10));
    assert_eq!(evaluate_formula("MATCH(23,A1:A3,-1)", &table), Some(1.0));
    assert_eq!(evaluate_formula("MATCH(15,A1:A3,-1)", &table), Some(2.0));
    assert_eq!(evaluate_formula("MATCH(31,A1:A3,-1)", &table), None);
    table.set(2, 0, CellValue::Int(25));
    assert_eq!(evaluate_formula("MATCH(30,A1:A3,-1)", &table), None);
}

#[test]
fn index_and_match_do_not_alias_offsheet_or_multicell_results() {
    let table = formula_table();
    for formula in [
        "INDEX(A1:B3,2,0)",
        "INDEX(A1:B3,2)",
        "INDEX(A1:B3,,2)",
        "INDEX(A1:B3,)",
        "INDEX(A1:B3,1.5,2)",
        "INDEX(A1:B10,8,2)",
        "INDEX(A1:B3,2,3)",
        "INDEX(A1:B3,2,2,1)",
        "MATCH(10,A1:B3,0)",
        "MATCH(10,D1:D1000000,0)",
        "MATCH(10,A1:A3,4)",
        "IFERROR(INDEX(A1:B3,1+,2),7)",
    ] {
        assert_eq!(evaluate_formula(formula, &table), None, "{formula}");
    }
    assert_eq!(
        evaluate_formula("IFERROR(INDEX(A1:B3,2,0),7)", &table),
        Some(7.0)
    );
    let bad = evaluate_formula_with_diagnostics("INDEX(A1:C3,1,3)", &table)
        .bad_cell
        .unwrap();
    assert_eq!((bad.row, bad.col, bad.content.as_str()), (0, 2, "hello"));
}

#[test]
fn index_match_turns_month_and_region_into_a_saved_xlsx_value() {
    use octa::formats::{FormatReader, csv_reader::CsvReader, excel_reader::ExcelReader};
    use octa::ui::table_view::commit_cell_text;

    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("rates.csv");
    std::fs::write(
        &source,
        "Region,Jan,Feb,Mar,Result\nMonth,January,February,March,0\nNorth,9.5,11.5,13.5,0\nWest,17.25,20.5,22.5,0\nEast,18,23.25,24,0\n",
    )
    .unwrap();
    let mut table = CsvReader.read_file(&source).unwrap();
    let formula =
        r#"=INDEX($B$2:$D$4;VERGLEICH("West";$A$2:$A$4;0);VERGLEICH("February";$B$1:$D$1;0))"#;
    assert!(commit_cell_text(
        &mut table,
        0,
        4,
        formula.into(),
        BinaryDisplayMode::default()
    ));
    assert_eq!(table.get(0, 4), Some(&CellValue::Float(20.5)));
    let report = dir.path().join("rates-report.xlsx");
    ExcelReader.write_file(&report, &table).unwrap();
    let reopened = ExcelReader.read_file(&report).unwrap();
    assert_eq!(reopened.get(0, 4), Some(&CellValue::Float(20.5)));
    assert_eq!(reopened.get(2, 2), Some(&CellValue::Float(20.5)));
    assert_eq!(reopened.get(2, 0), Some(&CellValue::String("West".into())));
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

#[test]
fn conditional_extrema_select_numbers_without_summing_or_inventing_zero() {
    use CellValue::{Bool, Float, Int, Null, String as Text};
    let mut table = DataTable {
        rows: vec![
            vec![Float(12.5), Text("west".into()), Text("Paid".into())],
            vec![
                Text(" -3.75 ".into()),
                Text("West".into()),
                Text("paid".into()),
            ],
            vec![
                Float(f64::INFINITY),
                Text("West".into()),
                Text("paid".into()),
            ],
            vec![Null, Text("West".into()), Text("paid".into())],
            vec![Bool(false), Text("West".into()), Text("paid".into())],
            vec![Int(99), Text("East".into()), Text("paid".into())],
            vec![Int(-11), Text("West".into()), Text("Unpaid".into())],
        ],
        ..formula_table()
    };
    for (formula, expected) in [
        (r#"MINIFS(A1:A7,B1:B7,"WEST",C1:C7,"p?ID")"#, -3.75),
        (r#"MAXIFS(A1:A7,B1:B7,"West",C1:C7,"Paid")"#, 12.5),
        (r#"MAXWENNS(A1:A7;B1:B7;"West";A1:A7;"<0")"#, -3.75),
        (r#"MINWENNS(A1:A7;B1:B7;"East")"#, 99.0),
        (r#"MINIFS(A1:A7,B1:B7,"North")"#, 0.0),
        (r#"MAXIFS(A3:A5,B3:B5,"West")"#, 0.0),
    ] {
        assert_eq!(
            evaluate_formula(formula, &table),
            Some(expected),
            "{formula}"
        );
    }
    table.set(0, 0, Float(1e308));
    table.set(1, 0, Float(1e308));
    for fn_name in ["MINIFS", "MAXIFS"] {
        let formula = format!(r#"{fn_name}(A1:A2,B1:B2,"West")"#);
        assert_eq!(evaluate_formula(&formula, &table), Some(1e308), "{formula}");
    }
}

#[test]
fn conditional_extrema_keep_relative_rectangles_and_check_loaded_bounds() {
    use CellValue::{Int, String as Text};
    let table = DataTable {
        rows: vec![
            vec![Int(-4), Int(8), Text("no".into())],
            vec![Int(6), Int(-3), Text("yes".into())],
            vec![Int(20), Int(30), Text("no".into())],
            vec![Int(40), Int(50), Text("yes".into())],
        ],
        row_offset: 1000,
        total_rows: Some(1_000_000),
        ..formula_table()
    };
    for (formula, value) in [
        (r#"MINIFS(B2:A1,C3:B2,"yes")"#, 8.0),
        (r#"MAXIFS(B3:B1,C4:C2,"yes")"#, 30.0),
        (r#"MINIFS(A1:A999999999999,C1:C999999999999,"yes")"#, 6.0),
        (r#"MAXIFS(A2:A10,C3:C11,"yes")"#, 20.0),
        (r#"MAXIFS(A1:A2,C900:C901,"")"#, 0.0),
        (r#"IFERROR(MINIFS(A1:A2,B1:C1,"yes"),42)"#, 42.0),
        (r#"IF(FALSE,MAXIFS(A1:A2,B1:C1,"yes"),7)"#, 7.0),
    ] {
        assert_eq!(evaluate_formula(formula, &table), Some(value), "{formula}");
    }
    for formula in [
        r#"MINIFS(A1:A2,B1:C1,"yes")"#,
        "MAXIFS(A1:A2)",
        "MAXIFS(A1:A2,B1:B2,)",
        r#"IF(FALSE,MINIFS(A1:A2,B1:B2,),7)"#,
    ] {
        assert_eq!(evaluate_formula(formula, &table), None, "{formula}");
    }
}

#[test]
fn conditional_formulas_compare_typed_clocks_and_unwrapped_durations() {
    use CellValue::{Duration, Int, String as Text, Time};
    use octa::data::times::{NANOS_PER_SEC, parse_time};
    let time = |clock| Time(parse_time(clock).unwrap());
    let elapsed = |h: i64| Duration(h * 3600 * NANOS_PER_SEC);
    let table = DataTable {
        rows: vec![
            vec![Int(10), time("18:00"), elapsed(27)],
            vec![Int(-5), time("19:00"), elapsed(8)],
            vec![Int(30), time("07:00"), elapsed(-2)],
            vec![Int(20), Text("18:00".into()), elapsed(27)],
            vec![Int(15), time("18:00"), elapsed(48)],
        ],
        ..formula_table()
    };
    for (formula, expected) in [
        ("COUNTIF(B1:B5,TIME(18,0,0))", 2.0),
        ("COUNTIF(B1:B5,B1)", 2.0),
        ("COUNTIF(C1:C5,C1)", 2.0),
        (r#"COUNTIF(B1:B5,">="&B1)"#, 3.0),
        (r#"COUNTIF(C1:C5,">"&C1)"#, 1.0),
        (r#"MAXIFS(A1:A5,B1:B5,">="&TIME(18,0,0),C1:C5,">1")"#, 15.0),
        (r#"MINIFS(A1:A5,B1:B5,">"&TIME(18,0,0))"#, -5.0),
        (r#"MINIFS(A1:A5,C1:C5,"<0")"#, 30.0),
        (r#"SUMIFS(A1:A5,B1:B5,">="&TIME(18,0,0),C1:C5,">1")"#, 25.0),
    ] {
        assert_eq!(
            evaluate_formula(formula, &table),
            Some(expected),
            "{formula}"
        );
    }
}

#[test]
fn evening_extrema_report_round_trips_xlsx_clocks_and_numeric_results() {
    use octa::data::times::parse_time;
    use octa::formats::{FormatReader, excel_reader::ExcelReader};
    use octa::ui::table_view::commit_cell_text;
    use rust_xlsxwriter::{Format, Workbook};

    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("shift-orders.xlsx");
    let mut book = Workbook::new();
    let sheet = book.add_worksheet().set_name("Orders").unwrap();
    for (col, label) in [
        "Region",
        "Clock",
        "Orders",
        "West evening min",
        "West evening max",
    ]
    .into_iter()
    .enumerate()
    {
        sheet.write_string(0, col as u16, label).unwrap();
    }
    let format = Format::new().set_num_format("hh:mm:ss");
    for (row, (region, hour, orders)) in [
        ("West", 7.0, 900),
        ("West", 19.25, 12),
        ("East", 20.0, 990),
        ("West", 23.0, 3),
        ("West", 20.25, 17),
    ]
    .into_iter()
    .enumerate()
    {
        let row = row as u32 + 1;
        sheet.write_string(row, 0, region).unwrap();
        sheet
            .write_number_with_format(row, 1, hour / 24.0, &format)
            .unwrap();
        sheet.write_number(row, 2, orders).unwrap();
    }
    sheet.write_number(1, 3, 0).unwrap();
    sheet.write_number(1, 4, 0).unwrap();
    book.save(&source).unwrap();

    let mut table = ExcelReader.read_file(&source).unwrap();
    for (col, name, expected) in [(3, "MINIFS", 3), (4, "MAXIFS", 17)] {
        let formula = format!(r#"={name}(C1:C5,A1:A5,"West",B1:B5,">="&TIME(18,0,0))"#);
        assert!(commit_cell_text(
            &mut table,
            0,
            col,
            formula,
            BinaryDisplayMode::default()
        ));
        assert_eq!(table.get(0, col), Some(&CellValue::Int(expected)));
    }
    let report = dir.path().join("west-evening.xlsx");
    ExcelReader.write_file(&report, &table).unwrap();
    let reopened = ExcelReader.read_file(&report).unwrap();
    assert_eq!(
        reopened.get(1, 1),
        Some(&CellValue::Time(parse_time("19:15").unwrap()))
    );
    assert_eq!(evaluate_formula("D1", &reopened), Some(3.0));
    assert_eq!(evaluate_formula("E1", &reopened), Some(17.0));
}

fn quantile_sample(values: impl IntoIterator<Item = CellValue>) -> DataTable {
    DataTable {
        rows: values.into_iter().map(|v| vec![v]).collect(),
        ..formula_table()
    }
}

#[test]
fn percentile_and_quartile_match_worked_excel_examples_and_legacy_names() {
    let table = quantile_sample([1, 3, 2, 4, 7, 8, 9, 10, 12].map(CellValue::Int));
    for (formula, expected) in [
        ("PERCENTILE.INC(A1:A4,0.3)", 1.9),
        ("quantil.inkl(A1:A4;30%)", 1.9),
        ("PERCENTILE(A1:A4,0.3)", 1.9),
        ("QUANTIL(A1:A4;30%)", 1.9),
        ("QUANTIL.EXKL(A1:A4;30%)", 1.5),
        ("PERCENTILE.EXC(A1:A4,1/5)", 1.0),
        ("PERCENTILE.EXC(A1:A4,4/5)", 4.0),
        ("QUARTILE.INC(A1:A4,0)", 1.0),
        ("QUARTILE.INKL(A1:A4;4)", 4.0),
        ("QUARTILE(A1:A4,2)", 2.5),
        ("QUARTILE.INKL(A1:A4;1.9)", 1.75),
        ("QUARTILE.EXC(A1:A4,1)", 1.25),
        ("QUARTILE.EXKL(A1:A4;3)", 3.75),
    ] {
        let got = evaluate_formula(formula, &table).unwrap();
        assert!(
            (got - expected).abs() < 1e-12,
            "{formula}: {got} != {expected}"
        );
    }
    let inclusive = quantile_sample([1, 2, 4, 7, 8, 9, 10, 12].map(CellValue::Int));
    assert_eq!(
        evaluate_formula("QUARTILE.INC(A1:A8,1)", &inclusive),
        Some(3.5)
    );
    let exclusive = quantile_sample([6, 7, 15, 36, 39, 40, 41, 42, 43, 47, 49].map(CellValue::Int));
    assert_eq!(
        evaluate_formula("QUARTILE.EXC(A1:A11,1)", &exclusive),
        Some(15.0)
    );
    assert_eq!(
        evaluate_formula("QUARTILE.EXC(A1:A11,3)", &exclusive),
        Some(43.0)
    );
}

#[test]
fn quantiles_follow_numeric_edits_and_only_loaded_cells_in_a_rectangle() {
    use CellValue::{Bool, Float, Int, Null, String as Text};
    let mut table = DataTable {
        rows: vec![
            vec![Int(30), Text(" 10 ".into()), Float(f64::NAN)],
            vec![Bool(true), Int(20), Text(" 40 ".into())],
            vec![Float(f64::INFINITY), Text("-inf".into()), Null],
        ],
        row_offset: 4000,
        total_rows: Some(900_000),
        ..formula_table()
    };
    table.set(0, 0, Int(50));
    assert_eq!(
        evaluate_formula("PERCENTILE.INC((ZZZ999999999:A1),25%)", &table),
        Some(17.5)
    );
    assert_eq!(
        evaluate_formula("QUARTILE.INC(A1:C3,2)", &table),
        Some(30.0)
    );
    assert_eq!(
        evaluate_formula("PERCENTILE.INC(B1,100%)", &table),
        Some(10.0)
    );
    assert_eq!(
        evaluate_formula("PERCENTILE.EXC(B1,50%)", &table),
        Some(10.0)
    );
    assert_eq!(
        evaluate_formula("PERCENTILE.INC(A4001:A4002,50%)", &table),
        None
    );
    assert_eq!(evaluate_formula("PERCENTILE.INC(A3:C3,50%)", &table), None);
}

#[test]
fn invalid_quantile_values_leave_no_result_and_lazy_branches_still_parse() {
    let table = quantile_sample([1, 2, 3, 4].map(CellValue::Int));
    for formula in [
        "PERCENTILE.INC(A1:A4)",
        "PERCENTILE.INC(A1:A4,0.5,0.9)",
        "PERCENTILE.INC(A1:A4,)",
        "PERCENTILE.INC(A1+A2,0.5)",
        "PERCENTILE.INC(A1:A4,-1%)",
        "PERCENTILE.INC(A1:A4,110%)",
        "PERCENTILE.INC(A1:A4,1E300*1E300)",
        r#"PERCENTILE.INC(A1:A4,"95%")"#,
        "PERCENTILE.EXC(A1:A4,0)",
        "PERCENTILE.EXC(A1:A4,100%)",
        "PERCENTILE.EXC(A1:A4,19%)",
        "PERCENTILE.EXC(A1:A4,81%)",
        "QUARTILE.EXC(A1:A4,0)",
        "QUARTILE.EXC(A1:A4,4)",
        "QUARTILE.EXC(A1:A2,1)",
        "QUARTILE.INC(A1:A4,5)",
        "QUARTILE.INC.EXTRA(A1:A4,1)",
        "IF(FALSE,PERCENTILE.INC(A1:A4),7)",
    ] {
        assert_eq!(evaluate_formula(formula, &table), None, "{formula}");
    }
    for formula in [
        "IF(FALSE,PERCENTILE.EXC(A1:A4,0),7)",
        "IFERROR(PERCENTILE.EXC(A1:A4,95%),7)",
    ] {
        assert_eq!(evaluate_formula(formula, &table), Some(7.0), "{formula}");
    }
    assert_eq!(
        evaluate_formula("1+PERCENTILE.INC(A1:A4,1/2)*2", &table),
        Some(6.0)
    );
}

#[test]
fn quantile_interpolation_keeps_extreme_finite_endpoints_finite() {
    let table = quantile_sample([f64::MAX, -f64::MAX].map(CellValue::Float));
    for (p, want) in [("0%", -f64::MAX), ("50%", 0.0), ("100%", f64::MAX)] {
        let formula = format!("PERCENTILE.INC(A1:A2,{p})");
        assert_eq!(evaluate_formula(&formula, &table), Some(want));
    }
    let near = f64::from_bits(f64::MAX.to_bits() - 2);
    let table = quantile_sample([f64::MAX, near, -near, -f64::MAX].map(CellValue::Float));
    for (p, lo, hi) in [("5%", -f64::MAX, -near), ("95%", near, f64::MAX)] {
        let formula = format!("PERCENTILE.INC(A1:A4,{p})");
        let got = evaluate_formula(&formula, &table).unwrap();
        assert!((lo..=hi).contains(&got), "{formula}: {got}");
    }
}

#[test]
fn quantile_selection_agrees_with_sorted_reference_on_unsorted_repeated_values() {
    for len in [1, 2, 3, 7, 16, 45, 127, 1023] {
        let data: Vec<f64> = (0..len)
            .map(|n| ((n * 211 + 73) % 97 - 50) as f64)
            .collect();
        let table = quantile_sample(data.iter().copied().map(CellValue::Float));
        let mut sorted = data;
        sorted.sort_unstable_by(f64::total_cmp);
        for percent in [0, 1, 5, 25, 35, 50, 75, 95, 99, 100] {
            let n = len as f64;
            for (method, rank) in [
                ("INC", (n - 1.0) * percent as f64 / 100.0),
                ("EXC", (n + 1.0) * percent as f64 / 100.0 - 1.0),
            ] {
                let formula = format!("PERCENTILE.{method}(A1:A{len},{percent}%)");
                let got = evaluate_formula(&formula, &table);
                if (0.0..n).contains(&rank) && rank.ceil() < n {
                    let lower = sorted[rank.floor() as usize];
                    let upper = sorted[rank.ceil() as usize];
                    let expected = lower + (upper - lower) * rank.fract();
                    assert!((got.unwrap() - expected).abs() < 1e-10, "{formula}");
                } else {
                    assert_eq!(got, None, "{formula}");
                }
            }
        }
    }
}

#[test]
fn latency_percentile_report_round_trips_numeric_results_through_xlsx() {
    use octa::formats::{FormatReader, excel_reader::ExcelReader};
    use octa::ui::table_view::commit_cell_text;
    use rust_xlsxwriter::Workbook;

    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("latency-ms.xlsx");
    let mut book = Workbook::new();
    let sheet = book.add_worksheet().set_name("Latency").unwrap();
    for (col, label) in ["Response ms", "P95 ms", "Upper quartile ms"]
        .into_iter()
        .enumerate()
    {
        sheet.write_string(0, col as u16, label).unwrap();
    }
    for (r, sample) in [100, 120, 160, 200, 300].into_iter().enumerate() {
        sheet.write_number(r as u32 + 1, 0, sample).unwrap();
    }
    sheet.write_number(1, 1, 0).unwrap();
    sheet.write_number(1, 2, 0).unwrap();
    book.save(&source).unwrap();

    let mut table = ExcelReader.read_file(&source).unwrap();
    for (col, formula) in [
        (1, "=QUANTIL.INKL(A1:A5;95%)"),
        (2, "=QUARTILE.EXC(A1:A5,3)"),
    ] {
        assert!(commit_cell_text(
            &mut table,
            0,
            col,
            formula.into(),
            BinaryDisplayMode::default()
        ));
    }
    let report = dir.path().join("latency-report.xlsx");
    ExcelReader.write_file(&report, &table).unwrap();
    let reopened = ExcelReader.read_file(&report).unwrap();
    for (col, expected) in [(1, 280.0), (2, 250.0)] {
        let actual = match reopened.get(0, col).unwrap() {
            CellValue::Float(v) => *v,
            CellValue::Int(v) => *v as f64,
            other => panic!("wanted a numeric XLSX cell, got {other:?}"),
        };
        assert!((actual - expected).abs() < 1e-10, "{actual} != {expected}");
    }
}

#[test]
fn dispersion_matches_excel_strength_examples_and_german_aliases() {
    let table = quantile_sample(
        [1345, 1301, 1368, 1322, 1310, 1370, 1318, 1350, 1303, 1299].map(CellValue::Int),
    );
    for (expected, aliases) in [
        (27.46391572, ["STDEV.S", "STABW.S", "STDEV", "STABW"]),
        (
            26.054558142482477,
            ["STDEV.P", "STABW.N", "STDEVP", "STABWN"],
        ),
        (754.2666666666667, ["VAR.S", "VAR.S", "VAR", "VARIANZ"]),
        (678.84, ["VAR.P", "VAR.P", "VARP", "VARIANZEN"]),
    ] {
        for name in aliases {
            let formula = format!("{}(A1:A10)", name.to_ascii_lowercase());
            let actual = evaluate_formula(&formula, &table).unwrap();
            assert!(
                (actual - expected).abs() < 1e-8,
                "{formula}: {actual} != {expected}"
            );
        }
    }
}

#[test]
fn dispersion_combines_loaded_ranges_and_scalar_numbers_without_counting_blanks() {
    use CellValue::{Bool, Float, Int, Null, String as Text};
    let mut table = DataTable {
        rows: vec![
            vec![Int(2), Text(" 4 ".into()), Bool(true)],
            vec![Float(f64::NAN), Null, Text("n/a".into())],
            vec![Int(6), Null, Float(f64::INFINITY)],
        ],
        row_offset: 1500,
        total_rows: Some(1_000_000),
        ..formula_table()
    };
    table.set(2, 0, Int(8));
    for (formula, expected) in [
        ("VAR.S($A$3:A1;B1:B2;2*5)", 40.0 / 3.0),
        ("VAR.P(ZZ999999999:A1,10)", 10.0),
        ("VAR.P(A1:A999999999)", 9.0),
        (r#"STDEV.P(A1,"4",TRUE,B2,C1)"#, (14.0f64 / 9.0).sqrt()),
        ("STABW.N(B1)", 0.0),
        ("VAR.P(1/2,1.5)", 0.25),
    ] {
        let actual = evaluate_formula(formula, &table).unwrap();
        assert!(
            (actual - expected).abs() < 1e-12,
            "{formula}: {actual} != {expected}"
        );
    }
    assert_eq!(evaluate_formula("STDEV.S(B1)", &table), None);
    assert_eq!(evaluate_formula("VAR.P(A1501:A1503)", &table), None);
    assert_eq!(evaluate_formula("VAR.P(C1:C3)", &table), None);
}

#[test]
fn dispersion_errors_do_not_execute_in_an_inactive_branch() {
    let table = quantile_sample([1, 4].map(CellValue::Int));
    for formula in [
        "VAR.S()",
        "VAR.S(A1,)",
        "VAR.S(A1:A2: A2)",
        r#"VAR.S("bad",2)"#,
        r#"VAR.P("NaN",1)"#,
        "VAR.P(1E300*1E300,1)",
        "IF(FALSE,STDEV.S(A1,),7)",
    ] {
        assert_eq!(evaluate_formula(formula, &table), None, "{formula}");
    }
    for formula in [
        "IF(FALSE,STDEV.S(A1),7)",
        r#"IF(FALSE,VAR.S("bad",2),7)"#,
        "IFERROR(VAR.S(A9:A10),7)",
    ] {
        assert_eq!(evaluate_formula(formula, &table), Some(7.0), "{formula}");
    }
}

#[test]
fn dispersion_keeps_nearby_large_readings_and_finite_extreme_deviations() {
    let base = 2.0_f64.powi(48);
    let table =
        quantile_sample([base + 1.0, base + 3.0, base + 5.0, base + 7.0].map(CellValue::Float));
    let sample_variance = evaluate_formula("VAR.S(A1:A4)", &table).unwrap();
    assert!((sample_variance - 20.0 / 3.0).abs() < 1e-12);
    assert_eq!(evaluate_formula("VAR.P(A1,A1)", &table), Some(0.0));

    let tiny = f64::from_bits(1);
    let table = quantile_sample([0.0, tiny * 4.0].map(CellValue::Float));
    assert_eq!(evaluate_formula("STDEV.P(A1:A2)", &table), Some(tiny * 2.0));

    for numbers in [[0.0, f64::MAX, -f64::MAX], [-f64::MAX, 0.0, f64::MAX]] {
        let table = quantile_sample(numbers.map(CellValue::Float));
        let got = evaluate_formula("STDEV.P(A1:A3)", &table).unwrap();
        assert!((got / f64::MAX - (2.0f64 / 3.0).sqrt()).abs() < 1e-15);
        assert_eq!(evaluate_formula("VAR.P(A1:A3)", &table), None);
    }

    let table = quantile_sample([-f64::MAX, f64::MAX].map(CellValue::Float));
    assert_eq!(evaluate_formula("STDEV.P(A1:A2)", &table), Some(f64::MAX));
    assert_eq!(evaluate_formula("STDEV.S(A1:A2)", &table), None);
}

#[test]
fn dispersion_matches_a_two_pass_reference_across_orders_and_offsets() {
    for len in [2, 3, 11, 93, 1023] {
        let mut residuals: Vec<f64> = (0..len)
            .map(|i| ((i * 1111 + 89) % 1297 - 601) as f64 / 8.0)
            .collect();
        let mean = residuals.iter().sum::<f64>() / f64::from(len);
        let sum_of_squares = residuals.iter().map(|x| (x - mean).powi(2)).sum::<f64>();
        let expected = [
            ("VAR.P", sum_of_squares / f64::from(len)),
            ("STDEV.S", (sum_of_squares / f64::from(len - 1)).sqrt()),
        ];
        for origin in [0.0, 2.0_f64.powi(43), -(2.0_f64.powi(43))] {
            let values = residuals.iter().map(|x| CellValue::Float(origin + x));
            let table = quantile_sample(values);
            for (kind, want) in expected {
                let formula = format!("{kind}(A1:A{len})");
                let got = evaluate_formula(&formula, &table).unwrap();
                assert!(
                    (got - want).abs() < want.abs() * 1e-12,
                    "{formula} origin={origin}, {got} != {want}"
                );
            }
            residuals.reverse();
        }
    }
}

#[test]
fn production_spread_report_reopens_as_numeric_xlsx_cells() {
    use octa::formats::{FormatReader, excel_reader::ExcelReader};
    use octa::ui::table_view::commit_cell_text;
    use rust_xlsxwriter::Workbook;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gauge-readings.xlsx");
    let mut book = Workbook::new();
    let sheet = book.add_worksheet().set_name("Gauge").unwrap();
    for (c, title) in ["Reading", "Sample SD", "Population variance"]
        .into_iter()
        .enumerate()
    {
        sheet.write_string(0, c as u16, title).unwrap();
    }
    for (r, n) in [96, 100, 104].into_iter().enumerate() {
        sheet.write_number(r as u32 + 1, 0, n).unwrap();
    }
    book.save(&path).unwrap();
    let mut table = ExcelReader.read_file(&path).unwrap();
    for (c, formula) in [(1, "=STABW.S(A1:A3)"), (2, "=VAR.P(A1:A3)")] {
        assert!(commit_cell_text(
            &mut table,
            0,
            c,
            formula.into(),
            BinaryDisplayMode::default()
        ));
    }
    let report = dir.path().join("gauge-spread.xlsx");
    ExcelReader.write_file(&report, &table).unwrap();
    let table = ExcelReader.read_file(&report).unwrap();
    for (c, want) in [(1, 4.0), (2, 32.0 / 3.0)] {
        let actual = match table.get(0, c).unwrap() {
            CellValue::Float(v) => *v,
            CellValue::Int(v) => *v as f64,
            other => panic!("expected numeric XLSX spread, got {other:?}"),
        };
        assert!((actual - want).abs() < 1e-12, "{c}: {actual} != {want}");
    }
}

#[test]
fn numbervalue_matches_documented_examples_and_fixed_separator_defaults() {
    let table = formula_table();
    for (formula, expected) in [
        (r#"NUMBERVALUE("2.500,27",",",".")"#, 2500.27),
        (r#"ZAHLENWERT("2,500.27";".";",")"#, 2500.27),
        (r#"NUMBERVALUE("2.500,27",",decimal",".group")"#, 2500.27),
        (r#"NUMBERVALUE("2,500.27")"#, 2500.27),
        (r#"ZAHLENWERT("2.500,27";",")"#, 2500.27),
        (r#"NUMBERVALUE("2,500.27",,",")"#, 2500.27),
        (r#"NUMBERVALUE("3.5%")"#, 0.035),
        (r#"ZAHLENWERT("3,5%";",";".")"#, 0.035),
        (r#"NUMBERVALUE("9%%")"#, 0.0009),
        (r#"NUMBERVALUE(" 3 000 ")"#, 3000.0),
        (r#"NUMBERVALUE("1 234,50",","," ")"#, 1234.5),
        (r#"NUMBERVALUE("-5·25","·","")"#, -5.25),
        (r#"NUMBERVALUE("")"#, 0.0),
        (r#"NUMBERVALUE("   ")"#, 0.0),
    ] {
        assert_eq!(
            evaluate_formula(formula, &table),
            Some(expected),
            "{formula}"
        );
    }
}

#[test]
fn numbervalue_uses_edited_separator_cells_and_pinpoints_bad_input() {
    use CellValue::{Float, Int, Null, String as Text};
    let mut table = DataTable {
        rows: vec![
            vec![Text("1.234,50".into()), Text(",".into()), Text(".".into())],
            vec![Text("1,2.34".into()), Null, Null],
            vec![Int(13), Null, Null],
            vec![Null, Null, Null],
            vec![Float(f64::INFINITY), Null, Null],
        ],
        row_offset: 7000,
        ..formula_table()
    };
    table.set(0, 0, Text("3.027,50".into()));
    assert_eq!(
        evaluate_formula("NUMBERVALUE(A1,$B$1,$C$1)", &table),
        Some(3027.5)
    );
    assert_eq!(
        evaluate_formula("NUMBERVALUE((A3),B1,C1)", &table),
        Some(13.0)
    );
    assert_eq!(evaluate_formula("NUMBERVALUE(A4,B1,C1)", &table), Some(0.0));
    assert_eq!(
        evaluate_formula("NUMBERVALUE(11+3,B1,C1)", &table),
        Some(14.0)
    );
    let invalid = evaluate_formula_with_diagnostics("NUMBERVALUE(A2,B1,C1)", &table);
    assert!(invalid.value.is_none());
    let offender = invalid.bad_cell.unwrap();
    assert_eq!(
        (offender.row, offender.col, offender.content.as_str()),
        (1, 0, "1,2.34")
    );
    assert_eq!(evaluate_formula("NUMBERVALUE(A5)", &table), None);
    assert_eq!(evaluate_formula("NUMBERVALUE(A7001)", &table), None);
    let bad_option = evaluate_formula_with_diagnostics("NUMBERVALUE(A1,A3,C1)", &table);
    assert!(bad_option.value.is_none() && bad_option.bad_cell.is_none());
}

#[test]
fn numbervalue_rejects_ambiguous_text_and_parses_lazy_branches_without_reading_them() {
    let table = formula_table();
    for formula in [
        "NUMBERVALUE()",
        r#"NUMBERVALUE("2,2,4", ",", ".")"#,
        r#"NUMBERVALUE("1,2.3", ",", ".")"#,
        r#"NUMBERVALUE("1.2,3")"#,
        r#"NUMBERVALUE("1,234", ".", "")"#,
        r#"NUMBERVALUE("12", ",", ",")"#,
        r#"NUMBERVALUE("12", "", ".")"#,
        r#"NUMBERVALUE("12", " ", ".")"#,
        r#"NUMBERVALUE("12", 5, ".")"#,
        r#"NUMBERVALUE("12%", ".", "%")"#,
        r#"NUMBERVALUE("12", ".", ",", 3)"#,
        r#"NUMBERVALUE("EUR 12.5")"#,
        r#"NUMBERVALUE("9%7")"#,
        r#"NUMBERVALUE("1,2e3,4", ",", ".")"#,
        r#"NUMBERVALUE("1.2e3,4")"#,
        r#"NUMBERVALUE("1e")"#,
        r#"NUMBERVALUE("NaN")"#,
        r#"NUMBERVALUE("Infinity")"#,
        r#"NUMBERVALUE("...")"#,
        r#"NUMBERVALUE("%")"#,
        r#"IF(FALSE,NUMBERVALUE("12",1+1),7)"#,
    ] {
        assert_eq!(evaluate_formula(formula, &table), None, "{formula}");
    }
    for formula in [
        r#"IF(FALSE,ZAHLENWERT("EUR 12";",";"."),7)"#,
        r#"IFERROR(NUMBERVALUE("12x"),7)"#,
        "IF(FALSE,NUMBERVALUE(A999,C999,B999),7)",
    ] {
        assert_eq!(evaluate_formula(formula, &table), Some(7.0), "{formula}");
    }
}

#[test]
fn numbervalue_applies_percent_scaling_before_the_decimal_conversion() {
    let table = formula_table();
    for (formula, expected) in [
        (r#"NUMBERVALUE("1.20e3%")"#, 12.0),
        (r#"NUMBERVALUE("-1,50e+2%%",",",".")"#, -0.015),
        (r#"NUMBERVALUE("1e310%")"#, 1e308),
        (r#"NUMBERVALUE("1e-310%")"#, 1e-312),
        (r#"NUMBERVALUE("0e1000%%")"#, 0.0),
    ] {
        assert_eq!(
            evaluate_formula(formula, &table),
            Some(expected),
            "{formula}"
        );
    }
    assert_eq!(evaluate_formula(r#"NUMBERVALUE("1e400%")"#, &table), None);
    assert_eq!(evaluate_formula(r#"NUMBERVALUE("1e+%")"#, &table), None);
}

#[test]
fn numbervalue_supplier_amounts_remain_numeric_through_xlsx() {
    use octa::formats::{FormatReader, excel_reader::ExcelReader};
    use octa::ui::table_view::commit_cell_text;
    use rust_xlsxwriter::Workbook;
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("supplier-import.xlsx");
    let mut book = Workbook::new();
    let sheet = book.add_worksheet();
    for (c, label) in ["Original amount", "Numeric amount"]
        .into_iter()
        .enumerate()
    {
        sheet.write_string(0, c as u16, label).unwrap();
    }
    let inputs = ["1.250,50", "-75,25", "9%%"];
    for (r, input) in inputs.into_iter().enumerate() {
        sheet.write_string(r as u32 + 1, 0, input).unwrap();
        sheet.write_number(r as u32 + 1, 1, 0).unwrap();
    }
    book.save(&source).unwrap();
    let mut table = ExcelReader.read_file(&source).unwrap();
    for r in 0..3 {
        let formula = format!(r#"=ZAHLENWERT(A{};",";".")"#, r + 1);
        assert!(commit_cell_text(
            &mut table,
            r,
            1,
            formula,
            BinaryDisplayMode::default()
        ));
    }
    let output = dir.path().join("supplier-amounts.xlsx");
    ExcelReader.write_file(&output, &table).unwrap();
    let table = ExcelReader.read_file(&output).unwrap();
    for (r, want) in [1250.50, -75.25, 0.0009].into_iter().enumerate() {
        assert_eq!(table.get(r, 0), Some(&CellValue::String(inputs[r].into())));
        assert_eq!(table.get(r, 1), Some(&CellValue::Float(want)));
    }
    assert!((evaluate_formula("SUM(B1:B3)", &table).unwrap() - 1175.2509).abs() < 1e-12);
}

#[test]
fn paired_statistics_align_rows_before_ignoring_non_numbers() {
    use CellValue::{Bool, Float, Int, Null, String as Text};
    let table = DataTable {
        rows: vec![
            vec![Int(0), Int(1)],
            vec![Null, Int(-9000)],
            vec![Int(1000), Text("9".into())],
            vec![Int(1), Int(3)],
            vec![Bool(true), Int(9000)],
            vec![Int(2), Int(5)],
            vec![Int(8000), Float(f64::INFINITY)],
        ],
        ..formula_table()
    };
    for (formula, want) in [
        ("CORREL(A1:A7,B1:B7)", 1.0),
        ("korrel(B7:B1;A7:A1)", 1.0),
        ("PEARSON((A1:A7),B1:B7)", 1.0),
        ("SLOPE(B1:B7,A1:A7)", 2.0),
        ("STEIGUNG(A1:A7;B1:B7)", 0.5),
        ("COVARIANCE.P(A1:A7,B1:B7)", 4.0 / 3.0),
        ("COVARIANCE.S(A1:A7,B1:B7)", 2.0),
        ("KOVARIANZ.P(A1:A7;B1:B7)", 4.0 / 3.0),
        ("KOVARIANZ.S(B1:B7;A1:A7)", 2.0),
        ("KOVAR(A1:A7;B1:B7)", 4.0 / 3.0),
        ("COVAR(A1:A7,B1:B7)", 4.0 / 3.0),
    ] {
        let outcome = evaluate_formula_with_diagnostics(formula, &table);
        assert!(outcome.bad_cell.is_none(), "{formula}");
        assert!(
            (outcome.value.unwrap() - want).abs() < 5e-14,
            "{formula}: {outcome:?}"
        );
    }

    let table = DataTable {
        rows: vec![
            vec![Int(0), Int(1)],
            vec![Int(2), Int(3)],
            vec![Int(8), Int(5)],
        ],
        ..formula_table()
    };
    // The equal rectangles live at different offsets. Their relative
    // positions, not absolute columns, determine their pairs.
    let value = evaluate_formula("CORREL(A1:B2,A2:B3)", &table).unwrap();
    assert!((value - 0.6831300510639733).abs() < 1e-14, "{value}");
}

#[test]
fn paired_statistics_are_stable_with_large_offsets_and_scales() {
    use CellValue::Float;
    let table = DataTable {
        rows: (0..4)
            .map(|n| vec![Float(1e15 + n as f64), Float(-1e15 - 2.0 * n as f64)])
            .collect(),
        ..formula_table()
    };
    for (formula, want) in [
        ("SLOPE(B1:B4,A1:A4)", -2.0),
        ("CORREL(A1:A4,B1:B4)", -1.0),
        ("COVARIANCE.P(A1:A4,B1:B4)", -2.5),
    ] {
        let got = evaluate_formula(formula, &table).unwrap();
        assert!((got - want).abs() < 1e-14, "{formula}: {got}");
    }

    let mut table = table;
    table.rows = vec![
        vec![Float(-1e308), Float(1e308), Float(1e-307)],
        vec![Float(1e308), Float(-1e308), Float(-1e-307)],
    ];
    assert!((evaluate_formula("CORREL(A1:A2,B1:B2)", &table).unwrap() + 1.0).abs() < 1e-14);
    assert!((evaluate_formula("SLOPE(A1:A2,B1:B2)", &table).unwrap() + 1.0).abs() < 1e-14);
    assert!((evaluate_formula("COVARIANCE.P(A1:A2,C1:C2)", &table).unwrap() + 10.0).abs() < 1e-13);
    assert_eq!(evaluate_formula("COVARIANCE.P(A1:A2,B1:B2)", &table), None);
}

#[test]
fn paired_statistics_skip_unloaded_tails_and_separate_syntax_from_runtime() {
    let mut table = formula_table();
    table.row_offset = 2000;
    table.total_rows = Some(9_999_999_999);
    let base = evaluate_formula("CORREL(A1:A3,B1:B3)", &table);
    assert!(base.is_some());
    assert_eq!(
        evaluate_formula("CORREL(A1:A9999999999,B1:B9999999999)", &table),
        base
    );
    assert_eq!(evaluate_formula("CORREL(A1:A3,B1:B2)", &table), None);
    assert_eq!(evaluate_formula("CORREL(A1:A3,A1:C1)", &table), None);
    assert_eq!(
        evaluate_formula("IFERROR(CORREL(A1:A3,B1:B2),17)", &table),
        Some(17.0)
    );
    assert_eq!(
        evaluate_formula("IF(0,SLOPE(A1:A99,B2:B7),17)", &table),
        Some(17.0)
    );
    assert_eq!(evaluate_formula("IF(0,SLOPE(7,B2:B7),17)", &table), None);
    assert_eq!(evaluate_formula("CORREL(A1:A3,B1:B3,17)", &table), None);
}

#[test]
fn paired_statistics_handle_constant_series_and_the_sample_boundary() {
    use CellValue::{Int, Null};
    let table = DataTable {
        rows: vec![
            vec![Int(7), Int(0)],
            vec![Int(7), Int(2)],
            vec![Null, Int(3)],
        ],
        ..formula_table()
    };
    for (formula, expected) in [
        ("CORREL(A1:A3,B1:B3)", None),
        ("SLOPE(B1:B3,A1:A3)", None),
        ("SLOPE(A1:A3,B1:B3)", Some(0.0)),
        ("COVARIANCE.S(A1:A3,B1:B3)", Some(0.0)),
        ("COVARIANCE.P(A1,B1)", Some(0.0)),
        ("COVARIANCE.S(A1,B1)", None),
        ("COVARIANCE.P(A3,B3)", None),
    ] {
        assert_eq!(evaluate_formula(formula, &table), expected, "{formula}");
    }
}

#[test]
fn paired_statistics_remain_numeric_after_xlsx_reopen() {
    use octa::formats::{FormatReader, excel_reader::ExcelReader};
    use octa::ui::table_view::commit_cell_text;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("paired-sensors.xlsx");
    let mut table = DataTable {
        rows: [0, 1, 2, 3]
            .map(|x| {
                vec![
                    CellValue::Int(x),
                    CellValue::Int(2 * x + 1),
                    CellValue::Null,
                ]
            })
            .to_vec(),
        ..formula_table()
    };
    table.columns[2].data_type = "Float64".into();
    for (row, formula) in [
        "=KORREL(A1:A4;B1:B4)",
        "=SLOPE(B1:B4,A1:A4)",
        "=COVARIANCE.P(A1:A4,B1:B4)",
    ]
    .into_iter()
    .enumerate()
    {
        assert!(commit_cell_text(
            &mut table,
            row,
            2,
            formula.into(),
            BinaryDisplayMode::default()
        ));
    }
    ExcelReader.write_file(&path, &table).unwrap();
    let read = ExcelReader.read_file(&path).unwrap();
    for (row, want) in [1.0, 2.0, 2.5].into_iter().enumerate() {
        let Some(CellValue::Float(value)) = read.get(row, 2) else {
            panic!("expected a numeric XLSX value after re-opening {row}");
        };
        assert!((value - want).abs() < 1e-14, "{value}");
    }
}

#[test]
fn month_formulas_clamp_signed_offsets_and_distinguish_the_last_day() {
    let table = formula_table();
    for (expr, expected) in [
        (r#"EDATE("2024-01-31",1)"#, 45_351.0),
        (r#"edatum("2023-01-31";1.9)"#, 44_985.0),
        (r#"EDATE("2024-03-31",-1.9)"#, 45_351.0),
        (r#"EDATUM("2024-12-15";-0.9)"#, 45_641.0),
        (r#"EDATE("2024-12-31",1)"#, 45_688.0),
        ("EDATE(DATE(2024,3,31),1)", 45_412.0),
        (r#"EDATE("2023-06-18",1)"#, 45_125.0),
        (r#"EOMONTH("2024-02-02",0)"#, 45_351.0),
        (r#"MONATSENDE("2024-12-06";-1.2)"#, 45_626.0),
        (r#"EOMONTH("2024-02-29",1)"#, 45_382.0),
        ("EOMONTH(45354.8,0)", 45_382.0),
        ("DAY(EDATE(45351,12))", 28.0),
    ] {
        assert_eq!(evaluate_formula(expr, &table), Some(expected), "{expr}");
    }
}

#[test]
fn month_formulas_follow_gregorian_bounds_and_lazy_branch_rules() {
    let table = formula_table();
    for (expr, expected) in [
        ("EOMONTH(1,1)", Some(59.0)),
        ("EDATE(59,1)", Some(88.0)),
        ("EOMONTH(2958465,0)", Some(2_958_465.0)),
        (r#"MONATSENDE("9999-11-01";1)"#, Some(2_958_465.0)),
        ("EDATE(60,1)", None),
        ("EOMONTH(60.9,1)", None),
        ("EOMONTH(1,-1)", None),
        ("EDATE(2958465,1)", None),
        ("EDATE(1,4294967296)", None),
        ("EOMONTH(1,1E80)", None),
        ("EOMONTH(1)", None),
        ("EDATE(1,2,3)", None),
        ("IF(FALSE,EDATE(60,1E80),7)", Some(7.0)),
        ("IF(FALSE,EOMONTH(1,2,),7)", None),
        ("IFERROR(MONATSENDE(60,0),99)", Some(99.0)),
    ] {
        assert_eq!(evaluate_formula(expr, &table), expected, "{expr}");
    }
}

#[test]
fn iso_week_follows_the_week_year_across_january_and_december() {
    let mut table = formula_table();
    table.set(
        0,
        0,
        CellValue::DateTime("2018-12-31T23:45:00-03:00".into()),
    );
    table.set(0, 1, CellValue::String("2021-01-03".into()));
    for (expr, result) in [
        ("ISOWEEKNUM(A1)", Some(1.0)),
        ("ISOKALENDERWOCHE(B1)", Some(53.0)),
        (r#"ISOWEEKNUM("2015-12-31")"#, Some(53.0)),
        (r#"ISOWEEKNUM("2016-01-01")"#, Some(53.0)),
        (r#"isokalenderwoche("2021-01-04")"#, Some(1.0)),
        ("ISOWEEKNUM(DATE(2023,1,1))", Some(52.0)),
        ("ISOWEEKNUM(1)", Some(1.0)),
        ("ISOWEEKNUM(60)", None),
        ("ISOWEEKNUM(1,2)", None),
        ("ISOWEEKNUM(1,)", None),
    ] {
        assert_eq!(evaluate_formula(expr, &table), result, "{expr}");
    }
}

#[test]
fn month_formulas_accept_typed_xlsx_dates_and_keep_date_diagnostics() {
    use octa::formats::{FormatReader, excel_reader::ExcelReader};
    use octa::ui::table_view::commit_cell_text;
    use rust_xlsxwriter::{ExcelDateTime, Format, Workbook};

    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("statements.xlsx");
    let mut book = Workbook::new();
    let sheet = book.add_worksheet();
    sheet.write_string(0, 0, "Date").unwrap();
    sheet.write_string(0, 1, "Month").unwrap();
    let date = ExcelDateTime::parse_from_str("2024-01-31").unwrap();
    sheet
        .write_datetime_with_format(1, 0, &date, &Format::new().set_num_format("yyyy-mm-dd"))
        .unwrap();
    sheet.write_string(2, 0, "31/01/2024").unwrap();
    sheet.write_number(1, 1, 0).unwrap();
    book.save(&source).unwrap();
    let original = std::fs::read(&source).unwrap();
    let mut table = ExcelReader.read_file(&source).unwrap();
    assert!(matches!(table.get(0, 0), Some(CellValue::DateTime(_))));
    assert_eq!(evaluate_formula("EDATE(A1,1)", &table), Some(45_351.0));
    assert_eq!(evaluate_formula("EOMONTH(A1,1)", &table), Some(45_351.0));
    let outcome = evaluate_formula_with_diagnostics("MONATSENDE(A2;1)", &table);
    assert_eq!(outcome.value, None);
    let bad = outcome.bad_cell.unwrap();
    assert_eq!(
        (bad.row, bad.col, bad.content.as_str()),
        (1, 0, "31/01/2024")
    );
    assert!(commit_cell_text(
        &mut table,
        0,
        1,
        "=MONTH(EOMONTH(A1,1))".into(),
        BinaryDisplayMode::default()
    ));
    assert_eq!(table.get(0, 1), Some(&CellValue::Int(2)));
    assert_eq!(std::fs::read(source).unwrap(), original);
}
