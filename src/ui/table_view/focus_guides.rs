//! Paint navigation guides over the visible grid; never interact with cells.

use std::{collections::HashSet, ops::Range};

use egui::{Color32, Painter, Rect, Rgba, Stroke, StrokeKind, pos2};

use crate::ui::theme::ThemeColors;

use super::{TableViewState, displayed_col_width};

pub(super) struct Frame<'a> {
    pub data_clip: Rect,
    pub row_height: f32,
    pub visible_rows: Range<usize>,
    pub filtered_rows: &'a [usize],
    pub column_starts: &'a [f32],
    pub hidden_columns: &'a HashSet<usize>,
    pub frozen_cols: usize,
    pub frozen_width: f32,
    pub data_bottom: f32,
    pub content_width: f32,
    pub scrollbar_width: f32,
    pub colors: ThemeColors,
}

/// A pane that does not show the active cell has no guides. Looking for its
/// row in the rendered slice works with filters and does not scan the table.
pub(super) fn paint(painter: &Painter, state: &TableViewState, frame: Frame<'_>) {
    let Some((row, col)) = state.selected_cell else {
        return;
    };
    if frame.hidden_columns.contains(&col) {
        return;
    }
    let Some(&col_left) = frame.column_starts.get(col) else {
        return;
    };
    let Some(local) = frame
        .filtered_rows
        .get(frame.visible_rows.clone())
        .and_then(|rows| rows.iter().position(|r| *r == row))
    else {
        return;
    };
    let display = frame.visible_rows.start + local;
    let offset = |idx| {
        state
            .row_y_offsets
            .get(idx)
            .copied()
            .unwrap_or(idx as f32 * frame.row_height)
    };
    let y = |idx| frame.data_clip.top() + offset(idx) - state.scroll_y;
    let col_width = displayed_col_width(&state.col_widths, frame.hidden_columns, col);
    let cell = Rect::from_min_max(
        pos2(col_left, y(display)),
        pos2(col_left + col_width, y(display + 1)),
    );

    let data_left = frame.data_clip.left() + state.row_number_width;
    let frozen_right = data_left + frame.frozen_width;
    let right = (frame.data_clip.left() + frame.content_width - state.scroll_x)
        .max(frozen_right)
        .min(frame.data_clip.right() - frame.scrollbar_width);
    let bottom = frame.data_clip.bottom().min(frame.data_bottom);
    let row_clip = Rect::from_min_max(frame.data_clip.min, pos2(right, bottom));
    let (band_left, band_right) = if col < frame.frozen_cols {
        (data_left, frozen_right.min(right))
    } else {
        (frozen_right, right)
    };
    let band = Rect::from_min_max(
        pos2(band_left, frame.data_clip.top()),
        pos2(band_right, bottom),
    );
    if !cell.intersect(band).is_positive() {
        return;
    }

    // Inset rails preserve the grid, coloured marks and search fills.
    // There is no per-cell fill or hit box, and each pane draws at most
    // two lines plus the outline of its active cell.
    let ink = guide_ink(frame.colors);
    let rail = Stroke::new(1.25, ink);
    painter.with_clip_rect(row_clip).hline(
        row_clip.x_range(),
        cell.bottom().min(bottom) - 1.5,
        rail,
    );
    let col_clip = Rect::from_min_max(
        pos2(band_left, painter.clip_rect().top()),
        pos2(band_right, bottom),
    );
    painter.with_clip_rect(col_clip).vline(
        cell.left().max(band_left) + 1.5,
        col_clip.y_range(),
        rail,
    );
    if !state
        .editing_cell
        .as_ref()
        .is_some_and(|(r, c, _)| (*r, *c) == (row, col))
    {
        painter.with_clip_rect(band).rect_stroke(
            cell.shrink(1.0),
            0.0,
            Stroke::new(2.0, ink),
            StrokeKind::Inside,
        );
    }
}

fn luminance(color: Color32) -> f32 {
    let [r, g, b, _] = Rgba::from(color).to_array();
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

pub(super) fn rail_contrast(a: Color32, b: Color32) -> f32 {
    let a = luminance(a) + 0.05;
    let b = luminance(b) + 0.05;
    a.max(b) / a.min(b)
}

pub(super) fn guide_ink(colors: ThemeColors) -> Color32 {
    // Some palettes use a light accent for fills. Give a fine line readable
    // ink against the standard header and row bands without changing them.
    if [colors.bg_header, colors.row_even, colors.row_odd]
        .into_iter()
        .all(|surface| rail_contrast(colors.accent, surface) >= 3.0)
    {
        colors.accent
    } else {
        colors.text_primary
    }
}
