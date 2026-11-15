//! Browse the filtered grid one record at a time, including records without
//! images. Left/Right and Space walk pictures in order, Up/Down walk rows.
//! The shared image cache services the picture and visible filmstrip cells.
use eframe::egui;
use egui::RichText;

use octa::data::links::{detect_image_url, detect_url};
use octa::data::{CellValue, DataTable};
use octa::i18n::t;
use octa::ui::icons::{self, Icon};
use octa::ui::settings::{
    DialogSize, dialog_header, draw_result_message, remember_dialog_rect, size_dialog_window,
};

use crate::app::state::{ImageItem, ImageViewerState, OctaApp, TabState};

/// Zoom step per key press or button click.
const ZOOM_STEP: f32 = 1.25;
/// Details panel width; the picture keeps the rest.
const DETAILS_W: f32 = 250.0;
/// Film strip height (one thumbnail row); hidden for a single picture.
const STRIP_H: f32 = 56.0;
/// Row-detail values are cut here; the cell itself may hold a paragraph.
const DETAIL_VALUE_MAX: usize = 80;

fn thumbnail_column(name: &str) -> bool {
    let name = name.to_lowercase();
    ["thumb", "vorschau", "miniatur", "preview"]
        .iter()
        .any(|token| name.contains(token))
}

fn row_sources(table: &DataTable, row: usize) -> impl Iterator<Item = (usize, bool, &str)> {
    table
        .columns
        .iter()
        .enumerate()
        .filter_map(move |(col, column)| {
            let CellValue::String(text) = table.get(row, col)? else {
                return None;
            };
            let url = detect_image_url(text)?;
            Some((col, thumbnail_column(&column.name), url))
        })
}

fn row_images(table: &DataTable, row: usize) -> Vec<ImageItem> {
    let mut images: Vec<_> = row_sources(table, row).collect();
    // Distinct full images keep their order. A preview is a fallback when
    // the record has no full image, not a second stop for the same picture.
    if images.iter().any(|(_, preview, _)| !preview) {
        images.retain(|(_, preview, _)| !preview);
    }
    let embedded: std::collections::HashSet<_> = images
        .iter()
        .filter_map(|(_, _, url)| url.strip_prefix("dbviewy-media:"))
        .collect();
    if !embedded.is_empty() {
        use octa::data::transform::hash_columns::HashColumnsAlgo;
        images.retain(|(_, _, url)| {
            detect_url(url).is_none()
                || !embedded.contains(HashColumnsAlgo::Sha256.hex(url.as_bytes()).as_str())
        });
    }
    // In the import album, Quelle records provenance for Bild. It is not
    // another photo even when the source is a URL or a local JPEG filename.
    if table
        .db_meta
        .as_ref()
        .is_some_and(|meta| meta.table_name == octa::data::embedded_media::ALBUM)
        && images
            .iter()
            .any(|(_, _, url)| octa::data::embedded_media::is_reference(url))
    {
        images.retain(|(col, _, _)| table.columns[*col].name != "Quelle");
    }
    images.sort_by_key(|(_, _, url)| detect_url(url).is_some());
    let mut result: Vec<ImageItem> = Vec::new();
    for (col, _, url) in images {
        if !result.iter().any(|seen| seen.url == url) {
            result.push(ImageItem {
                row,
                col,
                url: url.to_owned(),
            });
        }
    }
    result
}

impl OctaApp {
    pub(crate) fn open_image_viewer(&mut self, row: usize, col: usize) {
        let tab = &self.tabs[self.active_tab];
        if tab.filtered_rows.is_empty() {
            return;
        }
        let row_pos = tab
            .filtered_rows
            .iter()
            .position(|&r| r == row)
            .unwrap_or(0);
        let row = tab.filtered_rows[row_pos];
        let items = row_images(&tab.table, row);
        let pos = items
            .iter()
            .position(|it| it.col == col)
            .filter(|_| {
                !tab.table
                    .columns
                    .get(col)
                    .is_some_and(|c| thumbnail_column(&c.name))
            })
            .unwrap_or(0);
        self.image_viewer = Some(ImageViewerState {
            size: DialogSize::Normal,
            tab_id: tab.id,
            row,
            row_pos,
            grid_col: col,
            items,
            pos,
            reveal_row: true,
            fit_scale: 1.0,
            zoom: None,
            show_details: true,
            message: None,
        });
    }
}

