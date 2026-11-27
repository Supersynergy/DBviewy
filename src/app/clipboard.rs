//! Clipboard plumbing: copy current selection to tab-separated text, paste
//! tab-separated text back, and bridge to the OS clipboard.

use eframe::egui;

use octa::data;
use octa::ui;

use super::state::{OctaApp, TabState};

#[path = "clipboard_tsv.rs"]
mod tsv;

struct VisibleSelection {
    cols: Vec<usize>,
    rows: Vec<usize>,
    masked: bool,
}

impl VisibleSelection {
    fn new(tab: &TabState) -> Option<Self> {
        let state = &tab.table_state;
        let table = &tab.table;
        let visible_col = |col| col < table.col_count() && !tab.hidden_columns.contains(&col);
        // The filter emits loaded row indices in grid order (the underlying
        // table itself is reordered by Sort). It is safe to look up a lone
        // cell without walking every row in a large table.
        let visible_rows = &tab.filtered_rows;
        let visible_row = |row| {
            row < table.row_count()
                && visible_rows.binary_search(&row).is_ok()
                && !table.hidden_rows.contains(&row)
        };
        let rows_between = |lo, hi| {
            let start = visible_rows.partition_point(|&row| row < lo);
            let end = visible_rows.partition_point(|&row| row <= hi);
            visible_rows[start..end]
                .iter()
                .copied()
                .filter(|row| *row < table.row_count() && !table.hidden_rows.contains(row))
                .collect()
        };
        let mut masked = false;
        let (cols, rows): (Vec<_>, Vec<_>) = if !state.selected_rows.is_empty() {
            let mut rows: Vec<_> = state
                .selected_rows
                .iter()
                .copied()
                .filter(|&row| visible_row(row))
                .collect();
            rows.sort_unstable();
            (
                (0..table.col_count())
                    .filter(|&col| visible_col(col))
                    .collect(),
                rows,
            )
        } else if !state.selected_cols.is_empty() {
            let mut cols: Vec<_> = state
                .selected_cols
                .iter()
                .copied()
                .filter(|&col| visible_col(col))
                .collect();
            cols.sort_unstable();
            (cols, rows_between(0, table.row_count().saturating_sub(1)))
        } else if !state.selected_cells.is_empty() {
            let mut picks = state
                .selected_cells
                .iter()
                .copied()
                .filter(|&(r, c)| visible_col(c) && visible_row(r));
            let (first_row, first_col) = picks.next()?;
            let (r0, r1, c0, c1) = picks.fold(
                (first_row, first_row, first_col, first_col),
                |(r0, r1, c0, c1), (r, c)| (r0.min(r), r1.max(r), c0.min(c), c1.max(c)),
            );
            masked = true;
            (
                (c0..=c1).filter(|&col| visible_col(col)).collect(),
                rows_between(r0, r1),
            )
        } else if let Some((row, col)) = state.selected_cell
            && visible_col(col)
            && visible_row(row)
        {
            (vec![col], vec![row])
        } else {
            return None;
        };
        (!cols.is_empty() && !rows.is_empty()).then_some(Self { cols, rows, masked })
    }

    fn to_tsv(&self, tab: &TabState) -> String {
        let Self { cols, rows, masked } = self;
        let single_cell = rows.len() == 1 && cols.len() == 1;
        let mut text = String::new();
        for (i, &row) in rows.iter().enumerate() {
            if i != 0 {
                text.push('\n');
            }
            for (j, &col) in cols.iter().enumerate() {
                if j != 0 {
                    text.push('\t');
                }
                let cell = if !masked || tab.table_state.selected_cells.contains(&(row, col)) {
                    tab.table
                        .get(row, col)
                        .map(ToString::to_string)
                        .unwrap_or_default()
                } else {
                    String::new()
                };
                if single_cell && !cell.contains(['\t', '\r', '\n']) {
                    // Keep single-line cell text ready to paste into editors,
                    // including surrounding quotes in identifiers.
                    return cell;
                }
                tsv::write_field(&mut text, &cell, cols.len() == 1);
            }
        }
        text
    }
}

impl OctaApp {
    fn clipboard_tab(&mut self) -> &mut TabState {
        if self.tabs[self.active_tab].filter_dirty {
            self.recompute_filter();
        }
        &mut self.tabs[self.active_tab]
    }

