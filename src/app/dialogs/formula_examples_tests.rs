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
