//! Row-hash-diff renderer for the Compare view. Uses BLAKE3 over the user-
//! picked columns to *match* rows; the UI shows the actual row content so
//! the user can see which records are unique to each side or shared.
//!
//! - **Left only** - rows whose hash is present in left but missing in right.
//! - **Right only** - rows whose hash is present in right but missing in left.
//! - **Shared (in both files)** - hash present in both. Each bucket can hold
//!   multiple physical rows (duplicates within one file collapse to the same
//!   hash); all of them are listed so the user spots the cardinality.
//!
//! Cross-format works because hashing sees only `CellValue::to_string`
//! output - a CSV row and a Parquet row with the same logical content
//! produce identical digests.

use std::collections::HashMap;

use eframe::egui;
use egui::{RichText, ScrollArea};

use octa::data::{CellValue, DataTable};
use octa::i18n::t;

use super::hash::{hash_row, short_hex};
use crate::app::state::TabState;
use crate::ui;
use ui::theme::ThemeMode;

/// Limits for the preview, not for matching or its totals.
const BUCKET_DISPLAY_CAP: usize = 200;
const ROWS_PER_BUCKET: usize = 50;

type Digest = [u8; 32];
type Buckets = HashMap<Digest, Vec<usize>>;
type SideGroup<'a> = (&'a Digest, &'a [usize]);
type SharedGroup<'a> = (&'a Digest, &'a [usize], &'a [usize]);

#[derive(Default)]
struct Groups<'a> {
    left_only: Vec<SideGroup<'a>>,
    right_only: Vec<SideGroup<'a>>,
    shared: Vec<SharedGroup<'a>>,
}

fn separate_groups<'a>(left: &'a Buckets, right: &'a Buckets) -> Groups<'a> {
    let mut groups = Groups::default();
    for (hash, rows) in left {
        if let Some(matches) = right.get(hash) {
            groups.shared.push((hash, rows, matches));
        } else {
            groups.left_only.push((hash, rows));
        }
    }
    for (hash, rows) in right {
        if !left.contains_key(hash) {
            groups.right_only.push((hash, rows));
        }
    }
    // Byte order is hex order. Comparing the digest directly removes hex
    // allocation during sorting and breaks ties in the printed prefix.
    groups.left_only.sort_unstable_by(|a, b| a.0.cmp(b.0));
    groups.right_only.sort_unstable_by(|a, b| a.0.cmp(b.0));
    groups.shared.sort_unstable_by(|a, b| a.0.cmp(b.0));
    groups
}

