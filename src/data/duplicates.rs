//! Find duplicate rows in a DataTable by an N-column key.
//!
//! Key fields are length-prefixed so text can contain any separator. The
//! same comparison is used by removal and composite uniqueness analysis.

use std::borrow::Cow;
use std::collections::HashMap;

use crate::data::{CellValue, DataTable};

/// In a number column, 1 and 1.0 are one value. Other columns use the
/// display value, retaining the existing empty/blank equivalence.
pub(crate) fn cell_key<'a>(data_type: &str, cell: &'a CellValue) -> Cow<'a, str> {
    if crate::data::is_numeric_data_type(data_type)
        && let Some(value) = crate::data::decimal::value_key(cell)
    {
        return value.into();
    }
    match cell {
        CellValue::Null => "".into(),
        CellValue::String(s)
        | CellValue::Date(s)
        | CellValue::DateTime(s)
        | CellValue::Nested(s) => s.as_str().into(),
        other => other.to_string().into(),
    }
}

pub(crate) fn encode_row_key(key: &mut Vec<u8>, table: &DataTable, row: usize, cols: &[usize]) {
    key.clear();
    for &col in cols {
        let field = table
            .get(row, col)
            .map(|v| cell_key(&table.columns[col].data_type, v))
            .unwrap_or_default();
        // Use one prefix byte for short values. Size is in bytes, not
        // characters, so multibyte text and embedded controls are literal.
        let mut len = field.len();
        while len >= 128 {
            key.push((len as u8 & 0x7f) | 0x80);
            len >>= 7;
        }
        key.push(len as u8);
        key.extend_from_slice(field.as_bytes());
    }
}

/// Return the row indices (ascending) for every row with a duplicate key.
/// If no valid key column is selected, return an empty result.
pub fn find_duplicate_rows(table: &DataTable, key_cols: &[usize]) -> Vec<usize> {
    let rows = table.row_count();
    let cols: Vec<_> = key_cols
        .iter()
        .copied()
        .filter(|&c| c < table.col_count())
        .collect();
    if rows == 0 || cols.is_empty() {
        return Vec::new();
    }

    let mut firsts: HashMap<Vec<u8>, Option<usize>> = HashMap::with_capacity(rows.min(1024));
    let mut key = Vec::new();
    let mut duplicates = Vec::new();
    for row in 0..rows {
        encode_row_key(&mut key, table, row, &cols);
        if let Some(first) = firsts.get_mut(key.as_slice()) {
            if let Some(index) = first.take() {
                duplicates.push(index);
            }
            duplicates.push(row);
        } else {
            firsts.insert(key.clone(), Some(row));
        }
    }
    duplicates.sort_unstable();
    duplicates
}
