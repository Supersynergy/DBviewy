//! Route the context-menu and keyboard filters through the existing facet.

use octa::data::{CellValue, ViewMode};
use octa::ui::table_view::{FacetPopupResult, cell_filter_value};

use crate::app::state::OctaApp;

impl OctaApp {
    pub(crate) fn selected_cell_filter(&self) -> Result<(usize, &CellValue), &'static str> {
        let tab = self.tabs.get(self.active_tab).ok_or("greyed.need_table")?;
        if tab.view_mode != ViewMode::Table {
            return Err("greyed.need_table_view");
        }
        let (row, col) = tab
            .table_state
            .selected_cell
            .ok_or("bookmarks.need_selection")?;
        // The table's visible row IDs are emitted in physical order by
        // recompute_filter, even after sorting (which reorders the table).
        // The palette can ask every frame: don't scan a million row IDs.
        if tab.hidden_columns.contains(&col) || tab.filtered_rows.binary_search(&row).is_err() {
            return Err("bookmarks.need_selection");
        }
        let server_values = crate::app::db_view::server_conn(
            tab,
            self.settings.db_pushdown,
            &self.settings.db_connections,
        )
        .is_some();
        Ok((col, cell_filter_value(&tab.table, row, col, server_values)?))
    }

    pub(crate) fn filter_to_selected_cell(&mut self) {
        if let Ok((col, value)) = self.selected_cell_filter() {
            self.apply_column_values(FacetPopupResult {
                col,
                allowed: [value.to_string()].into(),
                cleared: false,
            });
        }
    }

    pub(super) fn apply_column_values(&mut self, result: FacetPopupResult) {
        let tab = &mut self.tabs[self.active_tab];
        let changed = if result.cleared {
            tab.column_filters.remove(&result.col).is_some()
        } else if tab.column_filters.get(&result.col) == Some(&result.allowed) {
            false
        } else {
            tab.column_filters.insert(result.col, result.allowed);
            true
        };
        tab.filter_dirty |= changed;
    }
}
