use super::*;
use octa::data::{CellValue, ColumnInfo, DataTable, evaluate_formula};

#[test]
fn every_example_evaluates_in_both_languages_with_wide_column_references() {
    let table = DataTable {
        columns: (0..28)
            .map(|i| ColumnInfo {
                name: format!("Series {i}"),
                data_type: "Int64".into(),
            })
            .collect(),
        rows: [45_000, 45_008, 45_021]
            .map(|n| vec![CellValue::Int(n); 28])
            .to_vec(),
        ..DataTable::empty()
    };
    assert_eq!(letters(0), "A");
    assert_eq!(letters(25), "Z");
    assert_eq!(letters(26), "AA");
    assert_eq!(letters(701), "ZZ");
    assert_eq!(letters(702), "AAA");
    let mut payments = table.clone();
    payments.rows[0][26] = CellValue::Int(-45_000);
    for &recipe in RECIPES {
        let input = if matches!(recipe.form, Form::DatedValue) {
            &payments
        } else {
            &table
        };
        for german in [false, true] {
            let formula = recipe
                .expression(26, Some(27), table.row_count(), german)
                .unwrap();
            let expr = formula.strip_prefix('=').unwrap();
            let value = evaluate_formula(expr, input);
            if matches!(recipe.form, Form::Running) && recipe.english == "STDEV.S" {
                // There is no sample deviation on row one.
                assert_eq!(value, None);
                let second = expr.replace(":AA1)", ":AA2)");
                assert!(evaluate_formula(&second, &table).is_some(), "{second}");
            } else {
                assert!(value.is_some(), "{formula}");
            }
            assert!(expr.contains("AA"), "{formula}");
        }
    }
}

#[test]
fn search_accepts_english_german_and_formula_punctuation() {
    for q in [
        "QUANTIL.INKL",
        "percentile",
        " =percentile.inc ",
        "quantil inkl",
    ] {
        let hits = matching(q);
        assert_eq!(hits.len(), 1, "{q}");
        assert_eq!(hits[0].english, "PERCENTILE.INC");
        assert_eq!(
            hits[0].expression(27, None, 105, true).unwrap(),
            "=QUANTIL.INKL($AB$1:$AB$105;95%)"
        );
    }
    assert!(matching("unregistered fx").is_empty());
    assert_eq!(matching("").len(), RECIPES.len());
}

#[test]
fn ranking_recipes_pin_the_sample_and_offer_both_language_names() {
    let table = DataTable {
        columns: vec![ColumnInfo {
            name: "Score".into(),
            data_type: "Float64".into(),
        }],
        rows: [75, 90, 75, 60].map(|n| vec![CellValue::Int(n)]).to_vec(),
        ..DataTable::empty()
    };
    for (query, german, function, draft, expected) in [
        ("rank.eq", false, "RANK.EQ", "=RANK.EQ(A1,$A$1:$A$4)", 2.0),
        (
            "rang.gleich",
            true,
            "RANK.EQ",
            "=RANG.GLEICH(A1;$A$1:$A$4)",
            2.0,
        ),
        (
            "rank.avg",
            false,
            "RANK.AVG",
            "=RANK.AVG(A1,$A$1:$A$4)",
            2.5,
        ),
        (
            "rang.mittelw",
            true,
            "RANK.AVG",
            "=RANG.MITTELW(A1;$A$1:$A$4)",
            2.5,
        ),
        ("large", false, "LARGE", "=LARGE($A$1:$A$4,2)", 75.0),
        ("kgrößte", true, "LARGE", "=KGRÖSSTE($A$1:$A$4;2)", 75.0),
        ("small", false, "SMALL", "=SMALL($A$1:$A$4,2)", 75.0),
        ("kkleinste", true, "SMALL", "=KKLEINSTE($A$1:$A$4;2)", 75.0),
    ] {
        let hit = matching(query)[0];
        assert_eq!(hit.english, function);
        assert_eq!(hit.expression(0, None, 4, german).unwrap(), draft);
        assert_eq!(evaluate_formula(&draft[1..], &table), Some(expected));
    }
    let lone = matching("kkleinste")[0];
    assert_eq!(
        lone.expression(26, None, 1, true).unwrap(),
        "=KKLEINSTE($AA$1:$AA$1;1)"
    );
}

