//! The image viewer: one picture at a time, walked with the arrow keys.
//!
//! It opens on a cell whose text is an image URL (cell menu **View image**,
//! or a click on a table thumbnail) and collects every image-bearing cell
//! of the tab into `items`, in the order the table showed them, so Left /
//! Right / Up / Down step through the sheet's pictures the way the grid
//! steps through its cells. The pictures themselves come from the shared
//! [`octa::ui::images::ImageCache`]: the current one plus its neighbours are
//! requested every frame, a finished fetch shows up on the next repaint,
//! and nothing is downloaded twice between cells and viewer.
//!
//! `zoom` is `None` for fit-to-window and `Some(factor)` for an exact
//! multiple of the pixel size; the scroll bars pan the zoomed picture.

use eframe::egui;
use egui::RichText;

use octa::data::CellValue;
use octa::data::links::detect_image_url;
use octa::i18n::t;
use octa::ui::settings::{
    DialogSize, dialog_header, draw_result_message, remember_dialog_rect, size_dialog_window,
};

use crate::app::state::{ImageViewerState, OctaApp};

/// Neighbouring pictures requested ahead of the arrow keys, so stepping
/// through a sheet feels instant once the first fetch landed.
const PREFETCH: usize = 2;
/// Zoom step per key press or button click.
const ZOOM_STEP: f32 = 1.25;
/// Details panel width; the picture keeps the rest.
const DETAILS_W: f32 = 250.0;
/// Row-detail values are cut here; the cell itself may hold a paragraph.
const DETAIL_VALUE_MAX: usize = 80;
/// Row details stop after this many non-empty cells.
const DETAIL_MAX_ROWS: usize = 14;

impl OctaApp {
    /// Open the viewer on the image cell (`row`, `col`) of the active tab.
    /// `row` indexes the table, like every other cell reference the table
    /// reports. Every other image-bearing cell joins the walk list, in the
    /// order the current filter and sort show them.
    pub(crate) fn open_image_viewer(&mut self, row: usize, col: usize) {
        let tab = &self.tabs[self.active_tab];
        let mut items = Vec::new();
        for &actual in &tab.filtered_rows {
            for c in 0..tab.table.col_count() {
                // Only plain strings can hold a URL; skipping every other
                // cell keeps the scan allocation-free on big sheets.
                let Some(CellValue::String(s)) = tab.table.get(actual, c) else {
                    continue;
                };
                if let Some(url) = detect_image_url(s) {
                    items.push(crate::app::state::ImageItem {
                        row: actual,
                        col: c,
                        url: url.to_string(),
                    });
                }
            }
        }
        let pos = items
            .iter()
            .position(|it| it.row == row && it.col == col)
            .unwrap_or(0);
        if items.is_empty() {
            self.status_message = Some((t("imgview.no_image_cell"), std::time::Instant::now()));
            return;
        }
        self.image_viewer = Some(ImageViewerState {
            size: DialogSize::Normal,
            items,
            pos,
            zoom: None,
            message: None,
        });
    }
}

pub(crate) fn render_image_viewer(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(mut st) = app.image_viewer.take() else {
        return;
    };
    let mut close = false;

    // Arrow keys walk the pictures, whatever has focus - unless a text
    // field owns the keyboard (the SQL panel, the chat input), where the
    // keys belong to the caret. Esc closes; 0 fits; +/- zoom.
    if !ctx.egui_wants_keyboard_input() {
        let key = |k: egui::Key| ctx.input(|i| i.key_pressed(k) && i.modifiers.is_none());
        if key(egui::Key::Escape) {
            close = true;
        } else {
            let prev =
                key(egui::Key::ArrowLeft) || key(egui::Key::ArrowUp) || key(egui::Key::PageUp);
            let next =
                key(egui::Key::ArrowRight) || key(egui::Key::ArrowDown) || key(egui::Key::PageDown);
            if prev && st.pos > 0 {
                st.pos -= 1;
            }
            if next && st.pos + 1 < st.items.len() {
                st.pos += 1;
            }
            if key(egui::Key::Num0) {
                st.zoom = None;
            } else if key(egui::Key::Plus) || key(egui::Key::Equals) {
                st.zoom = Some((st.zoom.unwrap_or(1.0) * ZOOM_STEP).min(16.0));
            } else if key(egui::Key::Minus) {
                st.zoom = Some((st.zoom.unwrap_or(1.0) / ZOOM_STEP).max(0.02));
            }
        }
    }
    st.pos = st.pos.min(st.items.len().saturating_sub(1));
    let item = st.items.get(st.pos).cloned();

    // Warm the cache for the current picture and its neighbours; `request`
    // deduplicates, so repeating this every frame costs a hash lookup.
    if item.is_some() {
        for d in -(PREFETCH as isize)..=(PREFETCH as isize) {
            if let Some(i) = (st.pos as isize + d)
                .try_into()
                .ok()
                .and_then(|i: usize| st.items.get(i))
            {
                app.image_cache.request(ctx, &i.url);
            }
        }
    }

    let dialog_id = egui::Id::new("octa_image_viewer");
    let window = egui::Window::new("octa_image_viewer_window")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, st.size, window, |w| {
        w.resizable(true)
            .default_width(760.0)
            .default_height(560.0)
            .min_width(380.0)
            .min_height(260.0)
    });
    let mut size = st.size;
    let inner = window.show(ctx, |ui| {
        close |= dialog_header(
            ui,
            "imgview_header",
            RichText::new(format!(
                "{} - {}/{}",
                t("imgview.title"),
                st.pos + 1,
                st.items.len()
            )),
            &mut size,
        );
        if size == DialogSize::Minimized {
            return;
        }
        draw_toolbar(ui, &mut st, &mut close);
        egui::Panel::bottom("imgview_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                if let Some((ok, msg)) = &st.message {
                    draw_result_message(ui, *ok, msg);
                }
            });
        draw_body(ui, app, &st, &item);
    });
    if let Some(inner) = inner.as_ref() {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    st.size = size;

    if !close {
        app.image_viewer = Some(st);
    }
}

