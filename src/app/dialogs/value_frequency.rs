//! Value-frequency dialog: top-N most common values in a column, with
//! their counts and percentages. Opened by:
//!
//! - Column-header right-click -> "Value frequency..."
//! - **Analyse -> Value frequency...** (via the column picker)
//! - `ShortcutAction::ColumnValueFrequency` (default Ctrl+Shift+I), which
//!   targets the column of the currently selected cell.
//!
//! Compute lives in `octa::data::value_frequency`; this file only renders.
//! The controls (Top-N / binning / bin count) are drawn *before* the result
//! is computed, so edits take effect in the same frame.

use eframe::egui;
use egui::RichText;
use egui_extras::{Column, TableBuilder};
use std::sync::Arc;
use std::time::Instant;

use octa::data::value_frequency::{
    BinningMode, ValueFrequency, WordMode, compute_value_frequency, compute_word_frequency,
};
use octa::data::{CellValue, ColumnInfo, DataTable};
use octa::ui::settings::{
    DialogSize, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use super::super::state::{OctaApp, TabState};
use crate::app::pushdown::ServerVf;

/// Top-N presets shown in the toolbar. `None` means "all distinct values".
/// Labels are built at render time via `t("dialog.vf_top")` + the number.
const TOP_N_PRESETS: &[Option<usize>] = &[Some(20), Some(50), Some(100), Some(500), None];

pub(crate) fn render_value_frequency_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    let active = app.active_tab;
    let Some(col_idx) = app.tabs[active].value_frequency_col else {
        return;
    };

    // Guard: the active tab might have lost the column (load_file replaced
    // the table while the dialog flag persisted). Close cleanly in that
    // case rather than rendering against stale state.
    let col_count = app.tabs[active].table.col_count();
    if col_idx >= col_count {
        app.tabs[active].value_frequency_col = None;
        app.value_frequency_closed(active);
        return;
    }

    let top_n = app.tabs[active].value_frequency_top_n;
    let bin = app.tabs[active].value_frequency_bin_numeric;
    let mut size = app.tabs[active].value_frequency_size;
    let mut top_n_state = top_n;
    let mut bin_state = bin;
    let mut bins_buf = app.tabs[active].value_frequency_bins_buf.clone();
    let mut words_state = app.tabs[active].value_frequency_words;
    let mut close_requested = false;
    let mut copy_payload: Option<String> = None;
    let mut grid_actions = FrequencyActions::default();
    let mut detached = None;
    let mut run_local = false;
    // On a server tab every Bins value is a query, so the field commits on
    // Enter or when it loses focus; a file tab recounts live as it types.
    let on_server = crate::app::pushdown::server_source_for(
        &app.tabs[active],
        app.settings.db_pushdown,
        &app.settings.db_connections,
    )
    .is_some();
    let unsaved_edits = app.tabs[active].table.is_modified();
    let mut committed_bins = app.tabs[active].value_frequency_bins;

    let (column_name, is_numeric) = {
        let tab = &app.tabs[active];
        let col = &tab.table.columns[col_idx];
        (
            col.name.clone(),
            octa::data::value_frequency::can_bin(&col.data_type),
        )
    };

    let dialog_id = egui::Id::new("octa_value_frequency_dialog");
    let build_size = size;
    let window = egui::Window::new("Value Frequency")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, build_size, window, |w| {
        // Wider than the other dialogs on purpose: this is the only one whose
        // title carries a value (the column name), so a narrow default put long
        // names under the window buttons. The truncating title below is what
        // actually guarantees they never collide; this just means the common
        // case does not need truncating at all.
        w.resizable(true)
            .default_width(720.0)
            .default_height(520.0)
            .min_width(420.0)
            .min_height(220.0)
    });
    let minimized = size == DialogSize::Minimized;

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("value_frequency_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                // Window controls are laid out FIRST, right to left, so they
                // always claim their space; the title then truncates into
                // whatever is left. Drawing the title first (as the other
                // dialogs do) is safe only for their short static titles - this
                // one appends the column name, which has no length limit, and a
                // long one used to run underneath the close button.
                octa::ui::control_row::control_line(ui, |ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if draw_window_controls(ui, &mut size) {
                            close_requested = true;
                        }
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(format!(
                                        "{} - {}",
                                        octa::i18n::t("dialog.vf_title"),
                                        column_name
                                    ))
                                    .strong()
                                    .size(16.0),
                                )
                                .truncate(),
                            )
                            .on_hover_text(&column_name);
                        });
                    });
                });
            });

        if minimized {
            return;
        }

        // Controls first, so the result below reflects this frame's edits.
        egui::Panel::top("value_frequency_controls")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                let binning_active = is_numeric && bin_state;
                octa::ui::control_row::control_row(ui, |ui| {
                    // Top-N only applies to raw value counts; when binning, the
                    // bin count is the control, so hide the presets.
                    if !binning_active {
                        ui.label(octa::i18n::t("dialog.vf_show"));
                        for preset in TOP_N_PRESETS {
                            let label = match preset {
                                Some(n) => format!("{} {}", octa::i18n::t("dialog.vf_top"), n),
                                None => octa::i18n::t("dialog.sel_all"),
                            };
                            let selected = top_n_state == *preset;
                            if ui.selectable_label(selected, label).clicked() {
                                top_n_state = *preset;
                            }
                        }
                    }
                    // Words are a question about text. The control stays in
                    // view on a number column, greyed with the reason.
                    ui.add_space(8.0);
                    ui.label(octa::i18n::t("dialog.vf_count_what"))
                        .on_hover_text(octa::i18n::t("dialog.vf_count_what_hint"));
                    ui.add_enabled_ui(!is_numeric, |ui| {
                        egui::ComboBox::from_id_salt("value_frequency_words")
                            .selected_text(octa::i18n::t(words_state.label_key()))
                            .show_ui(ui, |ui| {
                                for m in WordMode::ALL {
                                    ui.selectable_value(
                                        &mut words_state,
                                        m,
                                        octa::i18n::t(m.label_key()),
                                    );
                                }
                            })
                            .response
                            .on_hover_text(octa::i18n::t("dialog.vf_count_what_hint"))
                            .on_disabled_hover_text(octa::i18n::t("dialog.vf_words_text_only"));
                    });
                    if is_numeric {
                        if !binning_active {
                            ui.add_space(8.0);
                        }
                        ui.checkbox(&mut bin_state, octa::i18n::t("dialog.vf_bin_numeric"));
                        if bin_state {
                            ui.add_space(4.0);
                            ui.label(octa::i18n::t("dialog.vf_bins"));
                            let mut hint = octa::i18n::t("dialog.vf_bins_hint");
                            if on_server {
                                hint =
                                    format!("{hint}\n{}", octa::i18n::t("pushdown.vf_bins_hint"));
                            }
                            let resp = ui
                                .add(
                                    egui::TextEdit::singleline(&mut bins_buf)
                                        .desired_width(48.0)
                                        .hint_text(octa::i18n::t("dialog.vf_auto")),
                                )
                                .on_hover_text(hint);
                            if !on_server || resp.lost_focus() {
                                committed_bins = parse_bins(&bins_buf);
                            }
                            ui.label(
                                egui::RichText::new(octa::i18n::t("dialog.vf_ranges_note"))
                                    .small()
                                    .color(ui.visuals().weak_text_color()),
                            );
                        }
                    }
                });
            });

        // Compute the result from the live control values (no frame lag).
        let binning_mode = if is_numeric && bin_state {
            match committed_bins {
                Some(n) => BinningMode::Custom(n),
                None => BinningMode::Sturges,
            }
        } else {
            BinningMode::None
        };
        // A partial live-database tab counts on the server; every other tab
        // counts its rows in memory, every frame, as before.
        let mut server_total: Option<usize> = None;
        let by_words = !is_numeric && words_state != WordMode::Off;
        let server = if by_words {
            // Words are counted in the rows the tab holds, on every kind of
            // tab: no engine splits text the same way.
            ServerVf::Local
        } else {
            app.server_value_frequency(ctx, active, col_idx, top_n_state, binning_mode)
        };
        let freq = match server {
            ServerVf::Local if by_words => {
                let Some(f) = compute_word_frequency(
                    &app.tabs[active].table,
                    col_idx,
                    top_n_state,
                    words_state,
                ) else {
                    return;
                };
                Arc::new(f)
            }
            ServerVf::Local => {
                let Some(f) = compute_value_frequency(
                    &app.tabs[active].table,
                    col_idx,
                    top_n_state,
                    binning_mode,
                ) else {
                    // Bounds already checked above; defensive only.
                    return;
                };
                Arc::new(f)
            }
            ServerVf::Ready(f, total) => {
                server_total = Some(total);
                f
            }
            waiting => {
                // No result to show yet, or none coming: the status, then a
                // row with Close (the dialog stays closable while counting)
                // and, after an error or a Cancel, the loaded rows.
                ui.add_space(12.0);
                match &waiting {
                    ServerVf::Pending => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(octa::i18n::t("pushdown.vf_counting"));
                        });
                    }
                    ServerVf::Cancelled => {
                        ui.label(octa::i18n::t("pushdown.vf_cancelled"));
                    }
                    ServerVf::Failed(msg) => {
                        ui.label(octa::i18n::t("pushdown.error_title"));
                        let colour = ui.visuals().error_fg_color;
                        octa::ui::message::selectable_message(ui, colour, msg);
                    }
                    _ => {}
                }
                ui.add_space(12.0);
                octa::ui::control_row::control_row(ui, |ui| {
                    if !matches!(waiting, ServerVf::Pending)
                        && ui
                            .button(octa::i18n::t("pushdown.run_local"))
                            .on_hover_text(octa::i18n::t("pushdown.vf_run_local_hint"))
                            .clicked()
                    {
                        run_local = true;
                    }
                    if ui
                        .button(octa::i18n::t("common.close"))
                        .on_hover_text(octa::i18n::t("pushdown.vf_close_hint"))
                        .clicked()
                    {
                        close_requested = true;
                    }
                });
                return;
            }
        };

        egui::Panel::bottom("value_frequency_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show(ui, |ui| {
                octa::ui::control_row::control_row(ui, |ui| {
                    if ui.button(octa::i18n::t("common.close")).clicked() {
                        close_requested = true;
                    }
                    if ui.button(octa::i18n::t("dialog.vf_copy_tsv")).clicked() {
                        match build_tsv(&column_name, &freq) {
                            Ok(tsv) => copy_payload = Some(tsv),
                            Err(err) => {
                                tracing::error!("Could not copy value frequencies: {err:#}")
                            }
                        }
                    }
                    if frequency_result_button(ui, freq.rows.len()).clicked() {
                        detached = Some(FrequencySnapshot {
                            report: Arc::clone(&freq),
                            by_words,
                            server_total,
                        });
                    }
                    ui.label(
                        RichText::new(format!(
                            "{} {} | {} {} | {} {}{}",
                            freq.unique_count,
                            octa::i18n::t("dialog.vf_distinct"),
                            freq.total_non_null,
                            octa::i18n::t("dialog.vf_non_null"),
                            freq.nulls,
                            octa::i18n::t("dialog.vf_null"),
                            if freq.binned {
                                format!(" | {}", octa::i18n::t("dialog.vf_binned"))
                            } else {
                                String::new()
                            }
                        ))
                        .size(10.0)
                        .color(ui.visuals().weak_text_color()),
                    );
                    if by_words && (on_server || app.tabs[active].source_has_more()) {
                        ui.label(
                            RichText::new(octa::i18n::t("dialog.vf_words_loaded_rows"))
                                .size(10.0)
                                .color(ui.visuals().weak_text_color()),
                        );
                    }
                    if let Some(total) = server_total {
                        ui.label(
                            RichText::new(
                                octa::i18n::t("pushdown.vf_server").replace(
                                    "{total}",
                                    &octa::ui::status_bar::format_number(total),
                                ),
                            )
                            .size(10.0)
                            .color(ui.visuals().weak_text_color()),
                        );
                        if unsaved_edits {
                            ui.label(
                                RichText::new(octa::i18n::t("pushdown.note_unsaved"))
                                    .size(10.0)
                                    .color(ui.visuals().weak_text_color()),
                            );
                        }
                    }
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::default())
            .show(ui, |ui| {
                if freq.rows.is_empty() {
                    ui.add_space(12.0);
                    ui.label(
                        RichText::new(octa::i18n::t("dialog.vf_no_values"))
                            .color(ui.visuals().weak_text_color()),
                    );
                    return;
                }

                draw_frequency_grid(ui, &freq, by_words, &mut grid_actions);
            });
    });

    if let Some(inner) = inner {
        remember_dialog_rect(ctx, dialog_id, build_size, inner.response.rect);
    }

    if let Some(payload) = copy_payload.or(grid_actions.copy_value) {
        ctx.copy_text(payload);
        app.status_message = Some((octa::i18n::t("dialog.vf_copied"), std::time::Instant::now()));
    }

    if let Some(value) = grid_actions.filter_to_this {
        // Add a column filter limiting the active tab to this exact value.
        // Same path as the column-filter dialog's apply step.
        let tab = &mut app.tabs[active];
        let mut allow = std::collections::HashSet::new();
        allow.insert(value);
        tab.column_filters.insert(col_idx, allow);
        tab.filter_dirty = true;
    }

    let new_tab =
        detached.and_then(
            |snapshot| match frequency_result_tab(app, active, &snapshot) {
                Ok(tab) => Some(tab),
                Err(err) => {
                    app.status_message = Some((
                        format!("{}: {err:#}", octa::i18n::t("dialog.vf_title")),
                        Instant::now(),
                    ));
                    None
                }
            },
        );
    close_requested |= new_tab.is_some();
    if run_local {
        app.value_frequency_run_local(active);
    }
    if close_requested {
        app.value_frequency_closed(active);
    }

    let tab = &mut app.tabs[active];
    tab.value_frequency_top_n = top_n_state;
    tab.value_frequency_bin_numeric = bin_state;
    tab.value_frequency_words = words_state;
    // Closing with an uncommitted Bins text keeps what the field shows.
    tab.value_frequency_bins = if close_requested {
        parse_bins(&bins_buf)
    } else {
        committed_bins
    };
    tab.value_frequency_bins_buf = bins_buf;
    tab.value_frequency_size = size;
    if close_requested {
        tab.value_frequency_col = None;
        tab.value_frequency_size = DialogSize::Normal;
    }
    if let Some(new_tab) = new_tab {
        app.push_result_tab(new_tab);
    }
}

