use super::ribbon::{column_letters, parse_cell_ref};

#[test]
fn column_letters_follow_spreadsheet_naming() {
    assert_eq!(column_letters(0), "A");
    assert_eq!(column_letters(25), "Z");
    assert_eq!(column_letters(26), "AA");
    assert_eq!(column_letters(51), "AZ");
    assert_eq!(column_letters(52), "BA");
    assert_eq!(column_letters(701), "ZZ");
    assert_eq!(column_letters(702), "AAA");
}

#[test]
fn cell_reference_round_trips_with_column_letters() {
    assert_eq!(parse_cell_ref("A1"), Some((0, 0)));
    assert_eq!(parse_cell_ref(" b12 "), Some((11, 1)));
    for col in [0, 1, 25, 26, 51, 701, 702, 16383] {
        let text = format!("{}7", column_letters(col));
        assert_eq!(parse_cell_ref(&text), Some((6, col)), "{text}");
    }
}

#[test]
fn malformed_cell_references_are_rejected() {
    for bad in ["", "12", "A", "A0", "A-1", "1A", "A1B", "A 1", "$A$1"] {
        assert_eq!(parse_cell_ref(bad), None, "{bad:?}");
    }
}
