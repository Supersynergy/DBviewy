use crate::data::{CellValue, DataTable};
use std::collections::HashSet;

use super::duplicates::encode_row_key;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeepWhich {
    First,
    Last,
}

/// Remove duplicate rows by key_cols (empty = whole row). Keeps the first or
/// last occurrence of each key; surviving rows stay in original order.
pub fn dedupe_rows(table: &DataTable, key_cols: &[usize], keep: KeepWhich) -> DataTable {
    let mut dropped = dedupe_dropped_indices(table, key_cols, keep)
        .into_iter()
        .rev()
        .peekable();
    let mut out = DataTable::empty();
    out.columns = table.columns.clone();
    out.rows = (0..table.row_count())
        .filter(|row| {
            if dropped.peek() == Some(row) {
                dropped.next();
                false
            } else {
                true
            }
        })
        .map(|row| {
            (0..table.col_count())
                .map(|col| table.get(row, col).cloned().unwrap_or(CellValue::Null))
                .collect()
        })
        .collect();
    out.structural_changes = true;
    out
}

/// Original row indices that dedupe_rows would drop, in descending order
/// so deletion does not shift remaining targets. An invalid explicit key
/// never results in deletion.
pub fn dedupe_dropped_indices(
    table: &DataTable,
    key_cols: &[usize],
    keep: KeepWhich,
) -> Vec<usize> {
    let cols = if key_cols.is_empty() {
        (0..table.col_count()).collect::<Vec<_>>()
    } else if key_cols.iter().all(|&c| c < table.col_count()) {
        key_cols.to_vec()
    } else {
        return Vec::new();
    };

    let rows = table.row_count();
    let mut seen: HashSet<Vec<u8>> = HashSet::with_capacity(rows.min(1024));
    let mut key = Vec::new();
    let mut dropped = Vec::new();
    let mut visit = |row| {
        encode_row_key(&mut key, table, row, &cols);
        if seen.contains(key.as_slice()) {
            dropped.push(row);
        } else {
            seen.insert(key.clone());
        }
    };
    match keep {
        KeepWhich::First => {
            (0..rows).for_each(&mut visit);
            dropped.reverse();
        }
        KeepWhich::Last => (0..rows).rev().for_each(&mut visit),
    }
    dropped
}

#[cfg(test)]
#[path = "dedupe_tests.rs"]
mod tests;
