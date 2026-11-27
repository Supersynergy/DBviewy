use super::*;
use octa::data::compare::{CompareOptions, compare_join, compare_ordered};

fn table(columns: &[&str], rows: &[&[&str]]) -> DataTable {
    DataTable {
        columns: columns
            .iter()
            .map(|name| ColumnInfo {
                name: (*name).into(),
                data_type: "Utf8".into(),
            })
            .collect(),
        rows: rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|text| CellValue::String((*text).into()))
                    .collect()
            })
            .collect(),
        ..DataTable::empty()
    }
}

#[test]
fn keyed_projection_keeps_right_values_under_names_and_preserves_missing_cells() {
    let left = table(
        &["id", "qty", "a_note"],
        &[&["old", "1", "first"], &["id1", "2", "before"]],
    );
    let right = table(&["qty", "id"], &[&["3", "id1"], &["9", "new"]]);
    let result = compare_join(&left, &right, &["id".into()], &CompareOptions::default()).unwrap();
    let output = CompareOutput::new(&left, &right, &result, true);
    let text = |r| {
        let row = output.row(r).unwrap();
        (0..output.col_count())
            .map(|c| row.text(c).into_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(text(0), ["only_in_a", "", "old", "1", "first"]);
    assert_eq!(text(1), ["only_in_b", "", "new", "9", ""]);
    assert_eq!(text(2), ["changed_a", "qty", "id1", "2", "before"]);
    assert_eq!(text(3), ["changed_b", "qty", "id1", "3", ""]);
    assert!(output.row(4).is_none());
    let materialised = output.to_table();
    assert_eq!(materialised.get(3, 4), Some(&CellValue::Null));
    assert_eq!(materialised.get(3, 3), Some(&CellValue::String("3".into())));
}

#[test]
fn ordered_projection_remains_positional_and_right_fallback_is_complete() {
    let left = table(&["id", "qty"], &[&["1", "2"]]);
    let right = table(&["qty", "id"], &[&["2", "1"], &["9", "new"]]);
    let result = compare_ordered(&left, &right, &CompareOptions::default());
    let output = CompareOutput::new(&left, &right, &result, false);
    assert_eq!(output.header(2), "id");
    assert_eq!(output.row(0).unwrap().text(2), "9");
    assert_eq!(output.row(2).unwrap().text(2), "2");
    let empty = DataTable::empty();
    let all_added = compare_ordered(&empty, &right, &CompareOptions::default());
    let fallback = CompareOutput::new(&empty, &right, &all_added, false);
    assert_eq!(fallback.header(2), "qty");
    assert_eq!(fallback.row(1).unwrap().text(3), "new");
}

#[test]
fn tsv_preserves_embedded_tabs_quotes_and_newlines_without_a_row_cap() {
    let special = "Nord\tTor\r\nA \"B\"";
    let left = table(&["id", "note"], &[&["A", special]]);
    let right = table(&["id", "note"], &[&["B", "new"]]);
    let result = compare_join(&left, &right, &["id".into()], &CompareOptions::default()).unwrap();
    let output = CompareOutput::new(&left, &right, &result, true);
    let text = output.to_tsv().unwrap();
    let parsed = csv::ReaderBuilder::new()
        .delimiter(b'\t')
        .from_reader(text.as_bytes())
        .records()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(parsed.len(), 2);
    assert_eq!(&parsed[0][3], special);
    assert_eq!(&parsed[1][3], "new");
}
