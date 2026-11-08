//! "Enrich from web..." dialog: configure a column enrichment run before
//! it starts - what the cells mean (URL to fetch, or query to search), what
//! to extract from each page, where the values land and how far the run
//! may go. Opened from the column-header right-click menu on text columns.
//!
//! The dialog only collects choices; validation complaints are shown
//! inline so a bad regex never reaches the worker. Start hands the run to
//! [`crate::app::enrich`] whose status-bar pill then shows progress and
//! offers Cancel.

use eframe::egui;

use octa::ui::settings::{DialogSize, dialog_header, remember_dialog_rect, size_dialog_window};

use crate::app::enrich::GuiExtract;

use super::super::state::OctaApp;

pub(crate) fn render_enrich_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(col_idx) = app.tabs[app.active_tab]
        .enrich_dialog
        .as_ref()
        .map(|d| d.col_idx)
    else {
        return;
    };
    // Guard against the table changing shape while the dialog is open.
    let Some(col_name) = app.tabs[app.active_tab]
        .table
        .columns
        .get(col_idx)
        .map(|c| c.name.clone())
    else {
        app.tabs[app.active_tab].enrich_dialog = None;
        return;
    };

    let mut close = false;
    let mut start = false;

    let dialog_id = egui::Id::new("octa_enrich_dialog");
    let size_key = dialog_id.with("octa_dlg_size");
    let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key).unwrap_or_default());
    let minimized = size == DialogSize::Minimized;

    let window = egui::Window::new("octa_enrich")
        .id(dialog_id)
        .title_bar(false)
        .collapsible(false)
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(ctx.content_rect().center());
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable([true, true])
            .default_width(340.0)
            .default_height(400.0)
            .min_width(300.0)
            .min_height(280.0)
    });

    let inner = window.show(ctx, |ui| {
        close |= dialog_header(
            ui,
            "octa_enrich_header",
            egui::RichText::new(format!(
                "{} - {col_name}",
                octa::i18n::t("header.enrich_column").trim_end_matches('.')
            )),
            &mut size,
        );

        if minimized {
            return;
        }

        // Footer first so Start/Cancel stay pinned no matter how tall the
        // controls grow.
        egui::Panel::bottom("octa_enrich_footer").show(ui, |ui| {
            ui.add_space(6.0);
            octa::ui::control_row::control_line(ui, |ui| {
                if ui
                    .button(octa::i18n::t("enrich_dialog.start"))
                    .on_hover_text(octa::i18n::t("enrich_dialog.start_hint"))
                    .clicked()
                {
                    start = true;
                }
                if ui.button(octa::i18n::t("common.cancel")).clicked() {
                    close = true;
                }
            });
        });

        egui::CentralPanel::default().show(ui, |ui| {
            let dlg = app.tabs[app.active_tab].enrich_dialog.as_mut().unwrap();

            // What the cells mean. Text + Search need the pattern field
            // enabled differently: Pattern requires it, Search makes it
            // optional (without one the page text is stored).
            ui.label(octa::i18n::t("enrich_dialog.mode"));
            octa::ui::control_row::control_line(ui, |ui| {
                for (mode, key) in [
                    (GuiExtract::Status, "enrich_dialog.mode_status"),
                    (GuiExtract::Text, "enrich_dialog.mode_text"),
                    (GuiExtract::Pattern, "enrich_dialog.mode_pattern"),
                    (GuiExtract::Search, "enrich_dialog.mode_search"),
                ] {
                    ui.radio_value(&mut dlg.mode, mode, octa::i18n::t(key))
                        .on_hover_text(octa::i18n::t(&format!("{key}_hint")));
                }
            });

            ui.add_space(6.0);
            octa::ui::control_row::control_line(ui, |ui| {
                ui.label(octa::i18n::t("enrich_dialog.pattern"));
                let enabled = matches!(dlg.mode, GuiExtract::Pattern | GuiExtract::Search);
                ui.add_enabled(
                    enabled,
                    egui::TextEdit::singleline(&mut dlg.pattern_buf).desired_width(180.0),
                )
                .on_hover_text(octa::i18n::t("enrich_dialog.pattern_hint"));
            });

            octa::ui::control_row::control_line(ui, |ui| {
                ui.label(octa::i18n::t("enrich_dialog.target"));
                ui.add(egui::TextEdit::singleline(&mut dlg.target_buf).desired_width(180.0))
                    .on_hover_text(octa::i18n::t("enrich_dialog.target_hint"));
            });

            ui.add_space(6.0);
            octa::ui::control_row::control_line(ui, |ui| {
                ui.label(octa::i18n::t("enrich_dialog.limit"));
                ui.add(egui::TextEdit::singleline(&mut dlg.limit_buf).desired_width(56.0))
                    .on_hover_text(octa::i18n::t("enrich_dialog.limit_hint"));
            });
            ui.add_enabled_ui(dlg.mode == GuiExtract::Search, |ui| {
                octa::ui::control_row::control_line(ui, |ui| {
                    ui.label(octa::i18n::t("enrich_dialog.top_k"));
                    ui.add(egui::TextEdit::singleline(&mut dlg.top_k_buf).desired_width(56.0))
                        .on_hover_text(octa::i18n::t("enrich_dialog.top_k_hint"));
                });
            });
            ui.add_enabled_ui(
                matches!(dlg.mode, GuiExtract::Text | GuiExtract::Search),
                |ui| {
                    octa::ui::control_row::control_line(ui, |ui| {
                        ui.label(octa::i18n::t("enrich_dialog.cell_chars"));
                        ui.add(
                            egui::TextEdit::singleline(&mut dlg.cell_chars_buf).desired_width(56.0),
                        )
                        .on_hover_text(octa::i18n::t("enrich_dialog.cell_chars_hint"));
                    });
                },
            );

            ui.add_space(6.0);
            ui.checkbox(
                &mut dlg.provenance,
                octa::i18n::t("enrich_dialog.provenance"),
            )
            .on_hover_text(octa::i18n::t("enrich_dialog.provenance_hint"));
            ui.checkbox(&mut dlg.refresh, octa::i18n::t("enrich_dialog.refresh"))
                .on_hover_text(octa::i18n::t("enrich_dialog.refresh_hint"));
            ui.checkbox(
                &mut dlg.only_missing,
                octa::i18n::t("enrich_dialog.only_missing"),
            )
            .on_hover_text(octa::i18n::t("enrich_dialog.only_missing_hint"));

            if let Some(err) = &dlg.error {
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(err.clone())
                        .color(ui.visuals().error_fg_color)
                        .small(),
                );
            }
        });
    });

    if let Some(inner) = inner {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    ctx.data_mut(|d| {
        d.insert_temp(
            size_key,
            if close || start {
                DialogSize::Normal
            } else {
                size
            },
        )
    });

    if start {
        // start_enrich takes the dialog state out; a validation miss puts it
        // back with `error` set, so the window simply stays open.
        app.start_enrich();
    } else if close {
        app.tabs[app.active_tab].enrich_dialog = None;
    }
}
