use super::*;
use octa::i18n::t;

impl OctaApp {
    pub(super) fn render_start_screen(&mut self, ui: &mut egui::Ui) {
        let mut open = false;
        let mut new_table = false;
        let mut recent = None;
        egui::ScrollArea::vertical()
            .id_salt("dbviewy_start")
            .show(ui, |ui| {
                ui.add_space(32.0);
                ui.vertical_centered(|ui| {
                    if let Some(tex) = &self.welcome_logo_texture {
                        ui.image(egui::load::SizedTexture::new(tex.id(), [72.0, 72.0]));
                    }
                    ui.heading("DBviewy");
                    ui.label("CSV / Excel / SQLite / DuckDB / Parquet");
                    ui.add_space(16.0);
                });
                let width = ui.available_width().min(760.0);
                ui.add_space(8.0);
                ui.vertical_centered(|ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(width, 0.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            octa::ui::control_row::control_row(ui, |ui| {
                                open = ui
                                    .button(t("common.open"))
                                    .on_hover_text(t("file_menu.open_hint"))
                                    .clicked();
                                new_table = ui
                                    .button(t("file_menu.new_table"))
                                    .on_hover_text(t("file_menu.new_table_hint"))
                                    .clicked();
                            });
                            ui.add_space(20.0);
                            ui.heading(t("menu.recent_files"));
                            if self.recent_files.is_empty() {
                                ui.weak(t("file_menu.recent_none"));
                            }
                            for path in self.recent_files.iter().take(12) {
                                let file = std::path::Path::new(path);
                                let metadata = file.metadata().ok();
                                let name = file.file_name().unwrap_or_default().to_string_lossy();
                                let label = match &metadata {
                                    Some(m) => format!(
                                        "{}    {}",
                                        name,
                                        octa::ui::status_bar::human_size(m.len())
                                    ),
                                    None => name.to_string(),
                                };
                                if ui
                                    .add_enabled(metadata.is_some(), egui::Button::new(label))
                                    .on_hover_text(path)
                                    .on_disabled_hover_text(path)
                                    .clicked()
                                {
                                    recent = Some(file.to_path_buf());
                                }
                                ui.small(path);
                                ui.add_space(5.0);
                            }
                            ui.add_space(24.0);
                            ui.separator();
                            ui.label("Ctrl+O  /  Ctrl+F  /  Ctrl+K");
                            ui.small(format!(
                                "{}  |  {}  |  {}",
                                t("common.open"),
                                t("instant_search.placeholder"),
                                t("menu.help")
                            ));
                            ui.small(format!("SQLite / DuckDB: {}", t("table_picker.open_all")));
                            ui.small(format!(
                                "DBviewy  {}  |  {}",
                                crate::VERSION,
                                std::env::consts::ARCH
                            ));
                        },
                    );
                });
            });
        if open {
            self.open_file();
        }
        if new_table {
            self.new_table_dialog = Some(crate::app::state::NewTableState::default());
        }
        if let Some(path) = recent {
            self.enqueue_open_files(vec![path]);
        }
    }
}

#[cfg(test)]
#[path = "start_screen_tests.rs"]
mod tests;
