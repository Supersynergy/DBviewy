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
//! multiple of the pixel size; Ctrl+wheel zooms at the cursor, the scroll
//! bars and a plain drag pan the zoomed picture, and a double click jumps
//! between fit and 100%. The film strip under the picture clicks straight
//! to any item; **D** folds the details column away for a bigger picture.

use eframe::egui;
use egui::RichText;

use octa::data::CellValue;
use octa::data::links::{detect_image_url, detect_url};
use octa::i18n::t;
use octa::ui::icons::{self, Icon};
use octa::ui::settings::{
    DialogSize, dialog_header, draw_result_message, remember_dialog_rect, size_dialog_window,
};

use crate::app::state::{ImageViewerState, OctaApp};

/// Neighbouring pictures requested ahead of the arrow keys, so stepping
/// through a sheet feels instant once the first fetch landed.
const PREFETCH: usize = 2;
/// Zoom step per key press or button click.
const ZOOM_STEP: f32 = 1.25;
/// Zoom factor per wheel notch while Ctrl/Cmd is held.
const WHEEL_ZOOM: f32 = 0.0016;
/// Details panel width; the picture keeps the rest.
const DETAILS_W: f32 = 250.0;
/// Film strip height (one thumbnail row); hidden for a single picture.
const STRIP_H: f32 = 56.0;
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
            show_details: true,
            message: None,
        });
    }
}

pub(crate) fn render_image_viewer(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(mut st) = app.image_viewer.take() else {
        return;
    };
    let mut close = false;
    let mut open_url = false;

    // Arrow keys walk the pictures, whatever has focus - unless a text
    // field owns the keyboard (the SQL panel, the chat input), where the
    // keys belong to the caret. Esc closes; 0 fits; +/- zoom; Space and
    // Home/End move like a gallery; D folds the details column; O opens.
    if !ctx.egui_wants_keyboard_input() {
        let key = |k: egui::Key| ctx.input(|i| i.key_pressed(k) && i.modifiers.is_none());
        let shift = ctx.input(|i| i.modifiers.shift);
        if key(egui::Key::Escape) {
            close = true;
        } else {
            let prev = key(egui::Key::ArrowLeft)
                || key(egui::Key::ArrowUp)
                || key(egui::Key::PageUp)
                || (key(egui::Key::Space) && shift);
            let next = key(egui::Key::ArrowRight)
                || key(egui::Key::ArrowDown)
                || key(egui::Key::PageDown)
                || (key(egui::Key::Space) && !shift);
            if prev && st.pos > 0 {
                st.pos -= 1;
            }
            if next && st.pos + 1 < st.items.len() {
                st.pos += 1;
            }
            if key(egui::Key::Home) {
                st.pos = 0;
            }
            if key(egui::Key::End) {
                st.pos = st.items.len().saturating_sub(1);
            }
            if key(egui::Key::Num0) {
                st.zoom = None;
            } else if key(egui::Key::Plus) || key(egui::Key::Equals) {
                st.zoom = Some((st.zoom.unwrap_or(1.0) * ZOOM_STEP).min(16.0));
            } else if key(egui::Key::Minus) {
                st.zoom = Some((st.zoom.unwrap_or(1.0) / ZOOM_STEP).max(0.02));
            }
            if key(egui::Key::D) {
                st.show_details = !st.show_details;
            }
            if key(egui::Key::O) {
                open_url = true;
            }
        }
    }
    st.pos = st.pos.min(st.items.len().saturating_sub(1));
    let item = st.items.get(st.pos).cloned();
    if open_url && let Some(it) = &item {
        ctx.open_url(egui::OpenUrl::new_tab(&it.url));
    }

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
        draw_body(ui, app, &mut st, &item);
    });
    if let Some(inner) = inner.as_ref() {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    st.size = size;

    if !close {
        app.image_viewer = Some(st);
    }
}

