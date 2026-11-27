//! The Edit command copies typed values, not a clipboard representation.

use std::collections::{BTreeMap, HashMap, HashSet};

use octa::data::{CellValue, DataTable, ViewMode};
use octa::i18n::t;
use octa::ui::table_view::TableViewState;

use super::state::{OctaApp, TabState};

#[derive(Clone, Copy)]
pub(crate) enum Direction {
    Down,
    Right,
}

impl Direction {
    fn coordinate(self, line: usize, offset: usize) -> (usize, usize) {
        match self {
            Self::Down => (offset, line),
            Self::Right => (line, offset),
        }
    }

    fn need_cells_key(self) -> &'static str {
        match self {
            Self::Down => "greyed.need_two_cells",
            Self::Right => "greyed.need_two_cells_in_row",
        }
    }
}

enum Selection<'a> {
    Columns(&'a [usize], Vec<usize>),
    Cells(BTreeMap<usize, Vec<usize>>),
}

impl<'a> Selection<'a> {
    fn new(
        direction: Direction,
        st: &TableViewState,
        rows: &'a [usize],
        hidden: &HashSet<usize>,
        table: &DataTable,
    ) -> Option<Self> {
        // A row-header selection is not permission to write to every column.
        let (min_rows, min_cols) = match direction {
            Direction::Down => (2, 1),
            Direction::Right => (1, 2),
        };
        if !st.selected_rows.is_empty() || st.editing_cell.is_some() || rows.len() < min_rows {
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
            // A header selection uses one borrowed visible-row list.
            return (cols.len() >= min_cols).then_some(Self::Columns(rows, cols));
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
        let mut lines = BTreeMap::<_, Vec<_>>::new();
        for &row in rows {
            if picks.is_empty() {
                break;
            }
            let Some(mut cols) = picks.remove(&row) else {
                continue;
            };
            match direction {
                Direction::Down => {
                    for col in cols {
                        lines.entry(col).or_default().push(row);
                    }
                }
                Direction::Right if cols.len() >= 2 => {
                    cols.sort_unstable();
                    lines.insert(row, cols);
                }
                Direction::Right => {}
            }
        }
        lines.retain(|_, offsets| offsets.len() >= 2);
        (!lines.is_empty()).then_some(Self::Cells(lines))
    }

    fn apply(self, table: &mut DataTable, direction: Direction) -> bool {
        let undo_start = table.undo_stack.len();
        match (self, direction) {
            (Self::Columns(rows, cols), Direction::Down) => {
                for col in cols {
                    copy_line(table, direction, col, rows);
                }
            }
            (Self::Columns(rows, cols), Direction::Right) => {
                for &row in rows {
                    copy_line(table, direction, row, &cols);
                }
            }
            (Self::Cells(lines), _) => {
                for (line, offsets) in lines {
                    copy_line(table, direction, line, &offsets);
                }
            }
        }
        table.coalesce_undo_since(undo_start);
        table.undo_stack.len() > undo_start
    }
}

fn copy_line(table: &mut DataTable, direction: Direction, line: usize, offsets: &[usize]) {
    let Some((&first, rest)) = offsets.split_first() else {
        return;
    };
    let (row, col) = direction.coordinate(line, first);
    let value = table.get(row, col).cloned().unwrap_or(CellValue::Null);
    for &offset in rest {
        let (row, col) = direction.coordinate(line, offset);
        if table.get(row, col).unwrap_or(&CellValue::Null) != &value {
            table.set(row, col, value.clone());
        }
    }
}

// The toolbar retains a lazy, field-scoped borrow while its search box
// owns other mutable fields of the same tab.
pub(crate) fn fill_block(
    direction: Direction,
    view: ViewMode,
    state: &TableViewState,
    rows: &[usize],
    hidden: &HashSet<usize>,
    table: &DataTable,
) -> Option<&'static str> {
    if view != ViewMode::Table {
        Some("greyed.need_table_view")
    } else {
        Selection::new(direction, state, rows, hidden, table)
            .is_none()
            .then_some(direction.need_cells_key())
    }
}

impl TabState {
    pub(crate) fn fill_block(&self, direction: Direction) -> Option<&'static str> {
        fill_block(
            direction,
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
        self.fill_values(Direction::Down);
    }

    pub(crate) fn fill_right(&mut self) {
        self.fill_values(Direction::Right);
    }

    fn fill_values(&mut self, direction: Direction) {
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
            direction,
            &tab.table_state,
            &tab.filtered_rows,
            &tab.hidden_columns,
            &tab.table,
        ) else {
            self.status_message = Some((t(direction.need_cells_key()), std::time::Instant::now()));
            return;
        };
        if selection.apply(&mut tab.table, direction) {
            tab.table_state.invalidate_row_heights();
            tab.filter_dirty = true;
        }
    }
}

#[cfg(test)]
#[path = "fill_down_tests.rs"]
mod tests;
