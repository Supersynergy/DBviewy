//! Annotated ordered or keyed comparison output shared by the desktop and CLI.

use std::borrow::Cow;
use std::collections::HashMap;

use octa::data::compare::{CompareResult, RowChange};
use octa::data::{CellValue, ColumnInfo, DataTable};

const META_COLUMNS: [&str; 2] = ["status", "changed_columns"];

/// Column alignment and physical row ordering without a copy of either input.
/// Reordered columns align by name in a keyed comparison, never in Ordered.
pub(crate) struct CompareOutput<'a> {
    a: &'a DataTable,
    b: &'a DataTable,
    result: &'a CompareResult,
    columns: &'a [ColumnInfo],
    right_columns: Option<Vec<Option<usize>>>,
}

impl<'a> CompareOutput<'a> {
    pub(crate) fn new(
        a: &'a DataTable,
        b: &'a DataTable,
        result: &'a CompareResult,
        keyed: bool,
    ) -> Self {
        let columns = if a.col_count() == 0 {
            &b.columns
        } else {
            &a.columns
        };
        let right_columns = (keyed && a.col_count() > 0).then(|| {
            let mut by_name = HashMap::with_capacity(b.col_count());
            for (i, column) in b.columns.iter().enumerate() {
                // The comparison engine uses the first matching column name.
                by_name.entry(column.name.as_str()).or_insert(i);
            }
            columns
                .iter()
                .map(|column| by_name.get(column.name.as_str()).copied())
                .collect()
        });
        Self {
            a,
            b,
            result,
            columns,
            right_columns,
        }
    }

    pub(crate) fn col_count(&self) -> usize {
        META_COLUMNS.len() + self.columns.len()
    }

    pub(crate) fn header(&self, column: usize) -> &str {
        if let Some(&label) = META_COLUMNS.get(column) {
            label
        } else {
            &self.columns[column - META_COLUMNS.len()].name
        }
    }

    pub(crate) fn row_count(&self) -> usize {
        self.result.only_in_a.len() + self.result.only_in_b.len() + 2 * self.result.changed.len()
    }

    pub(crate) fn row(&self, index: usize) -> Option<OutputRow<'_>> {
        let map = self.right_columns.as_deref();
        let a_only = &self.result.only_in_a;
        let b_only = &self.result.only_in_b;
        if let Some(&source_row) = a_only.get(index) {
            return Some(OutputRow::new(self.a, source_row, "only_in_a", None, None));
        }
        let index = index - a_only.len();
        if let Some(&source_row) = b_only.get(index) {
            return Some(OutputRow::new(self.b, source_row, "only_in_b", map, None));
        }
        let index = index - b_only.len();
        let change = self.result.changed.get(index / 2)?;
        if index.is_multiple_of(2) {
            Some(OutputRow::new(
                self.a,
                change.row_a,
                "changed_a",
                None,
                Some(change),
            ))
        } else {
            Some(OutputRow::new(
                self.b,
                change.row_b,
                "changed_b",
                map,
                Some(change),
            ))
        }
    }

    pub(crate) fn to_table(&self) -> DataTable {
        let mut table = DataTable::empty();
        table
            .columns
            .extend(META_COLUMNS.into_iter().map(|name| ColumnInfo {
                name: name.into(),
                data_type: "Utf8".into(),
            }));
        table.columns.extend_from_slice(self.columns);
        table.rows.reserve(self.row_count());
        for i in 0..self.row_count() {
            let row = self
                .row(i)
                .expect("row_count is the length of the projection");
            let mut cells = Vec::with_capacity(self.col_count());
            cells.push(CellValue::String(row.status().into()));
            cells.push(CellValue::String(row.text(1).into_owned()));
            cells.extend(
                (0..self.columns.len()).map(|c| row.cell(c).cloned().unwrap_or(CellValue::Null)),
            );
            table.rows.push(cells);
        }
        table
    }

    // This is invoked only when the user explicitly copies the result.
    pub(crate) fn to_tsv(&self) -> anyhow::Result<String> {
        let mut writer = csv::WriterBuilder::new()
            .delimiter(b'\t')
            .from_writer(Vec::new());
        writer.write_record((0..self.col_count()).map(|i| self.header(i)))?;
        for i in 0..self.row_count() {
            let row = self
                .row(i)
                .expect("row_count is the length of the projection");
            for c in 0..self.col_count() {
                writer.write_field(row.text(c).as_bytes())?;
            }
            writer.write_record(std::iter::empty::<&str>())?;
        }
        Ok(String::from_utf8(writer.into_inner()?)?)
    }
}

pub(crate) struct OutputRow<'a> {
    source: &'a DataTable,
    index: usize,
    status: &'static str,
    map: Option<&'a [Option<usize>]>,
    change: Option<&'a RowChange>,
}

impl<'a> OutputRow<'a> {
    fn new(
        source: &'a DataTable,
        index: usize,
        status: &'static str,
        map: Option<&'a [Option<usize>]>,
        change: Option<&'a RowChange>,
    ) -> Self {
        Self {
            source,
            index,
            status,
            map,
            change,
        }
    }

    pub(crate) fn status(&self) -> &'static str {
        self.status
    }

    fn cell(&self, col: usize) -> Option<&'a CellValue> {
        let source_col = match self.map {
            Some(map) => *map.get(col)?.as_ref()?,
            None => col,
        };
        self.source.get(self.index, source_col)
    }

    pub(crate) fn text(&self, col: usize) -> Cow<'a, str> {
        match col {
            0 => self.status.into(),
            1 => self
                .change
                .map(|c| c.changed_columns.join(", ").into())
                .unwrap_or_else(|| "".into()),
            _ => match self.cell(col - META_COLUMNS.len()) {
                Some(CellValue::String(text)) => text.as_str().into(),
                Some(CellValue::Null) | None => "".into(),
                Some(cell) => cell.to_string().into(),
            },
        }
    }
}

#[cfg(test)]
#[path = "compare_output_tests.rs"]
mod tests;
