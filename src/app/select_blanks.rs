//! Select visible empty cells without treating the selection as an edit.

use std::borrow::Cow;
use std::collections::HashSet;

use octa::data::{CellValue, ViewMode};
use octa::i18n::t;

use super::state::{OctaApp, TabState};

type Cell = (usize, usize);

#[derive(Clone, Copy)]
struct Limits {
    scan: usize,
    matches: usize,
}

const LIMITS: Limits = Limits {
    scan: 2_000_000,
    matches: 50_000,
};

#[derive(Debug, PartialEq, Eq)]
enum Refusal {
    Empty,
    TooLarge,
}

#[derive(Debug, PartialEq, Eq)]
struct Picks {
    first: Cell,
    cells: HashSet<Cell>,
}

fn gather(tab: &TabState, limits: Limits) -> Result<Picks, Refusal> {
    let state = &tab.table_state;
    let visible = &tab.filtered_rows;
    let table = &tab.table;
    let visible_col = |c| c < table.col_count() && !tab.hidden_columns.contains(&c);
    let visible_row = |r| r < table.row_count() && visible.binary_search(&r).is_ok();
    let mut cells = HashSet::new();
    let mut first = None::<Cell>;
    let mut record = |row, col| {
        if match table.get(row, col) {
            None | Some(CellValue::Null) => true,
            Some(CellValue::String(text)) => text.is_empty(),
            _ => false,
        } {
            if cells.len() >= limits.matches {
                return Err(Refusal::TooLarge);
            }
            cells.insert((row, col));
            first = Some(first.map_or((row, col), |at| at.min((row, col))));
        }
        Ok(())
    };

    // Like Copy, row headers take precedence over column headers, then an
    // explicit cell selection. A single active cell is only a cursor.
    if state.selected_rows.is_empty()
        && state.selected_cols.is_empty()
        && !state.selected_cells.is_empty()
    {
        if state.selected_cells.len() > limits.scan {
            return Err(Refusal::TooLarge);
        }
        for &(row, col) in &state.selected_cells {
            if visible_col(col) && visible_row(row) {
                record(row, col)?;
            }
        }
    } else {
        let header_rows = !state.selected_rows.is_empty();
        let cols: Vec<usize> = if !header_rows && !state.selected_cols.is_empty() {
            state
                .selected_cols
                .iter()
                .copied()
                .filter(|&col| visible_col(col))
                .collect()
        } else {
            (0..table.col_count())
                .filter(|&col| visible_col(col))
                .collect()
        };
        if cols.is_empty() {
            return Err(Refusal::Empty);
        }
        let max_rows = limits.scan / cols.len();
        let rows: Cow<'_, [usize]> = if header_rows {
            Cow::Owned(
                state
                    .selected_rows
                    .iter()
                    .copied()
                    .filter(|&row| visible_row(row))
                    .take(max_rows.saturating_add(1))
                    .collect(),
            )
        } else {
            Cow::Borrowed(visible)
        };
        if rows.len() > max_rows {
            return Err(Refusal::TooLarge);
        }
        for &row in rows.iter().filter(|&&row| row < table.row_count()) {
            for &col in &cols {
                record(row, col)?;
            }
        }
    }
    let first = first.ok_or(Refusal::Empty)?;
    // Copy represents a sparse selection as a rectangle with blank gaps.
    // A pair of distant cells must not create a huge invisible rectangle.
    let (last_row, lo_col, hi_col) = cells.iter().fold(
        (first.0, first.1, first.1),
        |(last, lo, hi), &(row, col)| (last.max(row), lo.min(col), hi.max(col)),
    );
    let height =
        visible.partition_point(|&r| r <= last_row) - visible.partition_point(|&r| r < first.0);
    let hidden = tab
        .hidden_columns
        .iter()
        .filter(|&&col| (lo_col..=hi_col).contains(&col))
        .count();
    let width = hi_col - lo_col + 1 - hidden;
    if height.saturating_mul(width) > limits.scan {
        return Err(Refusal::TooLarge);
    }
    Ok(Picks { first, cells })
}

impl OctaApp {
    pub(crate) fn select_visible_blanks(&mut self) {
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        if tab.view_mode != ViewMode::Table || tab.table_state.editing_cell.is_some() {
            return;
        }
        if tab.filter_dirty {
            self.recompute_filter();
        }
        let message = match gather(&self.tabs[self.active_tab], LIMITS) {
            Err(Refusal::Empty) => t("search_menu.select_blanks_none"),
            Err(Refusal::TooLarge) => t("search_menu.select_blanks_large"),
            Ok(picks) => {
                let count = picks.cells.len();
                let (row, col) = picks.first;
                self.jump_to_cell(row, col);
                let state = &mut self.tabs[self.active_tab].table_state;
                state.selection_anchor_display = None;
                state.selected_cells = picks.cells;
                t("search_menu.select_blanks_done").replace("{count}", &count.to_string())
            }
        };
        self.status_message = Some((message, std::time::Instant::now()));
    }
}

#[cfg(test)]
#[path = "select_blanks_tests.rs"]
mod tests;