/// Write one quoted TSV section, with exactly the scope of its preview.
fn bucket_to_tsv(
    table: &DataTable,
    buckets: &[SideGroup<'_>],
    cols: &[usize],
) -> anyhow::Result<String> {
    let displayed = effective_columns(table, cols);
    let row_word = t("views.row");
    let mut w = csv::WriterBuilder::new()
        .delimiter(b'\t')
        .from_writer(Vec::new());
    w.write_record(
        std::iter::once(row_word.as_str())
            .chain(displayed.iter().map(|&c| table.columns[c].name.as_str())),
    )?;
    for (_, rows) in buckets.iter().take(BUCKET_DISPLAY_CAP) {
        for &r in rows.iter().take(ROWS_PER_BUCKET) {
            w.write_field((r + 1 + table.row_offset).to_string())?;
            for &c in &displayed {
                match table.get(r, c) {
                    Some(CellValue::String(text)) => w.write_field(text.as_bytes())?,
                    Some(CellValue::Null) | None => w.write_field(b"")?,
                    Some(value) => w.write_field(value.to_string())?,
                }
            }
            w.write_record(std::iter::empty::<&str>())?;
        }
    }
    Ok(String::from_utf8(w.into_inner()?)?)
}

fn clean_picks(table: &DataTable, selected: &mut Vec<usize>) {
    selected.retain(|&c| c < table.col_count());
}

/// The implicit scope for an untouched side is all its columns, not zero.
fn hash_width(table: &DataTable, explicit: &[usize]) -> usize {
    if explicit.is_empty() {
        table.col_count()
    } else {
        explicit.len()
    }
}

fn comparison_widths(
    left: &DataTable,
    right: &DataTable,
    a: &[usize],
    b: &[usize],
) -> (usize, usize) {
    (hash_width(left, a), hash_width(right, b))
}

fn column_caption(name: &str, position: Option<usize>) -> String {
    position.map_or_else(|| name.to_owned(), |number| format!("{number}  {name}"))
}

pub fn render(ui: &mut egui::Ui, tab: &mut TabState, theme_mode: ThemeMode) {
    let colors = ui::theme::ThemeColors::for_mode(theme_mode);
    let Some(ref right_box) = tab.compare_right_table else {
        ui.label(RichText::new(octa::i18n::t("views.rowdiff_no_right")).color(colors.text_muted));
        return;
    };
    let left = &tab.table;
    let right: &DataTable = right_box.as_ref();

    clean_picks(left, &mut tab.compare_columns_left);
    clean_picks(right, &mut tab.compare_columns_right);

    ui.collapsing(t("views.rowdiff_columns"), |ui| {
        ui.columns_const(|[a, b]| {
            draw_column_picker(
                a,
                "hash_left",
                &t("views.left"),
                left,
                &mut tab.compare_columns_left,
                &colors,
            );
            draw_column_picker(
                b,
                "hash_right",
                &t("views.right"),
                right,
                &mut tab.compare_columns_right,
                &colors,
            );
        });
        ui.add_space(4.0);
        ui.label(
            RichText::new(t("views.rowdiff_empty_note"))
                .color(colors.text_muted)
                .size(11.0),
        );
    });
    ui.separator();

    let (width_a, width_b) = comparison_widths(
        left,
        right,
        &tab.compare_columns_left,
        &tab.compare_columns_right,
    );
    if width_a == 0 && width_b == 0 {
        ui.label(t("views.no_columns"));
        return;
    }
    if width_a != width_b {
        let warning = t("views.rowdiff_pair_count")
            .replace("{left}", &width_a.to_string())
            .replace("{right}", &width_b.to_string());
        ui.label(RichText::new(warning).color(colors.warning));
        return;
    }

    let left_hashes = collect_hash_rows(left, &tab.compare_columns_left);
    let right_hashes = collect_hash_rows(right, &tab.compare_columns_right);
    let groups = separate_groups(&left_hashes, &right_hashes);

    // Summary line - totals upfront so the user knows the scale before
    // scrolling.
    octa::ui::control_row::control_row(ui, |ui| {
        ui.label(
            RichText::new(
                octa::i18n::t("views.rowdiff_summary")
                    .replace("{left}", &left.row_count().to_string())
                    .replace("{right}", &right.row_count().to_string())
                    .replace("{shared}", &groups.shared.len().to_string())
                    .replace("{left_only}", &groups.left_only.len().to_string())
                    .replace("{right_only}", &groups.right_only.len().to_string()),
            )
            .color(colors.text_secondary)
            .size(12.0),
        );
    });
    ui.add_space(6.0);

    let scroll = ScrollArea::vertical()
        .id_salt("compare_row_diff_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            draw_single_side_bucket(
                ui,
                &octa::i18n::t("views.left_only"),
                colors.warning,
                left,
                &groups.left_only,
                &tab.compare_columns_left,
                &colors,
            );
            ui.add_space(8.0);
            draw_single_side_bucket(
                ui,
                &octa::i18n::t("views.right_only"),
                colors.accent,
                right,
                &groups.right_only,
                &tab.compare_columns_right,
                &colors,
            );
            ui.add_space(8.0);
            draw_shared_bucket(
                ui,
                left,
                right,
                &groups.shared,
                &tab.compare_columns_left,
                &tab.compare_columns_right,
                &colors,
            );
        });

    let area_resp = ui.interact(
        scroll.inner_rect,
        ui.make_persistent_id("compare_row_diff_menu"),
        egui::Sense::click(),
    );
    let has_preview = !groups.left_only.is_empty() || !groups.right_only.is_empty();
    let mut copy = has_preview
        && (area_resp.hovered() || area_resp.has_focus())
        && octa::ui::text_selection::copy_pressed(ui.ctx());
    if area_resp.clicked() {
        area_resp.request_focus();
    }
    area_resp.context_menu(|ui| {
        let action = egui::Button::new(t("views.rowdiff_copy_preview"));
        if ui
            .add_enabled(has_preview, action)
            .on_disabled_hover_text(t("views.none"))
            .clicked()
        {
            copy = true;
            ui.close();
        }
    });
    if copy {
        match copy_preview(
            left,
            right,
            &groups,
            &tab.compare_columns_left,
            &tab.compare_columns_right,
        ) {
            Ok(text) => ui.ctx().copy_text(text),
            Err(err) => tracing::error!("Could not copy hash comparison preview: {err:#}"),
        }
    }
}

fn copy_preview(
    left: &DataTable,
    right: &DataTable,
    groups: &Groups<'_>,
    left_cols: &[usize],
    right_cols: &[usize],
) -> anyhow::Result<String> {
    let mut sections = Vec::with_capacity(2);
    for (title, source, buckets, columns) in [
        ("views.left_only", left, &groups.left_only, left_cols),
        ("views.right_only", right, &groups.right_only, right_cols),
    ] {
        if !buckets.is_empty() {
            sections.push(format!(
                "# {}\n{}",
                t(title),
                bucket_to_tsv(source, buckets, columns)?
            ));
        }
    }
    Ok(sections.join("\n"))
}