fn result_scope_label(rows: usize) -> String {
    octa::i18n::t("dialog.vf_result_tab_hint")
        .replace("{n}", &octa::ui::status_bar::format_number(rows))
}

fn frequency_result_button(ui: &mut egui::Ui, rows: usize) -> egui::Response {
    let count = octa::ui::status_bar::format_number(rows);
    let label = format!("{} ({count})", octa::i18n::t("dialog.vf_result_tab"));
    ui.add_enabled(rows > 0, egui::Button::new(label))
        .on_hover_text(result_scope_label(rows))
        .on_disabled_hover_text(octa::i18n::t("dialog.vf_no_values"))
}

struct FrequencySnapshot {
    report: Arc<ValueFrequency>,
    by_words: bool,
    server_total: Option<usize>,
}

/// Only cell categories partition the population in descending count
/// order. Word counts can overlap; binned counts are in interval order.
fn frequency_result_table(report: &ValueFrequency, by_words: bool) -> anyhow::Result<DataTable> {
    let cumulative = !report.binned && !by_words;
    let mut columns = vec![
        ("value", "Utf8"),
        ("count", "Int64"),
        ("percent", "Float64"),
    ];
    if cumulative {
        columns.push(("cumulative_percent", "Float64"));
    }
    let mut running = 0_usize;
    let rows = report
        .rows
        .iter()
        .map(|row| {
            let mut cells = vec![
                CellValue::String(row.label.clone()),
                CellValue::Int(i64::try_from(row.count)?),
                CellValue::Float(share(row.count, report.total_non_null) * 100.0),
            ];
            if cumulative {
                running = running
                    .checked_add(row.count)
                    .ok_or_else(|| anyhow::anyhow!("frequency total exceeds the numeric range"))?;
                cells.push(CellValue::Float(
                    share(running, report.total_non_null) * 100.0,
                ));
            }
            Ok(cells)
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(DataTable {
        columns: columns
            .into_iter()
            .map(|(name, data_type)| ColumnInfo {
                name: name.into(),
                data_type: data_type.into(),
            })
            .collect(),
        rows,
        ..DataTable::empty()
    })
}

fn frequency_result_tab(
    app: &OctaApp,
    source_index: usize,
    snapshot: &FrequencySnapshot,
) -> anyhow::Result<TabState> {
    let source = &app.tabs[source_index];
    let report = &snapshot.report;
    let mut tab = TabState::new(app.settings.default_search_mode);
    tab.table = frequency_result_table(report, snapshot.by_words)?;
    tab.filter_dirty = true;
    tab.custom_tab_label = Some(format!(
        "{} - {} - {}",
        octa::i18n::t("dialog.vf_title"),
        source.result_source_label(),
        report.column_name
    ));
    let mut hint = vec![result_scope_label(report.rows.len())];
    if let Some(total) = snapshot.server_total {
        hint.push(
            octa::i18n::t("pushdown.vf_server")
                .replace("{total}", &octa::ui::status_bar::format_number(total)),
        );
        if source.table.is_modified() {
            tab.parse_error_banner = Some(octa::i18n::t("pushdown.note_unsaved"));
        }
    } else if source.source_has_more() {
        tab.parse_error_banner = Some(octa::i18n::t("cohort.loaded_note"));
    }
    if let Some(note) = &tab.parse_error_banner {
        hint.push(note.clone());
    }
    tab.tab_hint = Some(hint.join(" "));
    tab.table_state.header_tooltips = vec![
        report.column_name.clone(),
        octa::i18n::t("dialog.vf_col_count"),
        "%".into(),
    ];
    if tab.table.col_count() > 3 {
        tab.table_state
            .header_tooltips
            .push(octa::i18n::t("chart.pareto_share"));
    }
    Ok(tab)
}

#[derive(Default)]
struct FrequencyActions {
    copy_value: Option<String>,
    filter_to_this: Option<String>,
}

/// Only viewport rows are arranged, even in All mode. Horizontal overflow
/// keeps counts and percentages accessible when labels or user-set widths
/// are wider than the dialog.
fn draw_frequency_grid(
    ui: &mut egui::Ui,
    freq: &ValueFrequency,
    by_words: bool,
    actions: &mut FrequencyActions,
) -> egui::scroll_area::ScrollAreaOutput<()> {
    let label_height = ui.text_style_height(&egui::TextStyle::Body);
    let row_height = (label_height + 12.0).max(28.0);
    let rank_width = (freq.rows.len().max(1).ilog10() + 1) as f32 * 8.0 + 12.0;
    let count_width = ((freq.total_non_null.max(1).ilog10() + 1) as f32 * 8.0 + 18.0).max(72.0);
    let share_width = 100.0;
    let row_spacing = 4.0 * ui.spacing().item_spacing.x;
    let value_width =
        (ui.available_width() - rank_width - count_width - share_width - row_spacing - 18.0)
            .max(124.0);
    let weak_text = ui.visuals().weak_text_color();

    egui::ScrollArea::horizontal()
        .id_salt("frequency_horizontal")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            TableBuilder::new(ui)
                .id_salt("frequency_rows")
                .sense(egui::Sense::click())
                .striped(true)
                .resizable(true)
                .auto_shrink([false, false])
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .column(
                    Column::initial(rank_width.max(36.0))
                        .at_least(32.0)
                        .clip(true),
                )
                .column(Column::initial(value_width).at_least(110.0).clip(true))
                .column(Column::initial(count_width).at_least(60.0).clip(true))
                .column(Column::initial(share_width).at_least(65.0).clip(true))
                .header(row_height, |mut header| {
                    for title in [
                        "#".to_string(),
                        octa::i18n::t("dialog.vf_col_value"),
                        octa::i18n::t("dialog.vf_col_count"),
                        "%".to_string(),
                    ] {
                        header.col(|ui| {
                            ui.add(egui::Label::new(RichText::new(&title).strong()).truncate())
                                .on_hover_text(&title);
                        });
                    }
                })
                .body(|body| {
                    body.rows(row_height, freq.rows.len(), |mut tr| {
                        let i = tr.index();
                        let row = &freq.rows[i];
                        tr.col(|ui| {
                            ui.label(RichText::new((i + 1).to_string()).color(weak_text));
                        });
                        tr.col(|ui| {
                            let value = ui
                                .add(egui::Label::new(&row.label).selectable(false).truncate())
                                .on_hover_ui(|ui| {
                                    ui.label(&row.label);
                                });
                            if !freq.binned {
                                value.context_menu(|ui| {
                                    if ui.button(octa::i18n::t("dialog.vf_copy_value")).clicked() {
                                        actions.copy_value = Some(row.label.clone());
                                        ui.close();
                                    }
                                    // Words aren't cell values and cannot be used as a value filter.
                                    let why = by_words
                                        .then(|| octa::i18n::t("dialog.vf_filter_not_words"));
                                    if octa::ui::menu_entry::menu_entry(
                                        ui,
                                        &octa::i18n::t("dialog.vf_filter_to_value"),
                                        "",
                                        why.as_deref(),
                                    ) {
                                        actions.filter_to_this = Some(row.label.clone());
                                        ui.close();
                                    }
                                });
                            }
                        });
                        tr.col(|ui| {
                            ui.label(row.count.to_string());
                        });
                        tr.col(|ui| {
                            draw_percentage(ui, row.count, freq.total_non_null);
                        });
                    });
                })
        })
        .inner
}