impl ImageViewerState {
    fn select_row(&mut self, tab: &TabState, index: usize) {
        let Some(&row) = tab.filtered_rows.get(index) else {
            return;
        };
        self.row_pos = index;
        self.row = row;
        self.items = row_images(&tab.table, row);
        self.pos = 0;
        self.message = None;
        self.reveal_row = true;
    }

    fn next_picture(&mut self, tab: &TabState, forward: bool) {
        if forward {
            if self.pos + 1 < self.items.len() {
                self.pos += 1;
            } else {
                self.select_row(tab, self.row_pos + 1);
            }
        } else if self.pos > 0 {
            self.pos -= 1;
        } else if self.row_pos > 0 {
            self.select_row(tab, self.row_pos - 1);
            self.pos = self.items.len().saturating_sub(1);
        }
    }

    fn change_zoom(&mut self, factor: f32) {
        self.zoom = Some((self.zoom.unwrap_or(self.fit_scale) * factor).clamp(0.02, 16.0));
    }
}

fn take_key(input: &mut egui::InputState, key: egui::Key, shift: bool) -> bool {
    let modifiers = if shift {
        egui::Modifiers::SHIFT
    } else {
        egui::Modifiers::NONE
    };
    let mut found = false;
    input.events.retain(|event| {
        if matches!(event, egui::Event::Key { key: k, pressed: true, modifiers: m, .. }
            if *k == key && *m == modifiers)
        {
            found = true;
            false
        } else {
            true
        }
    });
    found
}