/// One hash -> all row indices that produced it. Duplicates in one file
/// collapse to the same hash and accumulate into the same Vec.
fn collect_hash_rows(table: &DataTable, cols: &[usize]) -> Buckets {
    let mut out: HashMap<[u8; 32], Vec<usize>> = HashMap::with_capacity(table.row_count());
    for row_idx in 0..table.row_count() {
        let h = hash_row(table, row_idx, cols);
        out.entry(h).or_default().push(row_idx);
    }
    out
}

fn draw_column_picker(
    ui: &mut egui::Ui,
    id: &'static str,
    label: &str,
    table: &DataTable,
    selected: &mut Vec<usize>,
    colors: &ui::theme::ThemeColors,
) {
    ui.label(RichText::new(label).strong());
    let count = table.col_count();
    if count == 0 {
        ui.label(RichText::new(t("views.no_columns")).color(colors.text_muted));
        return;
    }
    let implicit = selected.is_empty();
    let ranks = if implicit {
        (1..=count).map(Some).collect::<Vec<_>>()
    } else {
        let mut ranks = vec![None; count];
        for (rank, &index) in selected.iter().enumerate() {
            ranks[index] = Some(rank + 1);
        }
        ranks
    };
    // Each side can scroll without moving its column numbers out of the
    // other side's view. Long column names wrap or fit their lane.
    ScrollArea::vertical()
        .id_salt(id)
        .max_height(210.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
            for (index, column) in table.columns.iter().enumerate() {
                let mut picked = !implicit && ranks[index].is_some();
                ui.push_id(index, |ui| {
                    let caption = column_caption(&column.name, ranks[index]);
                    let option = ui
                        .checkbox(&mut picked, caption)
                        .on_hover_text(column.name.as_str());
                    if option.clicked() {
                        option.request_focus();
                    }
                    if option.changed() {
                        if picked {
                            selected.push(index);
                        } else {
                            selected.retain(|&c| c != index);
                        }
                    }
                });
            }
        });
}

/// Render a Left-only / Right-only bucket. Shows actual row content so the
/// user sees which records are unique. Each bucket entry expands into a
/// per-hash collapsible carrying all rows that produced that hash.
fn draw_single_side_bucket(
    ui: &mut egui::Ui,
    title: &str,
    title_color: egui::Color32,
    table: &DataTable,
    buckets: &[SideGroup<'_>],
    selected_cols: &[usize],
    colors: &ui::theme::ThemeColors,
) {
    let total_rows: usize = buckets.iter().map(|(_, rows)| rows.len()).sum();
    ui.label(
        RichText::new(
            octa::i18n::t("views.rowdiff_bucket")
                .replace("{title}", title)
                .replace("{hashes}", &buckets.len().to_string())
                .replace("{rows}", &total_rows.to_string()),
        )
        .strong()
        .color(title_color),
    );
    let display_cols = effective_columns(table, selected_cols);
    if buckets.is_empty() {
        ui.label(
            RichText::new(octa::i18n::t("views.none"))
                .color(colors.text_muted)
                .size(11.0),
        );
        return;
    }
    let mut shown = 0usize;
    for (digest, rows) in buckets {
        // Header collapsible: hash digest + duplicate-count badge.
        let header = format!("{}     × {}", short_hex(digest), rows.len(),);
        egui::CollapsingHeader::new(
            RichText::new(header)
                .font(egui::FontId::new(12.0, egui::FontFamily::Monospace))
                .color(colors.text_primary),
        )
        .id_salt(("bucket_single", title, digest))
        .default_open(false)
        .show(ui, |ui| {
            draw_rows_inline(ui, table, &display_cols, rows, colors);
        });
        shown += 1;
        if shown >= BUCKET_DISPLAY_CAP {
            break;
        }
    }
    if buckets.len() > BUCKET_DISPLAY_CAP {
        ui.label(
            RichText::new(
                octa::i18n::t("views.rowdiff_more_buckets")
                    .replace("{n}", &(buckets.len() - BUCKET_DISPLAY_CAP).to_string()),
            )
            .color(colors.text_muted),
        );
    }
}

/// Render the Shared bucket. Each entry shows the left rows AND the right
/// rows that hashed to the same digest, side by side so the user can spot
/// content divergence in unhashed columns.
fn draw_shared_bucket(
    ui: &mut egui::Ui,
    left: &DataTable,
    right: &DataTable,
    buckets: &[SharedGroup<'_>],
    left_cols: &[usize],
    right_cols: &[usize],
    colors: &ui::theme::ThemeColors,
) {
    let total_left: usize = buckets.iter().map(|(_, l, _)| l.len()).sum();
    let total_right: usize = buckets.iter().map(|(_, _, r)| r.len()).sum();
    ui.label(
        RichText::new(
            octa::i18n::t("views.rowdiff_shared")
                .replace("{hashes}", &buckets.len().to_string())
                .replace("{left}", &total_left.to_string())
                .replace("{right}", &total_right.to_string()),
        )
        .strong()
        .color(colors.success),
    );
    if buckets.is_empty() {
        ui.label(
            RichText::new(octa::i18n::t("views.none"))
                .color(colors.text_muted)
                .size(11.0),
        );
        return;
    }
    let left_display_cols = effective_columns(left, left_cols);
    let right_display_cols = effective_columns(right, right_cols);
    let mut shown = 0usize;
    for (digest, lrows, rrows) in buckets {
        let header = format!(
            "{}     L × {}   R × {}",
            short_hex(digest),
            lrows.len(),
            rrows.len(),
        );
        egui::CollapsingHeader::new(
            RichText::new(header)
                .font(egui::FontId::new(12.0, egui::FontFamily::Monospace))
                .color(colors.text_primary),
        )
        .id_salt(("bucket_shared", digest))
        .default_open(false)
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(octa::i18n::t("views.left"))
                            .color(colors.text_muted)
                            .size(11.0),
                    );
                    draw_rows_inline(ui, left, &left_display_cols, lrows, colors);
                });
                ui.add_space(16.0);
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(octa::i18n::t("views.right"))
                            .color(colors.text_muted)
                            .size(11.0),
                    );
                    draw_rows_inline(ui, right, &right_display_cols, rrows, colors);
                });
            });
        });
        shown += 1;
        if shown >= BUCKET_DISPLAY_CAP {
            break;
        }
    }
    if buckets.len() > BUCKET_DISPLAY_CAP {
        ui.label(
            RichText::new(
                octa::i18n::t("views.rowdiff_more_buckets")
                    .replace("{n}", &(buckets.len() - BUCKET_DISPLAY_CAP).to_string()),
            )
            .color(colors.text_muted),
        );
    }
}