    fn copy_selection_to_string(&mut self) -> Option<String> {
        let tab = self.clipboard_tab();
        Some(VisibleSelection::new(tab)?.to_tsv(tab))
    }

    fn copy_selection_as_markdown(&mut self) -> Option<String> {
        let tab = self.clipboard_tab();
        let VisibleSelection { cols, rows, masked } = VisibleSelection::new(tab)?;
        let state = &tab.table_state;
        let table = &tab.table;

        // Only with a disjoint multi-cell selection do out-of-set cells render
        // blank; otherwise every cell in the rectangle is included.
        let cell = |r: usize, c: usize| -> String {
            if masked && !state.selected_cells.contains(&(r, c)) {
                return String::new();
            }
            md_escape(&table.get(r, c).map(|v| v.to_string()).unwrap_or_default())
        };

        let mut out = String::new();
        // Header.
        let headers: Vec<String> = cols
            .iter()
            .map(|&c| {
                md_escape(
                    table
                        .columns
                        .get(c)
                        .map(|ci| ci.name.as_str())
                        .unwrap_or(""),
                )
            })
            .collect();
        out.push_str("| ");
        out.push_str(&headers.join(" | "));
        out.push_str(" |\n|");
        for _ in &cols {
            out.push_str(" --- |");
        }
        out.push('\n');
        // Body.
        for &r in &rows {
            out.push_str("| ");
            let row_cells: Vec<String> = cols.iter().map(|&c| cell(r, c)).collect();
            out.push_str(&row_cells.join(" | "));
            out.push_str(" |\n");
        }
        Some(out)
    }

    /// Copy the current selection to the OS clipboard as a Markdown table.
    pub(crate) fn do_copy_markdown(&mut self, ctx: &egui::Context) {
        if let Some(text) = self.copy_selection_as_markdown() {
            ctx.copy_text(text);
        }
    }

    /// The Copy as submenu and its shortcuts.
    pub(crate) fn do_copy_as(&mut self, ctx: &egui::Context, how: data::copy_formats::CopyAs) {
        use data::copy_formats::{Block, CopyAs, insert_table_name, to_insert, to_json};
        match how {
            CopyAs::Markdown => self.do_copy_markdown(ctx),
            CopyAs::InList => self.do_copy_in_list(ctx),
            CopyAs::Json | CopyAs::Insert => {
                let tab = self.clipboard_tab();
                let Some(VisibleSelection { cols, rows, masked }) = VisibleSelection::new(tab)
                else {
                    return;
                };
                let block = Block {
                    table: &tab.table,
                    cols,
                    rows,
                    only: masked.then_some(&tab.table_state.selected_cells),
                };
                let text = if how == CopyAs::Json {
                    to_json(&block)
                } else {
                    let db_table = tab.db_origin.as_ref().map(|o| o.table.as_str()).or(tab
                        .table
                        .db_meta
                        .as_ref()
                        .map(|m| m.table_name.as_str()));
                    to_insert(
                        &block,
                        &insert_table_name(db_table, tab.table.source_path.as_deref()),
                    )
                };
                ctx.copy_text(text);
                self.status_message = Some((
                    octa::i18n::t("copy_as.copied")
                        .replace("{format}", &octa::i18n::t(how.label_key())),
                    std::time::Instant::now(),
                ));
            }
        }
    }

    /// Copy the visible selected values as a SQL `IN` list.
    pub(crate) fn do_copy_in_list(&mut self, ctx: &egui::Context) {
        let tab = self.clipboard_tab();
        let Some(VisibleSelection { cols, rows, masked }) = VisibleSelection::new(tab) else {
            return;
        };
        let table = &tab.table;
        let text = if masked {
            // A sparse selection can span a huge block. Walk only the
            // picked cells for a format that does not preserve blank gaps.
            let mut picks: Vec<_> = tab
                .table_state
                .selected_cells
                .iter()
                .copied()
                .filter(|(row, col)| {
                    rows.binary_search(row).is_ok() && cols.binary_search(col).is_ok()
                })
                .collect();
            picks.sort_unstable();
            data::in_list::sql_in_list(
                picks
                    .into_iter()
                    .filter_map(|(row, col)| table.get(row, col)),
            )
        } else {
            data::in_list::sql_in_list(
                rows.into_iter()
                    .flat_map(|r| cols.iter().filter_map(move |&c| table.get(r, c))),
            )
        };
        match text {
            Some(text) => {
                ctx.copy_text(text);
                self.status_message = Some((
                    octa::i18n::t("context_menu.copied_in_list"),
                    std::time::Instant::now(),
                ));
            }
            None => {
                self.status_message = Some((
                    octa::i18n::t("context_menu.copy_in_list_empty"),
                    std::time::Instant::now(),
                ));
            }
        }
    }

