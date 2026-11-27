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
    for &recipe in RECIPES {
        for german in [false, true] {
            let formula = recipe.expression(26, table.row_count(), german);
            let expr = formula.strip_prefix('=').unwrap();
            let value = evaluate_formula(expr, &table);
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
            hits[0].expression(27, 105, true),
            "=QUANTIL.INKL($AB$1:$AB$105;95%)"
        );
    }
    assert!(matching("unregistered fx").is_empty());
    assert_eq!(matching("").len(), RECIPES.len());
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
        assert_eq!(got[0].expression(0, 1, german), draft);
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
        let draft = hits[0].expression(0, 1, german);
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
        let draft = recipes[0].expression(0, 1, german);
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