/// Resolve which columns to render in the result rows. When the user
/// hasn't picked any columns we'd otherwise show *all* of them, which can
/// be unwieldy - fall back to the first 8 so the table stays readable.
fn effective_columns(table: &DataTable, picked: &[usize]) -> Vec<usize> {
    if !picked.is_empty() {
        return picked
            .iter()
            .copied()
            .filter(|i| *i < table.col_count())
            .collect();
    }
    (0..table.col_count().min(8)).collect()
}

/// Render a small inline grid of row content for the given indices and
/// columns. Used inside the per-hash CollapsingHeader bodies so the user
/// sees actual cell values, not just hex digests.
fn draw_rows_inline(
    ui: &mut egui::Ui,
    table: &DataTable,
    cols: &[usize],
    rows: &[usize],
    colors: &ui::theme::ThemeColors,
) {
    let mono = egui::FontId::new(12.0, egui::FontFamily::Monospace);
    egui::Grid::new(format!(
        "compare_rows_inline_{}_{}",
        rows.first().copied().unwrap_or(usize::MAX),
        rows.len(),
    ))
    .num_columns(cols.len() + 1)
    .spacing([12.0, 2.0])
    .striped(true)
    .show(ui, |ui| {
        // Header row.
        ui.label(
            RichText::new(octa::i18n::t("views.row"))
                .font(mono.clone())
                .color(colors.text_muted),
        );
        for &c in cols {
            let name = table
                .columns
                .get(c)
                .map(|col| col.name.as_str())
                .unwrap_or("?");
            ui.label(
                RichText::new(name)
                    .font(mono.clone())
                    .color(colors.text_muted),
            );
        }
        ui.end_row();
        // Data rows, capped per-bucket so a million-row duplicate set
        // doesn't drown the UI thread.
        for row_idx in rows.iter().take(ROWS_PER_BUCKET) {
            ui.label(
                RichText::new(format!("{}", row_idx + 1 + table.row_offset))
                    .font(mono.clone())
                    .color(colors.text_muted),
            );
            for &c in cols {
                let text = match table.get(*row_idx, c) {
                    Some(CellValue::Null) => "-".to_string(),
                    Some(v) => v.to_string(),
                    None => String::new(),
                };
                ui.label(RichText::new(text).font(mono.clone()));
            }
            ui.end_row();
        }
        if rows.len() > ROWS_PER_BUCKET {
            ui.label(
                RichText::new(
                    octa::i18n::t("views.rowdiff_more_rows")
                        .replace("{n}", &(rows.len() - ROWS_PER_BUCKET).to_string()),
                )
                .color(colors.text_muted),
            );
            ui.end_row();
        }
    });
}

#[cfg(test)]
#[path = "row_diff_tests.rs"]
mod tests;
