//! Row-level data diff between two `DataTable`s.
//!
//! Pure function so the CLI (`--diff`) and the MCP `diff_tables` tool can both
//! call it without a GUI. As in the Compare view, row fields use their
//! display text with explicit boundaries. A Parquet row and a CSV row
//! with the same displayed values therefore compare equal.
//!
//! Columns are compared **positionally** (all columns, in order). The two
//! tables should share the same column order/names for the result to be
//! meaningful - the same caveat the GUI row-diff states.

use std::collections::HashSet;

use crate::data::DataTable;

use super::row_key_encoding::{append_field, displayed};

/// Outcome of [`diff_rows`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RowDiff {
    /// Row indices in `a` whose key is absent from `b` (sorted ascending).
    pub only_in_a: Vec<usize>,
    /// Row indices in `b` whose key is absent from `a` (sorted ascending).
    pub only_in_b: Vec<usize>,
    /// Number of distinct row keys present in both tables.
    pub shared_keys: usize,
}

fn row_key(table: &DataTable, row: usize) -> Vec<u8> {
    let mut key = Vec::new();
    for col in 0..table.col_count() {
        append_field(&displayed(table.get(row, col)), |bytes| {
            key.extend_from_slice(bytes);
        });
    }
    key
}

/// Diff two tables by whole-row content. See module docs for the keying rules.
pub fn diff_rows(a: &DataTable, b: &DataTable) -> RowDiff {
    let a_keys: Vec<_> = (0..a.row_count()).map(|r| row_key(a, r)).collect();
    let b_keys: Vec<_> = (0..b.row_count()).map(|r| row_key(b, r)).collect();

    // Distinct-key membership for the "present on the other side?" test.
    let a_set: HashSet<&[u8]> = a_keys.iter().map(Vec::as_slice).collect();
    let b_set: HashSet<&[u8]> = b_keys.iter().map(Vec::as_slice).collect();

    let only_in_a: Vec<usize> = a_keys
        .iter()
        .enumerate()
        .filter(|(_, k)| !b_set.contains(k.as_slice()))
        .map(|(i, _)| i)
        .collect();
    let only_in_b: Vec<usize> = b_keys
        .iter()
        .enumerate()
        .filter(|(_, k)| !a_set.contains(k.as_slice()))
        .map(|(i, _)| i)
        .collect();

    let shared_keys = a_set.intersection(&b_set).count();

    RowDiff {
        only_in_a,
        only_in_b,
        shared_keys,
    }
}

#[cfg(test)]
#[path = "diff_tests.rs"]
mod tests;