/// Prev/next, zoom and the way out, in one control row. Icons are painted,
/// not glyphs, so no font can lack them; the hover names the action and
/// its keys, so the shortcuts stay discoverable.
fn draw_toolbar(ui: &mut egui::Ui, st: &mut ImageViewerState, close: &mut bool) {
    let mut first = false;
    let mut prev = false;
    let mut next = false;
    let mut last = false;
    let mut zoom_in = false;
    let mut zoom_out = false;
    let mut fit = false;
    let mut pct = false;
    let mut open_url = false;
    octa::ui::control_row::control_line(ui, |ui| {
        first = icons::icon_button(ui, st.pos > 0, Icon::First, t("imgview.first_hint"));
        prev = icons::icon_button(ui, st.pos > 0, Icon::Prev, t("imgview.prev_hint"));
        next = icons::icon_button(
            ui,
            st.pos + 1 < st.items.len(),
            Icon::Next,
            t("imgview.next_hint"),
        );
        last = icons::icon_button(
            ui,
            st.pos + 1 < st.items.len(),
            Icon::Last,
            t("imgview.last_hint"),
        );
        ui.separator();
        zoom_out = icons::icon_button(
            ui,
            st.zoom.is_some_and(|z| z > 0.021),
            Icon::ZoomOut,
            t("imgview.zoom_out_hint"),
        );
        let label = st
            .zoom
            .map(|z| format!("{:.0}%", z * 100.0))
            .unwrap_or_else(|| t("imgview.fit"));
        pct = ui
            .selectable_label(false, label)
            .on_hover_text(t("imgview.pct_hint"))
            .clicked();
        zoom_in = icons::icon_button(
            ui,
            st.zoom.is_none_or(|z| z < 15.9),
            Icon::ZoomIn,
            t("imgview.zoom_in_hint"),
        );
        fit = icons::icon_button(ui, st.zoom.is_some(), Icon::Fit, t("imgview.fit_hint"));
        ui.separator();
        open_url = icons::icon_button(ui, true, Icon::OpenUrl, t("imgview.open_url_hint"));
        if icons::icon_button(ui, true, Icon::Details, t("imgview.details_hint")) {
            st.show_details = !st.show_details;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icons::icon_button(ui, true, Icon::Close, t("common.close")) {
                *close = true;
            }
        });
    });
    if first {
        st.pos = 0;
    }
    if prev && st.pos > 0 {
        st.pos -= 1;
    }
    if next && st.pos + 1 < st.items.len() {
        st.pos += 1;
    }
    if last {
        st.pos = st.items.len().saturating_sub(1);
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
    if pct {
        // The percentage doubles as the fit/100% switch, like a browser's
        // zoom readout.
        st.zoom = if st.zoom.is_none() { Some(1.0) } else { None };
    }
    if open_url && let Some(it) = st.items.get(st.pos) {
        ui.ctx().open_url(egui::OpenUrl::new_tab(&it.url));
    }
}

/// Picture left, details right (when shown). The picture area is a
/// both-way scroll area so a zoomed image pans with the bars or a drag;
/// the film strip underneath jumps to any item, and the details column
/// scrolls on its own.
fn draw_body(
    ui: &mut egui::Ui,
    app: &OctaApp,
    st: &mut ImageViewerState,
    item: &Option<crate::app::state::ImageItem>,
) {
    let details_w = if st.show_details {
        DETAILS_W.min(ui.available_width() * 0.45).max(160.0)
    } else {
        0.0
    };
    let img_w = (ui.available_width() - details_w - 8.0).max(80.0);
    let body_h = ui.available_height();
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(img_w, body_h),
            egui::Layout::top_down(egui::Align::Min),
            |ui| draw_picture(ui, app, st, item),
        );
        if st.show_details {
            ui.allocate_ui_with_layout(
                egui::vec2(details_w, body_h),
                egui::Layout::top_down(egui::Align::Min),
                |ui| draw_details(ui, app, st, item),
            );
        }
    });
}

fn draw_picture(
    ui: &mut egui::Ui,
    app: &OctaApp,
    st: &mut ImageViewerState,
    item: &Option<crate::app::state::ImageItem>,
) {
    let Some(it) = item else { return };
    let scroll_id = ui.id().with("imgview_pan");
    // Ctrl/Cmd + wheel zooms at the cursor instead of scrolling; the
    // delta is consumed so the scroll area does not also pan.
    let wheel = ui.input(|i| i.smooth_scroll_delta.y);
    let wheel_area = ui
        .max_rect()
        .contains(ui.ctx().pointer_hover_pos().unwrap_or_default());
    if wheel != 0.0 && wheel_area && ui.input(|i| i.modifiers.command) {
        ui.ctx()
            .input_mut(|i| i.smooth_scroll_delta = egui::Vec2::ZERO);
        if let Some(tex) = app.image_cache.texture(&it.url) {
            let avail = ui.available_size() - egui::vec2(0.0, STRIP_H + 20.0);
            let old = st.zoom.unwrap_or_else(|| fit_zoom(avail, tex.size_vec2()));
            let new = (old * (1.0 + wheel * WHEEL_ZOOM)).clamp(0.02, 16.0);
            st.zoom = Some(new);
            // Keep the pixel under the cursor fixed: scale the scroll
            // offset by the same ratio the content just scaled by.
            if let (Some(mut sstate), Some(hp)) = (
                egui::scroll_area::State::load(ui.ctx(), scroll_id),
                ui.ctx().pointer_hover_pos(),
            ) {
                let min = ui.max_rect().min;
                let rel = sstate.offset + (hp - min);
                sstate.offset = (rel * (new / old) - (hp - min)).max(egui::Vec2::ZERO);
                sstate.store(ui.ctx(), scroll_id);
            }
        }
    }
    let texture = app.image_cache.texture(&it.url);
    let failure = app.image_cache.failure(&it.url);
    // The scroll area expands to whatever is free, so the strip and the
    // key hint reserve their share first - otherwise the picture would
    // push them off the bottom of the window.
    let strip_h = if st.items.len() > 1 { STRIP_H } else { 0.0 };
    let hint_h = ui.text_style_height(&egui::TextStyle::Small) + 4.0;
    let pic_h = (ui.available_height() - strip_h - hint_h).max(40.0);
    egui::ScrollArea::both()
        .id_salt(scroll_id)
        .auto_shrink([false, false])
        .max_height(pic_h)
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
                        // Vertically centred as well: a picture smaller
                        // than the viewport should float, not hug the top.
                        ui.add_space(((ui.available_height() - size.y) / 2.0).max(0.0));
                        let resp = ui.add(
                            egui::Image::from_texture(&tex)
                                .fit_to_exact_size(size)
                                .sense(egui::Sense::click()),
                        );
                        // Double click toggles fit <-> 100%, the gesture
                        // every picture viewer teaches first.
                        if resp.double_clicked() {
                            st.zoom = if st.zoom.is_none() { Some(1.0) } else { None };
                        }
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
    draw_filmstrip(ui, app, st);
    ui.weak(t("imgview.keys"));
}