pub(crate) fn render_image_viewer(app: &mut OctaApp, ctx: &egui::Context) {
    if app.gallery_table_picker {
        return;
    }
    let Some(mut st) = app.image_viewer.take() else {
        return;
    };
    let tab = &app.tabs[app.active_tab];
    if tab.id != st.tab_id || tab.filtered_rows.is_empty() {
        return;
    }
    if tab.filtered_rows.get(st.row_pos) != Some(&st.row) {
        let index = tab
            .filtered_rows
            .iter()
            .position(|&r| r == st.row)
            .unwrap_or_else(|| st.row_pos.min(tab.filtered_rows.len() - 1));
        st.select_row(tab, index);
    }
    if st.items.iter().any(|it| {
        !matches!(tab.table.get(it.row, it.col),
        Some(CellValue::String(text)) if detect_image_url(text) == Some(it.url.as_str()))
    }) {
        st.select_row(tab, st.row_pos);
    }
    app.image_cache.set_document(
        tab.table.source_path.as_deref().map(std::path::Path::new),
        tab.id.0,
    );
    let key = |key, shift| ctx.input_mut(|i| take_key(i, key, shift));
    use egui::Key;
    let mut close = key(Key::Escape, false);
    if key(Key::T, false)
        && tab.table.format_name.as_deref() == Some("SQLite")
        && let Some(path) = tab
            .table
            .source_path
            .as_deref()
            .map(std::path::PathBuf::from)
        && let Some(reader) = app.registry.reader_by_name("SQLite")
    {
        match reader.list_tables(&path) {
            Ok(Some(tables)) if !tables.is_empty() => {
                let selected = tab
                    .table
                    .db_meta
                    .as_ref()
                    .and_then(|meta| {
                        tables
                            .iter()
                            .position(|table| table.name == meta.table_name)
                    })
                    .unwrap_or(0);
                app.pending_table_picker = Some(octa::ui::table_picker::TablePickerState {
                    path,
                    format_name: "SQLite".into(),
                    tables,
                    selected,
                    visible_rows: app.settings.table_picker_visible_rows,
                });
                app.gallery_table_picker = true;
                app.image_viewer = Some(st);
                return;
            }
            Err(e) => st.message = Some((false, format!("{e:#}"))),
            _ => {}
        }
    }
    if key(Key::ArrowLeft, false) || key(Key::Space, true) {
        st.next_picture(tab, false);
    } else if key(Key::ArrowRight, false) || key(Key::Space, false) {
        st.next_picture(tab, true);
    } else if key(Key::ArrowUp, false) || key(Key::PageUp, false) {
        st.select_row(tab, st.row_pos.saturating_sub(1));
    } else if key(Key::ArrowDown, false) || key(Key::PageDown, false) {
        st.select_row(tab, st.row_pos + 1);
    } else if key(Key::Home, false) {
        st.select_row(tab, 0);
    } else if key(Key::End, false) {
        st.select_row(tab, tab.filtered_rows.len() - 1);
        st.pos = st.items.len().saturating_sub(1);
    }
    if key(Key::Num0, false) {
        st.zoom = None;
    } else if key(Key::Plus, false)
        || key(Key::Plus, true)
        || key(Key::Equals, false)
        || key(Key::Equals, true)
    {
        st.change_zoom(ZOOM_STEP);
    } else if key(Key::Minus, false) {
        st.change_zoom(1.0 / ZOOM_STEP);
    }
    if key(Key::D, false) {
        st.show_details = !st.show_details;
    }
    if key(Key::O, false)
        && let Some(it) = st.items.get(st.pos)
        && detect_url(&it.url).is_some()
    {
        ctx.open_url(egui::OpenUrl::new_tab(&it.url));
    }
    if let Some(it) = st.items.get(st.pos) {
        app.image_cache.request_preview(ctx, &it.url);
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
        ui.memory_mut(|m| m.set_modal_layer(ui.layer_id()));
        close |= dialog_header(
            ui,
            "imgview_header",
            RichText::new(format!(
                "{} - {}/{}",
                t("imgview.title"),
                st.row_pos + 1,
                tab.filtered_rows.len()
            )),
            &mut size,
        );
        if size == DialogSize::Minimized {
            return;
        }
        draw_toolbar(ui, tab, &mut st, &mut close);
        egui::Panel::bottom("imgview_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                if let Some((ok, msg)) = &st.message {
                    draw_result_message(ui, *ok, msg);
                }
            });
        let item = st.items.get(st.pos).cloned();
        draw_body(ui, app, &mut st, &item);
    });
    if let Some(inner) = inner.as_ref() {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    st.size = size;

    if close {
        let tab = &mut app.tabs[app.active_tab];
        let col = st.items.get(st.pos).map_or(st.grid_col, |it| it.col);
        tab.table_state.selected_cell =
            Some((st.row, col.min(tab.table.col_count().saturating_sub(1))));
        tab.table_state.scroll_to_row(st.row_pos);
        ctx.memory_mut(|m| {
            if let Some(id) = m.focused() {
                m.surrender_focus(id);
            }
        });
    } else {
        app.image_viewer = Some(st);
    }
}

