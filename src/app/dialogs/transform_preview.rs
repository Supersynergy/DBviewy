//! Non-mutating sample of the Split operation. Apply still uses the recipe.
use eframe::egui;
use octa::data::{
    CellValue, DataTable,
    recipe::{RecipeStep, Split, unique_name},
    transform::split_column,
};
use octa::i18n::t;

use super::{TransformState, transform_step};

const SAMPLE_ROWS: usize = 3;
const SAMPLE_COLS: usize = 6;
const SAMPLE_CELL_BYTES: usize = 1024;
const SAMPLE_RULE_BYTES: usize = 1024;

struct Sample {
    columns: Vec<(String, Vec<CellValue>)>,
}

fn sample(table: &DataTable, step: &Split) -> anyhow::Result<Option<Sample>> {
    let source = table.column_index(&step.column)?;
    if step.value.len() > SAMPLE_RULE_BYTES {
        return Ok(None);
    }
    let mut rows = Vec::with_capacity(SAMPLE_ROWS);
    for row in 0..table.row_count().min(SAMPLE_ROWS) {
        let value = table.get(row, source).unwrap_or(&CellValue::Null);
        let bytes = match value {
            CellValue::String(s)
            | CellValue::Nested(s)
            | CellValue::Date(s)
            | CellValue::DateTime(s) => s.len(),
            CellValue::Binary(data) => data.len().saturating_mul(3),
            _ => 0,
        };
        if bytes > SAMPLE_CELL_BYTES {
            return Ok(None);
        }
        rows.push(vec![value.clone()]);
    }
    if rows.is_empty() {
        return Ok(None);
    }

    let input = DataTable {
        columns: vec![table.columns[source].clone()],
        rows,
        ..DataTable::empty()
    };
    let spec = step.spec()?;
    let mut names = table
        .columns
        .iter()
        .map(|column| column.name.clone())
        .collect::<Vec<_>>();
    let base = step.new_name.trim();
    let columns = split_column(&input, 0, &spec)?
        .into_iter()
        .take(SAMPLE_COLS)
        .enumerate()
        .map(|(index, (auto, values))| {
            let proposed = if base.is_empty() {
                auto
            } else {
                format!("{base}_{}", index + 1)
            };
            let name = unique_name(&names, &proposed);
            names.push(name.clone());
            (name, values)
        })
        .collect();
    Ok(Some(Sample { columns }))
}

pub(super) fn render(ui: &mut egui::Ui, st: &TransformState, names: &[String], table: &DataTable) {
    let hint = t("transform.split_preview_hint");
    let why = if table.row_count() == 0 {
        Some(t("record.empty"))
    } else if !st.col.is_some_and(|col| col < names.len()) {
        Some(t("transform.need_column"))
    } else {
        None
    };
    let button = ui.add_enabled(
        why.is_none(),
        egui::Button::new(t("dialog.pv_preview")).small(),
    );
    let button = if let Some(why) = why {
        button.on_disabled_hover_text(octa::ui::menu_entry::disabled_tip(&hint, &why))
    } else {
        button.on_hover_text(&hint)
    };
    let restore_focus =
        egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&button))
            && ui.input(|input| input.key_pressed(egui::Key::Escape));
    egui::Popup::menu(&button).show(|ui| {
        ui.set_max_width(370.0);
        ui.add(egui::Label::new(egui::RichText::new(&hint).weak()).wrap());
        ui.separator();
        let result = transform_step(st, names).and_then(|step| {
            let RecipeStep::Split(step) = step else {
                return Ok(None);
            };
            sample(table, &step).map_err(|err| format!("{}: {err:#}", t("transform.failed")))
        });
        match result {
            Ok(Some(sample)) => draw_sample(ui, &sample),
            Ok(None) => {
                ui.weak("-");
            }
            Err(error) => {
                ui.add(egui::Label::new(error).wrap());
            }
        }
    });
    // egui dismisses the popup and clears focus on Escape. Return focus to
    // its trigger so the keyboard can reopen the sample.
    if restore_focus {
        button.request_focus();
    }
}

fn draw_sample(ui: &mut egui::Ui, sample: &Sample) {
    egui::ScrollArea::horizontal()
        .max_width(370.0)
        .show(ui, |ui| {
            egui::Grid::new("split_sample")
                .striped(true)
                .show(ui, |ui| {
                    ui.weak("#");
                    for (name, _) in &sample.columns {
                        preview_cell(ui, name, true);
                    }
                    ui.end_row();
                    let rows = sample.columns.first().map_or(0, |(_, cells)| cells.len());
                    for i in 0..rows {
                        ui.weak((i + 1).to_string());
                        for (_, cells) in &sample.columns {
                            let CellValue::String(value) = &cells[i] else {
                                preview_cell(ui, "-", false);
                                continue;
                            };
                            preview_cell(ui, if value.is_empty() { "\"\"" } else { value }, false);
                        }
                        ui.end_row();
                    }
                });
        });
}

fn preview_cell(ui: &mut egui::Ui, text: &str, header: bool) {
    let style = egui::RichText::new(text);
    let style = if header { style.strong() } else { style };
    ui.add_sized([104.0, 20.0], egui::Label::new(style).truncate())
        .on_hover_text(text);
}

#[cfg(test)]
#[path = "transform_preview_tests.rs"]
mod tests;
