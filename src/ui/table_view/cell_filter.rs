//! Input for the ordinary Values filter when its value comes from a grid cell.

use crate::data::{CellValue, DataTable};

/// Borrow the cell without allocating a key while a menu is drawn. A live
/// database supplies its own text for the Values list; numeric and temporal
/// spellings need not agree with local display. Use the list for those types.
pub fn cell_filter_value(
    table: &DataTable,
    row: usize,
    col: usize,
    server_values: bool,
) -> Result<&CellValue, &'static str> {
    let value = table.get(row, col).ok_or("bookmarks.need_selection")?;
    if matches!(value, CellValue::Binary(_) | CellValue::Nested(_)) {
        return Err("facet.from_cell_list_hint");
    }
    if server_values
        && (!matches!(value, CellValue::String(_) | CellValue::Null)
            || !table.columns.get(col).is_some_and(|column| {
                matches!(
                    column.data_type.as_str(),
                    "Utf8" | "LargeUtf8" | "Utf8View" | "String"
                )
            }))
    {
        return Err("facet.from_cell_list_hint");
    }
    Ok(value)
}
