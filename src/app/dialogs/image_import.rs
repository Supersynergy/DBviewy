//! Import explicitly selected files or a pasted web address into the open SQLite database.
use crate::app::pushdown::{ServerTask, spawn_local_fallible};
use crate::app::state::OctaApp;
use crate::app::tabs::TabId;
use eframe::egui;
use octa::data::embedded_media::{self, Source};
use octa::i18n::t;
use std::path::{Path, PathBuf};

pub(crate) struct ImageImport {
    tab: TabId,
    database: PathBuf,
    url: String,
    job: Option<ServerTask<String>>,
    error: Option<String>,
}

impl OctaApp {
    pub(crate) fn open_image_import(&mut self, row: usize, column: usize) {
        if self.is_readonly() || self.image_import.as_ref().is_some_and(|s| s.job.is_some()) {
            return;
        }
        let tab = &self.tabs[self.active_tab];
        if tab.table.format_name.as_deref() != Some("SQLite") {
            return;
        }
        let Some(path) = tab.table.source_path.as_ref() else {
            return;
        };
        let url = tab
            .table
            .get(row, column)
            .and_then(|v| match v {
                octa::data::CellValue::String(s) => octa::data::links::detect_url(s),
                _ => None,
            })
            .unwrap_or_default()
            .to_owned();
        self.image_import = Some(ImageImport {
            tab: tab.id,
            database: PathBuf::from(path),
            url,
            job: None,
            error: None,
        });
    }
}

pub(crate) fn render(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(mut state) = app.image_import.take() else {
        return;
    };
    if let Some(task) = &state.job {
        match task.poll_result() {
            Some(Ok(_)) => {
                app.load_table(
                    state.database,
                    embedded_media::ALBUM.to_owned(),
                    Some("SQLite"),
                );
                return;
            }
            Some(Err(error)) => {
                state.error = Some(error);
                state.job = None;
            }
            None => ctx.request_repaint_after(std::time::Duration::from_millis(75)),
        }
    }
    let mut keep = true;
    let mut source = None;
    let mut cancelled = false;
    let tab = &app.tabs[app.active_tab];
    let writable = tab.id == state.tab
        && tab.table.source_path.as_deref().map(Path::new) == Some(state.database.as_path())
        && !app.is_readonly();
    egui::Window::new(t("imgview.import"))
        .id(egui::Id::new("dbviewy_image_import"))
        .collapsible(false)
        .resizable(true)
        .open(&mut keep)
        .show(ctx, |ui| {
            ui.label(format!(
                "{}  /  {}",
                state
                    .database
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy(),
                embedded_media::ALBUM
            ));
            ui.add_enabled_ui(writable && state.job.is_none(), |ui| {
                if ui.button(t("dialog.swb_browse")).clicked() {
                    source = rfd::FileDialog::new()
                        .add_filter(
                            &t("imgview.title"),
                            &["jpg", "jpeg", "png", "webp", "gif", "svg"],
                        )
                        .pick_file()
                        .map(Source::File);
                }
                ui.horizontal(|ui| {
                    ui.label("URL");
                    ui.text_edit_singleline(&mut state.url);
                    if ui
                        .add_enabled(
                            octa::data::links::detect_url(&state.url).is_some(),
                            egui::Button::new(t("common.save")),
                        )
                        .clicked()
                    {
                        source = Some(Source::Url(state.url.trim().to_owned()));
                    }
                });
            });
            if state.job.is_some() {
                ui.spinner();
            }
            if let Some(error) = &state.error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            cancelled = ui
                .add_enabled(state.job.is_none(), egui::Button::new(t("common.cancel")))
                .clicked();
        });
    if let Some(source) = source {
        let path = state.database.clone();
        state.job = Some(spawn_local_fallible(move || {
            embedded_media::import(&path, source)
        }));
        state.error = None;
        ctx.request_repaint();
    }
    if state.job.is_some() || (keep && !cancelled) {
        app.image_import = Some(state);
    }
}