/// Prev/next, zoom and the way out, in one control row. Icons are painted,
/// not glyphs, so no font can lack them; the hover names the action and
/// its keys, so the shortcuts stay discoverable.
fn draw_toolbar(ui: &mut egui::Ui, tab: &TabState, st: &mut ImageViewerState, close: &mut bool) {
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
        first = icons::icon_button(
            ui,
            st.row_pos > 0 || st.pos > 0,
            Icon::First,
            t("imgview.first_hint"),
        );
        prev = icons::icon_button(
            ui,
            st.row_pos > 0 || st.pos > 0,
            Icon::Prev,
            t("imgview.prev_hint"),
        );
        next = icons::icon_button(
            ui,
            st.row_pos + 1 < tab.filtered_rows.len() || st.pos + 1 < st.items.len(),
            Icon::Next,
            t("imgview.next_hint"),
        );
        last = icons::icon_button(
            ui,
            st.row_pos + 1 < tab.filtered_rows.len() || st.pos + 1 < st.items.len(),
            Icon::Last,
            t("imgview.last_hint"),
        );
        ui.separator();
        zoom_out = icons::icon_button(
            ui,
            st.zoom.unwrap_or(st.fit_scale) > 0.02,
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
        open_url = icons::icon_button(
            ui,
            st.items
                .get(st.pos)
                .is_some_and(|it| detect_url(&it.url).is_some()),
            Icon::OpenUrl,
            t("imgview.open_url_hint"),
        );
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
        st.select_row(tab, 0);
    }
    if prev {
        st.next_picture(tab, false);
    }
    if next {
        st.next_picture(tab, true);
    }
    if last {
        st.select_row(tab, tab.filtered_rows.len() - 1);
        st.pos = st.items.len().saturating_sub(1);
    }
    if zoom_in {
        st.change_zoom(ZOOM_STEP);
    }
    if zoom_out {
        st.change_zoom(1.0 / ZOOM_STEP);
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
    let tab = &app.tabs[app.active_tab];
    let strip_h = if tab.filtered_rows.len() > 1 {
        STRIP_H
    } else {
        0.0
    };
    let hint_h = ui.text_style_height(&egui::TextStyle::Small) + 8.0;
    let viewport = egui::vec2(
        ui.available_width(),
        (ui.available_height() - strip_h - hint_h).max(40.0),
    );
    let texture = item
        .as_ref()
        .and_then(|it| app.image_cache.preview_texture(&it.url));
    let failure = item
        .as_ref()
        .and_then(|it| app.image_cache.preview_failure(&it.url));
    let origin = ui.cursor().min;
    let salt = ("imgview_pan", st.row, st.pos);
    let id = ui.make_persistent_id(salt);
    if let Some(tex) = &texture {
        let natural = tex.size_vec2();
        st.fit_scale = fit_zoom(viewport, natural);
        let zoom_delta = ui.input(|i| i.zoom_delta());
        if zoom_delta != 1.0
            && let Some(pointer) = ui.ctx().pointer_hover_pos()
            && egui::Rect::from_min_size(origin, viewport).contains(pointer)
        {
            let old = st.zoom.unwrap_or(st.fit_scale);
            st.change_zoom(zoom_delta);
            let new = st.zoom.unwrap_or(old);
            let mut state = egui::scroll_area::State::load(ui.ctx(), id).unwrap_or_default();
            let at = pointer - origin;
            let old_padding = (viewport - natural * old).max(egui::Vec2::ZERO) * 0.5;
            let new_padding = (viewport - natural * new).max(egui::Vec2::ZERO) * 0.5;
            state.offset = ((state.offset + at - old_padding) * (new / old) + new_padding - at)
                .max(egui::Vec2::ZERO);
            state.store(ui.ctx(), id);
        }
    }
    egui::ScrollArea::both()
        .id_salt(salt)
        .auto_shrink([false, false])
        .scroll_source(egui::scroll_area::ScrollSource::ALL)
        .max_width(viewport.x)
        .max_height(viewport.y)
        .show(ui, |ui| match (texture, failure) {
            (Some(tex), _) => {
                let size = tex.size_vec2() * st.zoom.unwrap_or(st.fit_scale);
                let (rect, resp) = ui.allocate_exact_size(size.max(viewport), egui::Sense::click());
                ui.painter().image(
                    tex.id(),
                    egui::Rect::from_center_size(rect.center(), size),
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
                if resp.double_clicked() {
                    st.zoom = if st.zoom.is_none() { Some(1.0) } else { None };
                }
            }
            (None, Some(err)) => {
                ui.label(RichText::new(t("imgview.failed")).color(ui.visuals().error_fg_color));
                ui.label(RichText::new(err).weak().monospace());
                if item.as_ref().is_some_and(|it| {
                    detect_url(&it.url).is_none()
                        && !octa::data::embedded_media::is_reference(&it.url)
                }) && ui.button(t("dialog.dbx_choose")).clicked()
                    && let Some(dir) = rfd::FileDialog::new().pick_folder()
                    && let Err(e) = app.image_cache.allow_local_directory(&dir)
                {
                    st.message = Some((false, e.to_string()));
                }
            }
            (None, None) if item.is_none() => {
                ui.weak(t("imgview.no_image_cell"));
            }
            (None, None) => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(t("imgview.loading"));
                });
            }
        });
    draw_filmstrip(ui, app, st);
    ui.weak(t("imgview.keys"));
}

