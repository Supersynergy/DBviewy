//! The Edit command copies typed values, not a clipboard representation.

use std::collections::{BTreeMap, HashMap, HashSet};

use octa::data::{CellValue, DataTable, ViewMode};
use octa::i18n::t;
use octa::ui::table_view::TableViewState;

use super::state::{OctaApp, TabState};

enum Selection<'a> {
    Columns(&'a [usize], Vec<usize>),
    Cells(BTreeMap<usize, Vec<usize>>),
}

impl<'a> Selection<'a> {
    fn new(
        st: &TableViewState,
        rows: &'a [usize],
        hidden: &HashSet<usize>,
        table: &DataTable,
    ) -> Option<Self> {
        // Entire rows have their own shortcuts. Do not turn a row-header
        // selection into an unannounced write to every column.
        if !st.selected_rows.is_empty() || st.editing_cell.is_some() || rows.len() < 2 {
            return None;
        }
        let visible_column = |&col: &usize| col < table.col_count() && !hidden.contains(&col);
        if !st.selected_cols.is_empty() {
            let mut cols: Vec<_> = st
                .selected_cols
                .iter()
                .copied()
                .filter(visible_column)
                .collect();
            cols.sort_unstable();
            // Borrow one ordered row vector rather than making a copy per
            // column, especially when the headers select many loaded rows.
            return (!cols.is_empty()).then_some(Self::Columns(rows, cols));
        }
        if st.selected_cells.len() < 2 {
            return None;
        }
        let mut picks: HashMap<_, Vec<_>> = HashMap::new();
        for &(row, col) in &st.selected_cells {
            if row < table.row_count() && visible_column(&col) {
                picks.entry(row).or_default().push(col);
            }
        }
        let mut cols = BTreeMap::<_, Vec<_>>::new();
        for &row in rows {
            if picks.is_empty() {
                break;
            }
            if let Some(cells) = picks.remove(&row) {
                for col in cells {
                    cols.entry(col).or_default().push(row);
                }
            }
        }
        cols.retain(|_, rows| rows.len() >= 2);
        (!cols.is_empty()).then_some(Self::Cells(cols))
    }

    fn apply(self, table: &mut DataTable) -> bool {
        let undo_start = table.undo_stack.len();
        match self {
            Self::Columns(rows, cols) => {
                for col in cols {
                    copy_column(table, col, rows);
                }
            }
            Self::Cells(cols) => {
                for (col, rows) in cols {
                    copy_column(table, col, &rows);
                }
            }
        }
        table.coalesce_undo_since(undo_start);
        table.undo_stack.len() > undo_start
    }
}

fn copy_column(table: &mut DataTable, col: usize, rows: &[usize]) {
    let Some((&first, below)) = rows.split_first() else {
        return;
    };
    let value = table.get(first, col).cloned().unwrap_or(CellValue::Null);
    for &row in below {
        if table.get(row, col).unwrap_or(&CellValue::Null) != &value {
            table.set(row, col, value.clone());
        }
    }
}

// The toolbar retains a lazy, field-scoped borrow while its search box
// owns other mutable fields of the same tab.
pub(crate) fn fill_down_block(
    view: ViewMode,
    state: &TableViewState,
    rows: &[usize],
    hidden: &HashSet<usize>,
    table: &DataTable,
) -> Option<&'static str> {
    if view != ViewMode::Table {
        Some("greyed.need_table_view")
    } else {
        Selection::new(state, rows, hidden, table)
            .is_none()
            .then_some("greyed.need_two_cells")
    }
}

impl TabState {
    pub(crate) fn fill_down_block(&self) -> Option<&'static str> {
        fill_down_block(
            self.view_mode,
            &self.table_state,
            &self.filtered_rows,
            &self.hidden_columns,
            &self.table,
        )
    }
}

impl OctaApp {
    pub(crate) fn fill_down(&mut self) {
        if self.is_readonly() {
            return;
        }
        if self.tabs[self.active_tab].filter_dirty {
            self.recompute_filter();
        }
        let tab = &mut self.tabs[self.active_tab];
        if tab.view_mode != ViewMode::Table {
            return;
        }
        let Some(selection) = Selection::new(
            &tab.table_state,
            &tab.filtered_rows,
            &tab.hidden_columns,
            &tab.table,
        ) else {
            self.status_message = Some((t("greyed.need_two_cells"), std::time::Instant::now()));
            return;
        };
        if selection.apply(&mut tab.table) {
            tab.filter_dirty = true;
        }
    }
}

#[cfg(test)]
#[path = "fill_down_tests.rs"]
mod tests;
