//! Offline customer workflows, not another registry smoke test. Each case drives
//! production I/O or automation and checks the data a user gets back.
use std::fs;
use std::path::{Path, PathBuf};

use octa::data::{CellValue, ColumnInfo, DataTable};
use octa::formats::{FormatRegistry, read_table_auto};

#[path = "dbviewy_workflows/automation.rs"]
mod automation;
#[path = "dbviewy_workflows/databases.rs"]
mod databases;
#[path = "dbviewy_workflows/formats.rs"]
mod formats;
#[path = "dbviewy_workflows/formulas.rs"]
mod formulas;
#[path = "dbviewy_workflows/sql.rs"]
mod sql;

const MAX_DECOMPRESSED: u64 = 8 * 1024 * 1024;

struct Fixture(tempfile::TempDir);

impl Fixture {
    fn new() -> Self {
        Self(tempfile::tempdir().unwrap())
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.path().join(name)
    }

    fn file(&self, name: &str, body: impl AsRef<[u8]>) -> PathBuf {
        let path = self.path(name);
        fs::write(&path, body).unwrap();
        path
    }
}

fn read(path: &Path) -> DataTable {
    read_table_auto(path, None, MAX_DECOMPRESSED)
        .unwrap_or_else(|e| panic!("open {}: {e:#}", path.display()))
}

fn write(path: &Path, table: &DataTable) {
    FormatRegistry::new()
        .reader_for_path(path)
        .expect("registered writer")
        .write_file(path, table)
        .unwrap_or_else(|e| panic!("save {}: {e:#}", path.display()));
}

fn table(columns: &[(&str, &str)], rows: Vec<Vec<CellValue>>) -> DataTable {
    assert!(rows.iter().all(|row| row.len() == columns.len()));
    DataTable {
        columns: columns
            .iter()
            .map(|(name, data_type)| ColumnInfo {
                name: (*name).into(),
                data_type: (*data_type).into(),
            })
            .collect(),
        rows,
        ..DataTable::empty()
    }
}

fn col(table: &DataTable, name: &str) -> usize {
    table
        .columns
        .iter()
        .position(|col| col.name == name)
        .unwrap_or_else(|| panic!("missing {name:?} in {:?}", table.columns))
}

fn cell<'a>(table: &'a DataTable, row: usize, name: &str) -> &'a CellValue {
    table.get(row, col(table, name)).expect("cell in result")
}

fn cells(table: &DataTable, name: &str) -> Vec<CellValue> {
    (0..table.row_count())
        .map(|row| cell(table, row, name).clone())
        .collect()
}

fn text(table: &DataTable, name: &str, expected: &[&str]) {
    let expected: Vec<_> = expected
        .iter()
        .map(|v| CellValue::String((*v).into()))
        .collect();
    assert_eq!(cells(table, name), expected, "{name}");
}

fn numbers(table: &DataTable, name: &str, expected: &[f64]) {
    let actual: Vec<_> = cells(table, name)
        .into_iter()
        .map(|value| match value {
            CellValue::Int(n) => n as f64,
            CellValue::Float(n) => n,
            other => panic!("{name:?} expected numeric data, got {other:?}"),
        })
        .collect();
    assert_eq!(actual, expected, "{name}");
}