/// Only visible rows are inspected and painted. Use the preview image in
/// the strip when the same record also provides a full-sized picture.
fn draw_filmstrip(ui: &mut egui::Ui, app: &OctaApp, st: &mut ImageViewerState) {
    let tab = &app.tabs[app.active_tab];
    let count = tab.filtered_rows.len();
    if count < 2 {
        return;
    }
    let mut jump = None;
    egui::ScrollArea::horizontal()
        .id_salt("imgview_strip")
        .auto_shrink([false, false])
        .max_height(STRIP_H)
        .show_viewport(ui, |ui, viewport| {
            let side = STRIP_H - 10.0;
            let stride = side + ui.spacing().item_spacing.x;
            let (strip, _) = ui.allocate_exact_size(
                egui::vec2(count as f32 * stride, side),
                egui::Sense::hover(),
            );
            let tile = |i| {
                egui::Rect::from_min_size(
                    strip.min + egui::vec2(i as f32 * stride, 0.0),
                    egui::Vec2::splat(side),
                )
            };
            if st.reveal_row {
                ui.scroll_to_rect(tile(st.row_pos), None);
                st.reveal_row = false;
            }
            let first = (viewport.left() / stride).floor().max(0.0) as usize;
            let last = ((viewport.right() / stride).ceil() as usize + 1).min(count);
            for i in first..last {
                let rect = tile(i);
                let resp = ui.interact(rect, ui.id().with(i), egui::Sense::click());
                let preview =
                    row_sources(&tab.table, tab.filtered_rows[i]).min_by_key(|(_, thumb, url)| {
                        (
                            !octa::data::embedded_media::is_reference(url),
                            !thumb,
                            detect_url(url).is_some(),
                        )
                    });
                let frame = rect.shrink(2.0);
                let texture = preview.and_then(|(_, _, url)| {
                    app.image_cache.request(ui.ctx(), url);
                    app.image_cache.texture(url)
                });
                if let Some(tex) = texture {
                    let draw = egui::Rect::from_center_size(
                        frame.center(),
                        tex.size_vec2() * fit_zoom(frame.size(), tex.size_vec2()),
                    );
                    ui.painter().image(
                        tex.id(),
                        draw,
                        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                } else {
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        (i + 1).to_string(),
                        egui::FontId::proportional(11.0),
                        ui.visuals().weak_text_color(),
                    );
                }
                let stroke = if i == st.row_pos {
                    ui.visuals().selection.stroke
                } else {
                    ui.visuals().widgets.noninteractive.bg_stroke
                };
                ui.painter()
                    .rect_stroke(rect, 3.0, stroke, egui::StrokeKind::Inside);
                if resp
                    .on_hover_text(format!("{} {}", t("imgview.row"), tab.filtered_rows[i] + 1))
                    .clicked()
                {
                    jump = Some(i);
                }
            }
        });
    if let Some(i) = jump {
        st.select_row(tab, i);
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
    let tab = &app.tabs[app.active_tab];
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.strong(t("imgview.details"));
            ui.separator();
            ui.label(format!("{}: {}", t("imgview.row"), st.row + 1));
            if let Some(it) = item {
                if let Some(col) = tab.table.columns.get(it.col) {
                    ui.label(format!("{}: {}", t("imgview.column"), col.name));
                }
                ui.label(format!(
                    "{}: {}/{}",
                    t("imgview.position"),
                    st.pos + 1,
                    st.items.len()
                ));
                if let Some(tex) = app.image_cache.preview_texture(&it.url) {
                    let [w, h] = tex.size();
                    ui.label(format!("{}: {w} x {h}", t("imgview.size")));
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
                        let label = RichText::new(&it.url).monospace().size(11.0);
                        if detect_url(&it.url).is_some() {
                            ui.add(egui::Hyperlink::from_label_and_url(label, it.url.clone()));
                        } else {
                            ui.label(label);
                        }
                    });
                ui.separator();
            }
            let selected = item.as_ref().map(|it| it.col);
            draw_row_links(ui, app, st.row, selected);
            draw_row_details(ui, app, st.row, selected);
        });
}

