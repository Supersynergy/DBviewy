//! Thin wrapper around the shared `ui::table_picker` widget; dispatches its
//! [`TablePickerAction`] back to [`OctaApp::load_table`].

use eframe::egui;

use octa::ui;

use super::super::state::OctaApp;

pub(crate) fn render_table_picker(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(state) = app.pending_table_picker.as_mut() else {
        return;
    };
    let action = ui::table_picker::render_table_picker(ctx, state);
    match action {
        ui::table_picker::TablePickerAction::None => {}
        ui::table_picker::TablePickerAction::Cancel => {
            app.gallery_table_picker = false;
            app.pending_table_picker = None;
        }
        ui::table_picker::TablePickerAction::Open(path, table_name) => {
            let reader_name = state.format_name.clone();
            app.pending_table_picker = None;
            let previous_id = app.tabs[app.active_tab].id;
            app.load_table(path, table_name, Some(&reader_name));
            if std::mem::take(&mut app.gallery_table_picker)
                && app.tabs[app.active_tab].id != previous_id
            {
                app.image_viewer = None;
                app.recompute_filter();
                if let Some(&row) = app.tabs[app.active_tab].filtered_rows.first() {
                    app.open_image_viewer(row, 0);
                }
            }
        }
    }
}