    /// Paste tab-separated text into the table at the current selection.
    pub(crate) fn paste_text_into_table(&mut self, text: &str) {
        let decoded = tsv::quoted_grid(text);
        let parsed_rows: Vec<Vec<&str>> = if let Some(rows) = &decoded {
            rows.iter()
                .map(|row| row.iter().map(|field| field.as_ref()).collect())
                .collect()
        } else {
            tsv::plain_lines(text)
                .map(|line| line.split('\t').collect())
                .collect()
        };
        if parsed_rows.is_empty() {
            return;
        }

        let tab = &mut self.tabs[self.active_tab];
        let (start_row, start_col) = tab.table_state.selected_cell.unwrap_or((0, 0));
        let undo_from = tab.table.undo_stack.len();

        // A block bigger than the table grows it, as a spreadsheet does; one
        // smaller leaves the cells past it as they are.
        let rows_needed = start_row + parsed_rows.len();
        let cols_needed = start_col + parsed_rows.iter().map(Vec::len).max().unwrap_or(0);
        grow_paste_target(tab, rows_needed, cols_needed);

        for (ri, row_cells) in parsed_rows.iter().enumerate() {
            let target_row = start_row + ri;
            if target_row >= tab.table.row_count() {
                break;
            }
            for (ci, &cell_text) in row_cells.iter().enumerate() {
                let target_col = start_col + ci;
                if target_col >= tab.table.col_count() {
                    break;
                }
                if let Some(existing) = tab.table.get(target_row, target_col).cloned() {
                    let new_val = data::CellValue::parse_like(&existing, cell_text);
                    tab.table.set(target_row, target_col, new_val);
                }
            }
        }
        // One paste, one Ctrl+Z, rows and columns it added included.
        tab.table.coalesce_undo_since(undo_from);
        tab.filter_dirty = true;
    }

    /// Cut: copy selection then clear the underlying cells.
    pub(crate) fn do_cut(&mut self, ctx: &egui::Context) {
        // Ctrl+X and the right-click menu both land here.
        if self.is_readonly() {
            return;
        }
        let tab = self.clipboard_tab();
        let Some(selection) = VisibleSelection::new(tab) else {
            return;
        };
        ctx.copy_text(selection.to_tsv(tab));
        let undo_from = tab.table.undo_stack.len();
        let clear = |table: &mut data::DataTable, row, col| {
            if table
                .get(row, col)
                .is_some_and(|value| *value != data::CellValue::Null)
            {
                table.set(row, col, data::CellValue::Null);
            }
        };
        if selection.masked {
            for &(row, col) in &tab.table_state.selected_cells {
                if selection.rows.binary_search(&row).is_ok()
                    && selection.cols.binary_search(&col).is_ok()
                {
                    clear(&mut tab.table, row, col);
                }
            }
        } else {
            for row in selection.rows {
                for &col in &selection.cols {
                    clear(&mut tab.table, row, col);
                }
            }
        }
        if tab.table.undo_stack.len() > undo_from {
            tab.table.coalesce_undo_since(undo_from);
            tab.table_state.invalidate_row_heights();
            tab.filter_dirty = true;
        }
    }

    /// Copy the selection to the OS clipboard.
    ///
    /// Every clipboard write goes through egui (`ctx.copy_text`), never a
    /// clipboard library of our own: egui's windowing layer talks Wayland on
    /// Wayland and X11 on X11, and it is the same clipboard every text box
    /// pastes from. A separate `arboard` handle used to write the X11
    /// clipboard even on Wayland, so a copied cell never reached the SQL
    /// editor.
    pub(crate) fn do_copy(&mut self, ctx: &egui::Context) {
        if let Some(text) = self.copy_selection_to_string() {
            ctx.copy_text(text);
        }
    }