/// Every other link column of the row as a clickable address: the lot's
/// product page is the usual one, but any URL cell joins in, capped by
/// screen space rather than by which columns happen to sit early.
fn draw_row_links(ui: &mut egui::Ui, app: &OctaApp, row: usize, selected: Option<usize>) {
    const LINK_DISPLAY_MAX: usize = 55;
    let tab = &app.tabs[app.active_tab];
    let mut header_drawn = false;
    for c in 0..tab.table.col_count() {
        if Some(c) == selected {
            continue;
        }
        let Some(CellValue::String(s)) = tab.table.get(row, c) else {
            continue;
        };
        let text = s.trim();
        let Some(url) = detect_url(text) else {
            continue;
        };
        if !header_drawn {
            ui.label(RichText::new(t("imgview.links")).weak());
            header_drawn = true;
        }
        let name = tab
            .table
            .columns
            .get(c)
            .map(|col| col.name.clone())
            .unwrap_or_else(|| c.to_string());
        let mut value: String = url.chars().take(LINK_DISPLAY_MAX).collect();
        if url.chars().count() > LINK_DISPLAY_MAX {
            value.push_str("...");
        }
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("{name}:"))
                    .weak()
                    .monospace()
                    .size(11.0),
            );
            // Tooltip is the address itself - no key needed, the user reads
            // where the click will land.
            ui.add(
                egui::Hyperlink::from_label_and_url(
                    RichText::new(value).monospace().size(11.0),
                    url.to_string(),
                )
                .open_in_new_tab(true),
            )
            .on_hover_text(url.to_string());
        });
    }
    if header_drawn {
        ui.separator();
    }
}

/// The row's other non-empty cells, so a picture can be judged against its
/// title, price and notes without leaving the viewer. Only drawn while the
/// cell still resolves to the same address - after a sort or a tab switch
/// the row index may point somewhere else entirely. Values that are URLs
/// stay live links; everything else is plain text.
fn draw_row_details(ui: &mut egui::Ui, app: &OctaApp, row: usize, selected: Option<usize>) {
    let tab = &app.tabs[app.active_tab];
    for c in 0..tab.table.col_count() {
        if Some(c) == selected {
            continue;
        }
        let Some(v) = tab.table.get(row, c) else {
            continue;
        };
        let text = v.display_with_binary_mode(app.settings.binary_display_mode);
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        // Pure link cells live in the Links section above; repeating them
        // here would just spend one of the few detail slots.
        if detect_url(text).is_some() {
            continue;
        }
        let name = tab
            .table
            .columns
            .get(c)
            .map(|col| col.name.clone())
            .unwrap_or_else(|| c.to_string());
        if text.chars().nth(DETAIL_VALUE_MAX).is_some() {
            let summary: String = text.chars().take(DETAIL_VALUE_MAX).collect();
            egui::CollapsingHeader::new(format!("{name}: {summary}..."))
                .id_salt((tab.id.0, row, c))
                .show(ui, |ui| {
                    ui.add(egui::Label::new(text).wrap().selectable(true));
                });
        } else {
            ui.label(
                RichText::new(format!("{name}: {text}"))
                    .weak()
                    .monospace()
                    .size(11.0),
            );
        }
    }
}

#[cfg(test)]
#[path = "image_viewer_tests.rs"]
mod tests;