/// Prev/next, zoom and the way out, in one control row. Every button names
/// its keys in the hover, so the keyboard hints are discoverable.
fn draw_toolbar(ui: &mut egui::Ui, st: &mut ImageViewerState, close: &mut bool) {
    let mut prev = false;
    let mut next = false;
    let mut zoom_in = false;
    let mut zoom_out = false;
    let mut fit = false;
    let mut open_url = false;
    octa::ui::control_row::control_line(ui, |ui| {
        prev = ui
            .add_enabled(st.pos > 0, egui::Button::new("<"))
            .on_hover_text(t("imgview.prev_hint"))
            .clicked();
        next = ui
            .add_enabled(st.pos + 1 < st.items.len(), egui::Button::new(">"))
            .on_hover_text(t("imgview.next_hint"))
            .clicked();
        ui.separator();
        zoom_out = ui
            .add_enabled(st.zoom.is_some_and(|z| z > 0.021), egui::Button::new("-"))
            .on_hover_text(t("imgview.zoom_out_hint"))
            .clicked();
        let pct = st
            .zoom
            .map(|z| format!("{:.0}%", z * 100.0))
            .unwrap_or_else(|| t("imgview.fit"));
        ui.weak(pct);
        zoom_in = ui
            .add_enabled(st.zoom.is_none_or(|z| z < 15.9), egui::Button::new("+"))
            .on_hover_text(t("imgview.zoom_in_hint"))
            .clicked();
        fit = ui
            .selectable_label(st.zoom.is_none(), t("imgview.fit"))
            .on_hover_text(t("imgview.fit_hint"))
            .clicked();
        ui.separator();
        open_url = ui
            .button(t("imgview.open_url"))
            .on_hover_text(t("imgview.open_url_hint"))
            .clicked();
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(t("common.close")).clicked() {
                *close = true;
            }
        });
    });
    if prev && st.pos > 0 {
        st.pos -= 1;
    }
    if next && st.pos + 1 < st.items.len() {
        st.pos += 1;
    }
    if zoom_in {
        st.zoom = Some((st.zoom.unwrap_or(1.0) * ZOOM_STEP).min(16.0));
    }
    if zoom_out {
        st.zoom = Some((st.zoom.unwrap_or(1.0) / ZOOM_STEP).max(0.02));
    }
    if fit {
        st.zoom = None;
    }
    if open_url && let Some(it) = st.items.get(st.pos) {
        ui.ctx().open_url(egui::OpenUrl::new_tab(&it.url));
    }
}

/// Picture left, details right. The picture area is a both-way scroll area
/// so a zoomed image pans with the bars or a drag; the details column
/// scrolls on its own.
fn draw_body(
    ui: &mut egui::Ui,
    app: &OctaApp,
    st: &ImageViewerState,
    item: &Option<crate::app::state::ImageItem>,
) {
    let details_w = DETAILS_W.min(ui.available_width() * 0.45).max(160.0);
    let img_w = (ui.available_width() - details_w - 8.0).max(80.0);
    let body_h = ui.available_height();
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(img_w, body_h),
            egui::Layout::top_down(egui::Align::Min),
            |ui| draw_picture(ui, app, st, item),
        );
        ui.allocate_ui_with_layout(
            egui::vec2(details_w, body_h),
            egui::Layout::top_down(egui::Align::Min),
            |ui| draw_details(ui, app, st, item),
        );
    });
}

