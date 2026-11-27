//! Row hashing for the Compare view's RowHashDiff mode.
//!
//! Stable across runs (BLAKE3 has no per-process state) so two Octa sessions
//! comparing the same files always produce the same digests. The hash sees
//! only the *string representation* of cells via `CellValue::to_string`, so
//! comparison is cross-format: a CSV row and a Parquet row containing the
//! same logical values hash identically.

use octa::data::{CellValue, DataTable};

#[path = "../../data/row_key_encoding.rs"]
mod row_key_encoding;
use row_key_encoding::{append_field, displayed};

/// Hash the selected columns of one row into a 32-byte BLAKE3 digest.
/// `cols.is_empty()` means "hash every column" - the typical default until
/// the user picks specific columns in the UI.
///
/// Column ordering matters: hashing `(a=1, b=2)` produces a different digest
/// from `(b=2, a=1)`. Callers that need order-insensitive comparison should
/// pre-sort `cols` consistently between left and right.
pub fn hash_row(table: &DataTable, row_idx: usize, cols: &[usize]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    if cols.is_empty() {
        for col_idx in 0..table.col_count() {
            append_cell(&mut hasher, table, row_idx, col_idx);
        }
    } else {
        for &col_idx in cols {
            append_cell(&mut hasher, table, row_idx, col_idx);
        }
    }
    *hasher.finalize().as_bytes()
}

fn append_cell(hasher: &mut blake3::Hasher, table: &DataTable, row: usize, col: usize) {
    append_field(&displayed(table.get(row, col)), |chunk| {
        hasher.update(chunk);
    });
}

/// Format a 32-byte digest as a short hex preview (first 8 bytes = 16 hex
/// chars). Used in the diff result UI so users can spot identical hashes at
/// a glance without scrolling through 64 characters.
pub fn short_hex(digest: &[u8; 32]) -> String {
    let mut s = String::with_capacity(16);
    for b in &digest[..8] {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use octa::data::ColumnInfo;

    #[test]
    fn comparison_preview_hashes_fields_not_flattened_control_text() {
        let key = |value: &str| CellValue::String(value.into());
        let data = DataTable {
            columns: ["left", "right"]
                .map(|name| ColumnInfo {
                    name: name.into(),
                    data_type: "Utf8".into(),
                })
                .to_vec(),
            rows: vec![
                vec![key("Nord\u{1f}Lager"), key("Tor")],
                vec![key("Nord"), key("Lager\u{1f}Tor")],
                vec![key("Nord\u{1f}Lager"), key("Tor")],
            ],
            ..DataTable::empty()
        };
        assert_ne!(hash_row(&data, 0, &[]), hash_row(&data, 1, &[]));
        assert_eq!(hash_row(&data, 0, &[]), hash_row(&data, 2, &[]));
        assert_eq!(hash_row(&data, 0, &[0, 1]), hash_row(&data, 0, &[]));
        assert_ne!(hash_row(&data, 0, &[1, 0]), hash_row(&data, 0, &[]));
    }
}