#[test]
fn search_example_fills_the_position_column_from_original_text_rows() {
    let table = DataTable {
        columns: vec![ColumnInfo {
            name: "Code".into(),
            data_type: "Utf8".into(),
        }],
        rows: ["SKU-01", "ä😀sKu-02", "0007", "sku-😀7"]
            .map(|code| vec![CellValue::String(code.into())])
            .to_vec(),
        ..DataTable::empty()
    };
    for (query, german, expected_formula) in [
        ("search", false, r#"=IFERROR(SEARCH("sku-??",A1),0)"#),
        ("suchen", true, r#"=WENNFEHLER(SUCHEN("sku-??";A1);0)"#),
    ] {
        let example = matching(query)[0];
        assert_eq!(example.english, "SEARCH");
        let draft = example
            .expression(0, None, table.row_count(), german)
            .unwrap();
        assert_eq!(draft, expected_formula);
        let (rows, bad) = super::super::fill_formula_template(&draft[1..], &table);
        assert!(bad.is_none());
        assert_eq!(rows, vec![(0, 1.0), (1, 3.0), (2, 0.0), (3, 1.0)]);
    }
}

#[test]
fn text_checks_are_searchable_and_examples_evaluate_in_both_languages() {
    let table = DataTable {
        rows: vec![vec![CellValue::String("SKU-01".into())]],
        ..DataTable::empty()
    };
    for (query, german, english, draft, value) in [
        ("len", false, "LEN", "=LEN(A1)", 6.0),
        ("länge", true, "LEN", "=LÄNGE(A1)", 6.0),
        ("exact", false, "EXACT", r#"=EXACT(A1,"SKU-01")"#, 1.0),
        (
            "identisch",
            true,
            "EXACT",
            r#"=IDENTISCH(A1;"SKU-01")"#,
            1.0,
        ),
        ("find", false, "FIND", r#"=IFERROR(FIND("-",A1),0)"#, 4.0),
        (
            "finden",
            true,
            "FIND",
            r#"=WENNFEHLER(FINDEN("-";A1);0)"#,
            4.0,
        ),
    ] {
        let got = matching(query);
        assert_eq!(got[0].english, english, "{query}");
        assert_eq!(got[0].expression(0, None, 1, german).unwrap(), draft);
        assert_eq!(evaluate_formula(&draft[1..], &table), Some(value));
    }
}

#[test]
fn directed_rounding_recipes_find_both_names_and_set_correct_arity() {
    let table = DataTable {
        columns: vec![ColumnInfo {
            name: "Spend".into(),
            data_type: "Float64".into(),
        }],
        rows: vec![vec![CellValue::Float(-1.141)]],
        ..DataTable::empty()
    };
    for (query, german, expected_expr, value) in [
        ("roundup", false, "=ROUNDUP(A1,2)", -1.15),
        ("aufrunden", true, "=AUFRUNDEN(A1;2)", -1.15),
        ("rounddown", false, "=ROUNDDOWN(A1,2)", -1.14),
        ("abrunden", true, "=ABRUNDEN(A1;2)", -1.14),
        ("trunc", false, "=TRUNC(A1)", -1.0),
        ("kürzen", true, "=KÜRZEN(A1)", -1.0),
    ] {
        let hits = matching(query);
        assert_eq!(hits.len(), 1, "{query}");
        let draft = hits[0].expression(0, None, 1, german).unwrap();
        assert_eq!(draft, expected_expr);
        assert_eq!(evaluate_formula(&draft[1..], &table), Some(value));
    }
}

#[test]
fn numbervalue_recipe_sets_explicit_input_separators_for_each_language() {
    for (query, german, input, expected_formula) in [
        ("zahlenwert", true, "2.500,27", r#"ZAHLENWERT(A1;",";".")"#),
        (
            "numbervalue",
            false,
            "2,500.27",
            r#"NUMBERVALUE(A1,".",",")"#,
        ),
    ] {
        let recipes = matching(query);
        assert_eq!(recipes.len(), 1);
        let draft = recipes[0].expression(0, None, 1, german).unwrap();
        assert_eq!(draft.strip_prefix('='), Some(expected_formula));
        let table = DataTable {
            columns: vec![ColumnInfo {
                name: "import".into(),
                data_type: "Utf8".into(),
            }],
            rows: vec![vec![CellValue::String(input.into())]],
            ..DataTable::empty()
        };
        assert_eq!(evaluate_formula(expected_formula, &table), Some(2500.27));
    }
}

#[test]
fn paired_examples_anchor_both_columns_and_keep_slope_argument_order() {
    let table = DataTable {
        columns: ["Y", "Other", "X"]
            .map(|name| ColumnInfo {
                name: name.into(),
                data_type: "Int64".into(),
            })
            .to_vec(),
        rows: (0..5)
            .map(|x| {
                vec![
                    CellValue::Int(3 * x + 1),
                    CellValue::Int(-100),
                    CellValue::Int(x),
                ]
            })
            .collect(),
        ..DataTable::empty()
    };
    for (query, german, expected, number) in [
        ("korrel", true, "=KORREL($C$1:$C$5;$A$1:$A$5)", 1.0),
        ("steigung", true, "=STEIGUNG($A$1:$A$5;$C$1:$C$5)", 3.0),
        ("slope", false, "=SLOPE($A$1:$A$5,$C$1:$C$5)", 3.0),
        ("intercept", false, "=INTERCEPT($A$1:$A$5,$C$1:$C$5)", 1.0),
        (
            "bestimmtheitsmaß",
            true,
            "=BESTIMMTHEITSMASS($A$1:$A$5;$C$1:$C$5)",
            1.0,
        ),
        (
            "prognose.linear",
            true,
            "=PROGNOSE.LINEAR(C1;$A$1:$A$5;$C$1:$C$5)",
            1.0,
        ),
        (
            "covariance.p",
            false,
            "=COVARIANCE.P($C$1:$C$5,$A$1:$A$5)",
            6.0,
        ),
        (
            "kovarianz.s",
            true,
            "=KOVARIANZ.S($C$1:$C$5;$A$1:$A$5)",
            7.5,
        ),
    ] {
        let recipe = matching(query)[0];
        assert!(recipe.uses_pair());
        assert_eq!(recipe.expression(2, None, 5, german), None);
        assert_eq!(recipe.expression(2, Some(2), 5, german), None);
        let draft = recipe.expression(2, Some(0), 5, german).unwrap();
        assert_eq!(draft, expected);
        let got = evaluate_formula(&draft[1..], &table).unwrap();
        assert!((got - number).abs() < 1e-14, "{query}: {got}");
    }
}

fn examples_app() -> crate::app::OctaApp {
    let mut app = crate::app::OctaApp::new(
        vec![],
        octa::ui::settings::AppSettings::default(),
        octa::ui::settings::IconVariant::Rose,
    );
    let tab = &mut app.tabs[0];
    tab.table = DataTable {
        columns: ["Measured demand", "Hidden units", "Temperature"]
            .map(|name| ColumnInfo {
                name: name.into(),
                data_type: "Int64".into(),
            })
            .to_vec(),
        rows: (0..4)
            .map(|x| {
                vec![
                    CellValue::Int(4 * x + 1),
                    CellValue::Int(7),
                    CellValue::Int(x),
                ]
            })
            .collect(),
        ..DataTable::empty()
    };
    tab.table_state.selected_cell = Some((0, 2));
    tab.hidden_columns.insert(1);
    tab.new_col_formula = "=11".into();
    tab.new_col_examples = FormulaExamples::open(tab);
    app
}

fn picker_frame(
    ctx: &egui::Context,
    tab: &mut TabState,
    id: egui::Id,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let mut out = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(360.0, 600.0),
            )),
            events,
            ..Default::default()
        },
        |ui| render(ui.ctx(), tab, id, true),
    );
    out.textures_delta.clear();
    out
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

#[test]
fn paired_recipe_keyboard_applies_only_the_visible_two_column_draft() {
    let mut app = examples_app();
    let tab = &mut app.tabs[0];
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let draft = egui::Id::new("worksheet-formula");
    let query = egui::Id::new(("dbviewy_formula_examples", tab.id)).with("query");
    picker_frame(&ctx, tab, draft, vec![]);
    assert!(ctx.memory(|m| m.has_focus(query)));
    let out = picker_frame(&ctx, tab, draft, vec![egui::Event::Text("steigung".into())]);
    let expression = if current_language() == "de" {
        "=STEIGUNG($A$1:$A$4;$C$1:$C$4)"
    } else {
        "=SLOPE($A$1:$A$4,$C$1:$C$4)"
    };
    let texts: Vec<_> = out
        .shapes
        .iter()
        .filter_map(|item| match &item.shape {
            egui::Shape::Text(shape) if item.clip_rect.intersects(shape.visual_bounding_rect()) => {
                Some(shape.galley.text())
            }
            _ => None,
        })
        .collect();
    assert!(texts.contains(&expression), "{texts:?}");
    let column = t("dialog.date_column");
    assert!(texts.contains(&format!("{column} (X)").as_str()));
    assert!(texts.contains(&format!("{column} (Y)").as_str()));
    assert!(texts.iter().all(|line| !line.contains("Hidden units")));
    assert_eq!(tab.new_col_formula, "=11");
    picker_frame(&ctx, tab, draft, vec![key(egui::Key::Escape)]);
    assert_eq!(tab.new_col_formula, "=11");
    assert!(tab.new_col_examples.is_none());
    assert!(ctx.memory(|m| m.has_focus(draft)));

    tab.new_col_examples = FormulaExamples::open(tab);
    picker_frame(&ctx, tab, draft, vec![]);
    picker_frame(&ctx, tab, draft, vec![egui::Event::Text("steigung".into())]);
    picker_frame(&ctx, tab, draft, vec![key(egui::Key::Enter)]);
    assert_eq!(tab.new_col_formula, expression);
    assert!(tab.new_col_examples.is_none());
    assert!(ctx.memory(|m| m.has_focus(draft)));
    assert_eq!(tab.table.col_count(), 3);
    assert_eq!(tab.table.get(0, 0), Some(&CellValue::Int(1)));
    assert!(tab.table.undo_stack.is_empty());
}

#[test]
fn paired_recipe_with_no_visible_partner_never_overwrites_the_draft() {
    let mut app = examples_app();
    let tab = &mut app.tabs[0];
    tab.hidden_columns.insert(0);
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let draft = egui::Id::new("worksheet-formula");
    picker_frame(&ctx, tab, draft, vec![]);
    let out = picker_frame(&ctx, tab, draft, vec![egui::Event::Text("korrel".into())]);
    let expected = t("dialog.formula_examples_use");
    let update = out.platform_output.accesskit_update.unwrap();
    assert!(
        update
            .nodes
            .iter()
            .any(|(_, node)| { node.label() == Some(expected.as_str()) && node.is_disabled() })
    );
    assert_eq!(partner_source(tab, 2, Some(0)), None);
    picker_frame(&ctx, tab, draft, vec![key(egui::Key::Enter)]);
    assert_eq!(tab.new_col_formula, "=11");
    assert!(tab.new_col_examples.is_some());
    picker_frame(&ctx, tab, draft, vec![key(egui::Key::Escape)]);
    assert_eq!(tab.new_col_formula, "=11");

    tab.hidden_columns.remove(&0);
    assert_eq!(partner_source(tab, 2, Some(1)), Some(0));
}

#[test]
fn calendar_period_examples_are_searchable_and_keep_date_cell_relative() {
    let table = DataTable {
        rows: vec![
            vec![CellValue::Date("2024-01-31".into())],
            vec![CellValue::Date("2021-01-03".into())],
        ],
        ..DataTable::empty()
    };
    for (search, german, expected, want) in [
        ("EDATE", false, "=EDATE(A1,1)", 45_351.0),
        ("edatum", true, "=EDATUM(A1;1)", 45_351.0),
        ("eomonth", false, "=EOMONTH(A1,0)", 45_322.0),
        ("monatsende", true, "=MONATSENDE(A1;0)", 45_322.0),
        ("isoweeknum", false, "=ISOWEEKNUM(A1)", 5.0),
        ("isokalenderwoche", true, "=ISOKALENDERWOCHE(A1)", 5.0),
    ] {
        let recipe = matching(search)[0];
        let formula = recipe
            .expression(0, None, table.row_count(), german)
            .unwrap();
        assert_eq!(formula, expected);
        assert_eq!(evaluate_formula(&formula[1..], &table), Some(want));
        if search == "isokalenderwoche" {
            let shifted = formula.replace("A1", "A2");
            assert_eq!(evaluate_formula(&shifted[1..], &table), Some(53.0));
        }
    }
}

#[test]
fn cashflow_recipes_emit_the_expected_rate_and_anchored_ranges() {
    for (q, name, partner, german, expected) in [
        ("npv", "NPV", None, false, "=NPV(10%,$AA$1:$AA$30)"),
        ("nbw", "NPV", None, true, "=NBW(10%;$AA$1:$AA$30)"),
        (
            "xnpv",
            "XNPV",
            Some(27),
            false,
            "=XNPV(10%,$AA$1:$AA$30,$AB$1:$AB$30)",
        ),
        (
            "xkapitalwert",
            "XNPV",
            Some(27),
            true,
            "=XKAPITALWERT(10%;$AA$1:$AA$30;$AB$1:$AB$30)",
        ),
    ] {
        let found = matching(q);
        assert_eq!(found[0].english, name);
        assert_eq!(
            found[0].expression(26, partner, 30, german).as_deref(),
            Some(expected)
        );
    }
}

#[test]
fn dated_recipe_names_amounts_and_dates_and_keyboard_only_updates_the_draft() {
    let mut app = examples_app();
    let tab = &mut app.tabs[0];
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let draft = egui::Id::new("cashflow-draft");
    let query = egui::Id::new(("dbviewy_formula_examples", tab.id)).with("query");
    picker_frame(&ctx, tab, draft, vec![]);
    assert!(ctx.memory(|m| m.has_focus(query)));
    let output = picker_frame(
        &ctx,
        tab,
        draft,
        vec![egui::Event::Text("xkapitalwert".into())],
    );
    let expr = if current_language() == "de" {
        "=XKAPITALWERT(10%;$C$1:$C$4;$A$1:$A$4)"
    } else {
        "=XNPV(10%,$C$1:$C$4,$A$1:$A$4)"
    };
    let texts: Vec<_> = output
        .shapes
        .iter()
        .filter_map(|item| match &item.shape {
            egui::Shape::Text(shape) if item.clip_rect.intersects(shape.visual_bounding_rect()) => {
                Some(shape.galley.text())
            }
            _ => None,
        })
        .collect();
    assert!(texts.contains(&expr), "{texts:?}");
    assert!(texts.contains(&t("rfm.amount").as_str()), "{texts:?}");
    assert!(texts.contains(&t("rfm.date").as_str()), "{texts:?}");
    assert!(texts.iter().all(|text| !text.contains("Hidden units")));
    assert_eq!(tab.new_col_formula, "=11");
    picker_frame(&ctx, tab, draft, vec![key(egui::Key::Enter)]);
    assert_eq!(tab.new_col_formula, expr);
    assert!(ctx.memory(|m| m.has_focus(draft)));
    assert_eq!(tab.table.col_count(), 3);
    assert!(tab.table.undo_stack.is_empty());
}

#[test]
fn localized_means_prepare_fixed_ranges_and_fill_numeric_results() {
    let table = DataTable {
        columns: vec![ColumnInfo {
            name: "Rate".into(),
            data_type: "Int64".into(),
        }],
        rows: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
            .map(|n| vec![CellValue::Int(n)])
            .to_vec(),
        ..DataTable::empty()
    };
    for (query, en, original, expected) in [
        (
            "geomittel",
            "GEOMEAN",
            "=GEOMITTEL($A$1:$A$10)",
            3_628_800.0_f64.powf(0.1),
        ),
        (
            "harmittel",
            "HARMEAN",
            "=HARMITTEL($A$1:$A$10)",
            10.0 / (1..=10).map(|n| 1.0 / f64::from(n)).sum::<f64>(),
        ),
        (
            "gestutztmittel",
            "TRIMMEAN",
            "=GESTUTZTMITTEL($A$1:$A$10;20%)",
            5.5,
        ),
    ] {
        let suggestion = matching(query)[0];
        assert_eq!(suggestion.english, en);
        assert_eq!(
            suggestion
                .expression(0, None, table.row_count(), true)
                .unwrap(),
            original
        );
        let english = suggestion
            .expression(0, None, table.row_count(), false)
            .unwrap();
        assert!(english.starts_with(&format!("={en}($A$1:$A$10")));
        let (rows, bad) = super::super::fill_formula_template(&original[1..], &table);
        assert!(bad.is_none());
        assert_eq!(rows.len(), table.row_count());
        assert!(
            rows.iter()
                .enumerate()
                .all(|(i, (row, value))| i == *row && (value - expected).abs() < 1e-10)
        );
        let other = evaluate_formula(&english[1..], &table).unwrap();
        assert!((other - expected).abs() < 1e-10);
    }
}