fn draw_picture(
    ui: &mut egui::Ui,
    app: &OctaApp,
    st: &ImageViewerState,
    item: &Option<crate::app::state::ImageItem>,
) {
    let Some(it) = item else { return };
    let texture = app.image_cache.texture(&it.url);
    let failure = app.image_cache.failure(&it.url);
    egui::ScrollArea::both()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                match (texture, failure) {
                    (Some(tex), _) => {
                        let natural = tex.size_vec2();
                        // Fit uses the viewport the scroll area just got;
                        // a manual zoom is an exact multiple of the pixels.
                        let scale = match st.zoom {
                            Some(z) => z,
                            None => fit_zoom(ui.available_size(), natural),
                        };
                        let size = natural * scale;
                        ui.add(
                            egui::Image::from_texture(&tex)
                                .fit_to_exact_size(size)
                                .sense(egui::Sense::click()),
                        );
                    }
                    (None, Some(err)) => {
                        ui.label(
                            RichText::new(t("imgview.failed")).color(ui.visuals().error_fg_color),
                        );
                        ui.label(RichText::new(err).weak().monospace());
                    }
                    (None, None) => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(t("imgview.loading"));
                        });
                    }
                }
            });
        });
    ui.weak(t("imgview.keys"));
}

/// The scale that fits `natural` inside `avail`, aspect kept, never
/// upside-down on a zero-sized viewport. Pure so the arithmetic is testable.
fn fit_zoom(avail: egui::Vec2, natural: egui::Vec2) -> f32 {
    if natural.x <= 0.0 || natural.y <= 0.0 || avail.x <= 0.0 || avail.y <= 0.0 {
        return 1.0;
    }
    (avail.x / natural.x).min(avail.y / natural.y).max(0.01)
}

/// What the picture belongs to: the cell's column, where it sits in the
/// sheet, the address itself, and the row's other values so the picture is
/// never seen without its context.
fn draw_details(
    ui: &mut egui::Ui,
    app: &OctaApp,
    st: &ImageViewerState,
    item: &Option<crate::app::state::ImageItem>,
) {
    let Some(it) = item else { return };
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let tab = &app.tabs[app.active_tab];
            let col_name = tab
                .table
                .columns
                .get(it.col)
                .map(|c| c.name.clone())
                .unwrap_or_default();
            ui.strong(t("imgview.details"));
            ui.separator();
            ui.label(format!("{}: {}", t("imgview.column"), col_name));
            ui.label(format!("{}: {}", t("imgview.row"), it.row + 1));
            ui.label(format!(
                "{}: {}/{}",
                t("imgview.position"),
                st.pos + 1,
                st.items.len()
            ));
            if let Some(tex) = app.image_cache.texture(&it.url) {
                let s = tex.size_vec2();
                ui.label(format!(
                    "{}: {} x {}",
                    t("imgview.size"),
                    s.x as u32,
                    s.y as u32
                ));
            }
            ui.add_space(4.0);
            ui.label(RichText::new(t("imgview.url")).weak());
            egui::ScrollArea::horizontal()
                .id_salt("imgview_url")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add(
                        egui::Label::new(RichText::new(&it.url).monospace().size(11.0))
                            .selectable(true),
                    );
                });
            ui.separator();
            draw_row_details(ui, app, it);
        });
}

/// The row's other non-empty cells, so a picture can be judged against its
/// title, price and notes without leaving the viewer. Only drawn while the
/// cell still resolves to the same address - after a sort or a tab switch
/// the row index may point somewhere else entirely.
fn draw_row_details(ui: &mut egui::Ui, app: &OctaApp, it: &crate::app::state::ImageItem) {
    let tab = &app.tabs[app.active_tab];
    let fresh = matches!(
        tab.table.get(it.row, it.col),
        Some(CellValue::String(s)) if detect_image_url(s) == Some(it.url.as_str())
    );
    if !fresh {
        return;
    }
    let mut shown = 0;
    for c in 0..tab.table.col_count() {
        if shown >= DETAIL_MAX_ROWS {
            break;
        }
        if c == it.col {
            continue;
        }
        let Some(v) = tab.table.get(it.row, c) else {
            continue;
        };
        let text = v.display_with_binary_mode(app.settings.binary_display_mode);
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        let name = tab
            .table
            .columns
            .get(c)
            .map(|col| col.name.clone())
            .unwrap_or_else(|| c.to_string());
        let mut value: String = text.chars().take(DETAIL_VALUE_MAX).collect();
        if text.chars().count() > DETAIL_VALUE_MAX {
            value.push_str("...");
        }
        ui.label(
            RichText::new(format!("{name}: "))
                .weak()
                .monospace()
                .size(11.0),
        );
        ui.label(RichText::new(value).monospace().size(11.0));
        shown += 1;
    }
}

#[cfg(test)]
#[path = "image_viewer_tests.rs"]
mod tests;