fn share(count: usize, total_non_null: usize) -> f64 {
    count as f64 / total_non_null.max(1) as f64
}

fn draw_percentage(ui: &mut egui::Ui, count: usize, total_non_null: usize) {
    let rect = ui.available_rect_before_wrap();
    let fraction = share(count, total_non_null);
    ui.label(format!("{:.1}%", fraction * 100.0));

    // The number remains readable without colour. Keep the small track
    // under it, and keep the same scale for Top N and All.
    let track = egui::Rect::from_min_max(
        egui::pos2(rect.left() + 2.0, rect.bottom() - 5.0),
        egui::pos2(rect.right() - 2.0, rect.bottom() - 2.0),
    );
    if track.is_positive() {
        ui.painter()
            .rect_filled(track, 1.0, ui.visuals().widgets.inactive.bg_fill);
        let bar_width = track.width() * fraction.clamp(0.0, 1.0) as f32;
        if bar_width > 0.0 {
            let bar = egui::Rect::from_min_size(track.min, egui::vec2(bar_width, track.height()));
            ui.painter()
                .rect_filled(bar, 1.0, ui.visuals().selection.stroke.color);
        }
    }
}

/// The Bins field as a bin count; blank or not a positive number is Auto.
fn parse_bins(buf: &str) -> Option<usize> {
    buf.trim().parse::<usize>().ok().filter(|n| *n > 0)
}

fn build_tsv(column_name: &str, freq: &ValueFrequency) -> anyhow::Result<String> {
    let mut writer = csv::WriterBuilder::new()
        .delimiter(b'\t')
        .from_writer(Vec::new());
    writer.write_record([column_name, "count", "percent"])?;
    for row in &freq.rows {
        let count = row.count.to_string();
        let percentage = format!("{:.1}", share(row.count, freq.total_non_null) * 100.0);
        writer.write_record([row.label.as_str(), count.as_str(), percentage.as_str()])?;
    }
    Ok(String::from_utf8(writer.into_inner()?)?)
}

#[cfg(test)]
#[path = "value_frequency_tests.rs"]
mod tests;