/// The thumbnail strip under the picture: every item as a tiny button so
/// a click jumps straight there; the current one carries the accent ring.
/// Only items actually scrolled into view request their texture.
fn draw_filmstrip(ui: &mut egui::Ui, app: &OctaApp, st: &mut ImageViewerState) {
    if st.items.len() < 2 {
        return;
    }
    let mut jump = None;
    egui::ScrollArea::horizontal()
        .id_salt("imgview_strip")
        .auto_shrink([false, false])
        .max_height(STRIP_H)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                for (i, it) in st.items.iter().enumerate() {
                    let (rect, resp) = ui.allocate_exact_size(
                        egui::vec2(STRIP_H - 10.0, STRIP_H - 10.0),
                        egui::Sense::click(),
                    );
                    if !ui.is_rect_visible(rect) {
                        continue;
                    }
                    app.image_cache.request(ui.ctx(), &it.url);
                    let frame = rect.shrink(1.0);
                    ui.painter().rect_stroke(
                        frame,
                        3.0,
                        egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
                        egui::StrokeKind::Inside,
                    );
                    if let Some(tex) = app.image_cache.texture(&it.url) {
                        let size = tex.size_vec2();
                        let scale = (frame.width() / size.x.max(size.y)).min(1.0);
                        let draw = egui::Rect::from_center_size(frame.center(), size * scale);
                        ui.painter().with_clip_rect(frame).image(
                            tex.id(),
                            draw,
                            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                            egui::Color32::WHITE,
                        );
                    }
                    if i == st.pos {
                        ui.painter().rect_stroke(
                            rect,
                            3.0,
                            egui::Stroke::new(2.0, ui.visuals().selection.stroke.color),
                            egui::StrokeKind::Inside,
                        );
                    }
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        resp.clone().on_hover_text(&it.url);
                    }
                    if resp.clicked() {
                        jump = Some(i);
                    }
                }
            });
        });
    if let Some(i) = jump {
        st.pos = i;
    }
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
/// sheet, the address itself (a real hyperlink, not just selectable text),
/// and the row's other values so the picture is never seen without its
/// context.
fn draw_details(
    ui: &mut egui::Ui,
    app: &OctaApp,
    st: &mut ImageViewerState,
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
            ui.horizontal(|ui| {
                ui.label(RichText::new(t("imgview.url")).weak());
                if icons::icon_button(ui, true, Icon::Copy, t("imgview.copy_hint")) {
                    ui.ctx().copy_text(it.url.clone());
                    st.message = Some((true, t("imgview.copied")));
                }
            });
            egui::ScrollArea::horizontal()
                .id_salt("imgview_url")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    // A real hyperlink: click opens the address, Ctrl/Cmd is
                    // not needed. Monospace so it still reads like data.
                    ui.add(egui::Hyperlink::from_label_and_url(
                        RichText::new(&it.url).monospace().size(11.0),
                        it.url.clone(),
                    ));
                });
            ui.separator();
            draw_row_details(ui, app, it);
        });
}

/// The row's other non-empty cells, so a picture can be judged against its
/// title, price and notes without leaving the viewer. Only drawn while the
/// cell still resolves to the same address - after a sort or a tab switch
/// the row index may point somewhere else entirely. Values that are URLs
/// stay live links; everything else is plain text.
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
        // A cell whose whole text is a link stays a link - the auction
        // page is one click away, not a copy-paste round-trip. Detection
        // runs on the untruncated text so a long URL still resolves.
        if let Some(url) = detect_url(text) {
            ui.add(
                egui::Hyperlink::from_label_and_url(
                    RichText::new(value).monospace().size(11.0),
                    url.to_string(),
                )
                .open_in_new_tab(true),
            );
        } else {
            ui.label(RichText::new(value).monospace().size(11.0));
        }
        shown += 1;
    }
}

#[cfg(test)]
#[path = "image_viewer_tests.rs"]
mod tests;