    /// Paste `text` into the table. `None` (a menu click, a remapped Paste
    /// key) carries no text yet: ask the windowing layer for the clipboard,
    /// which comes back next frame as an ordinary `Event::Paste`.
    pub(crate) fn do_paste(&mut self, ctx: &egui::Context, text: Option<String>) {
        if self.is_readonly() {
            return;
        }
        match text {
            Some(text) if !text.is_empty() => self.paste_text_into_table(&text),
            Some(_) => {}
            None => ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaste),
        }
    }

    /// Zoom to `percent`, held to 25-500%, remember it for the next start and
    /// re-measure the active tab's rows. The shortcuts, the View menu and the
    /// status bar all end here.
    pub(crate) fn set_zoom(&mut self, ctx: &egui::Context, percent: u32) {
        self.remember_zoom(percent);
        self.settings.save();
        self.apply_zoom(ctx);
        self.tabs[self.active_tab]
            .table_state
            .invalidate_row_heights();
    }

    fn remember_zoom(&mut self, percent: u32) {
        self.settings.zoom_percent = percent;
        self.zoom_percent = self.settings.zoom_percent();
    }

    pub(crate) fn apply_zoom(&self, ctx: &egui::Context) {
        let base_font_size = self.settings.font_size;
        let effective_font_size = base_font_size * self.zoom_percent as f32 / 100.0;
        ui::theme::apply_theme(
            ctx,
            self.theme_mode,
            ui::theme::FontSettings {
                size: effective_font_size,
                body: self.settings.body_font,
                custom_path: Some(self.settings.custom_font_path.as_str()),
            },
        );
    }
}

/// Extend the grid only when its source allows it. Both paste routes
/// coalesce growth and cell edits into one undo entry.
pub(super) fn grow_paste_target(
    tab: &mut super::state::TabState,
    rows_needed: usize,
    cols_needed: usize,
) {
    if tab.paste_can_grow() {
        while tab.table.col_count() < cols_needed {
            let name = tab
                .table
                .unique_column_name(&format!("col{}", tab.table.col_count() + 1));
            let at = tab.table.col_count();
            tab.table.insert_column(at, name, "Utf8".to_string());
        }
        append_paste_rows(&mut tab.table, rows_needed);
    }
}

fn append_paste_rows(table: &mut data::DataTable, rows_needed: usize) {
    let first = table.row_count();
    let extra = rows_needed.saturating_sub(first);
    match extra {
        0 => return,
        1 => return table.insert_row(first),
        _ => {}
    }
    // Keep the existing structural flags and undo entries, but park the
    // row-indexed overlays while appending. Reindex them at most once,
    // not once per new row.
    let edits = std::mem::take(&mut table.edits);
    let marks = std::mem::take(&mut table.marks);
    let hidden = std::mem::take(&mut table.hidden_rows);
    for row in first..rows_needed {
        table.insert_row(row);
    }
    let shift = |row| if row < first { row } else { row + extra };
    table.edits = if edits.keys().any(|&(row, _)| row >= first) {
        edits
            .into_iter()
            .map(|((r, c), value)| ((shift(r), c), value))
            .collect()
    } else {
        edits
    };
    use data::MarkKey::{Cell, Column, Row};
    table.marks = if marks
        .keys()
        .any(|k| matches!(k, Cell(r, _) | Row(r) if *r >= first))
    {
        marks
            .into_iter()
            .map(|(key, color)| {
                let key = match key {
                    Cell(r, c) => Cell(shift(r), c),
                    Row(r) => Row(shift(r)),
                    k @ Column(_) => k,
                };
                (key, color)
            })
            .collect()
    } else {
        marks
    };
    table.hidden_rows = if hidden.iter().any(|&r| r >= first) {
        hidden.into_iter().map(shift).collect()
    } else {
        hidden
    };
}

/// Escape a cell value for a Markdown table cell: pipes break the column
/// structure and newlines break the row, so neutralise both.
fn md_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace('\r', "")
        .replace('\n', "<br>")
}

#[cfg(test)]
#[path = "clipboard_tests.rs"]
mod tests;
