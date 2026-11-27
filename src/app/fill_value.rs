//! Review a single input across explicit visible cells before writing anything.

use std::collections::BTreeMap;
use std::time::Instant;

use eframe::egui;
use octa::data::{CellValue, ColumnInfo, ViewMode};
use octa::i18n::t;
use octa::ui::control_row::{control_line, control_text_edit};
use octa::ui::menu_entry::disabled_tip;

use super::sql_panel::DataStamp;
use super::state::{OctaApp, TabState};
use super::tabs::TabId;

const MAX_CELLS: usize = 50_000;
const MAX_BYTES: usize = 8 * 1024 * 1024;
const MAX_INPUT: usize = 8192;
const SAMPLE: usize = 3;
type Address = (usize, usize);

#[derive(Debug, PartialEq, Eq)]
enum Failure {
    Selection,
    Empty,
    TooLarge,
    BadColumn(usize),
    NoChange,
    Stale,
}

struct Target {
    address: Address,
    before: CellValue,
}

#[derive(Debug)]
struct Plan {
    values: BTreeMap<usize, CellValue>,
    changed: usize,
    samples: Vec<usize>,
}

pub(crate) struct ValueFill {
    tab: TabId,
    title: String,
    stamp: DataStamp,
    scope: String,
    targets: Vec<Target>,
    columns: BTreeMap<usize, ColumnInfo>,
    input: String,
    plan: Result<Plan, Failure>,
    stale: bool,
    focus_set: bool,
    previous_focus: Option<egui::Id>,
}

fn visible_targets(tab: &TabState) -> Result<Vec<Address>, Failure> {
    let st = &tab.table_state;
    if tab.view_mode != ViewMode::Table || !st.selected_rows.is_empty() || tab.filter_dirty {
        return Err(Failure::Selection);
    }
    let table = &tab.table;
    let rows = &tab.filtered_rows;
    let good_row = |&row: &usize| row < table.row_count() && !table.hidden_rows.contains(&row);
    let good_col = |&col: &usize| col < table.col_count() && !tab.hidden_columns.contains(&col);
    let mut out = Vec::new();
    if !st.selected_cols.is_empty() {
        let mut cols: Vec<_> = st.selected_cols.iter().copied().filter(good_col).collect();
        cols.sort_unstable();
        if cols.is_empty() {
            return Err(Failure::Selection);
        }
        for row in rows.iter().copied().filter(good_row) {
            for &col in &cols {
                if out.len() == MAX_CELLS {
                    return Err(Failure::TooLarge);
                }
                out.push((row, col));
            }
        }
    } else if !st.selected_cells.is_empty() {
        for &(row, col) in &st.selected_cells {
            if good_row(&row) && good_col(&col) && rows.binary_search(&row).is_ok() {
                if out.len() == MAX_CELLS {
                    return Err(Failure::TooLarge);
                }
                out.push((row, col));
            }
        }
        out.sort_unstable();
    } else if let Some((row, col)) = st.selected_cell
        && good_row(&row)
        && good_col(&col)
        && rows.binary_search(&row).is_ok()
    {
        out.push((row, col));
    }
    if out.is_empty() {
        Err(Failure::Selection)
    } else {
        Ok(out)
    }
}

fn bytes(value: &CellValue) -> usize {
    match value {
        CellValue::String(s)
        | CellValue::Date(s)
        | CellValue::DateTime(s)
        | CellValue::Nested(s) => s.len(),
        CellValue::Binary(b) => b.len(),
        _ => 0,
    }
}

fn same(a: &CellValue, b: &CellValue) -> bool {
    match (a, b) {
        (CellValue::Float(a), CellValue::Float(b)) => a.to_bits() == b.to_bits(),
        _ => a == b,
    }
}

/// Input is a literal, not a formula. Validate first; the generic conversion
/// helper assumes validation and may otherwise turn an invalid number into 0.
fn value_for_type(input: &str, ty: &str) -> Option<CellValue> {
    use octa::data::{can_convert_value, convert_value, decimal, num_parse, times};
    let s = input.trim();
    if let Some((precision, scale)) = decimal::decimal_parts(ty) {
        let plain = if s.contains(',') && !s.contains('.') {
            s.replace(',', ".")
        } else {
            s.to_owned()
        };
        let raw = decimal::parse_decimal(&plain, precision, scale)?;
        return Some(decimal::decimal_cell(raw, scale));
    }
    if let Some(kind) = times::TimeKind::of_type(ty) {
        let ns = kind.parse(s)?;
        let quantum = if ty.contains("Nanosecond") {
            1
        } else if ty.contains("Microsecond") {
            1_000
        } else if ty.contains("Millisecond") {
            1_000_000
        } else if ty.contains("Second") {
            1_000_000_000
        } else {
            return None;
        };
        return (ns % quantum == 0).then(|| kind.cell(ns));
    }
    let value = match ty {
        "String" | "Utf8" | "LargeUtf8" | "Utf8View" => CellValue::String(input.to_owned()),
        "Int8" => CellValue::Int(s.parse::<i8>().ok()?.into()),
        "Int16" => CellValue::Int(s.parse::<i16>().ok()?.into()),
        "Int32" => CellValue::Int(s.parse::<i32>().ok()?.into()),
        "Int64" => CellValue::Int(s.parse().ok()?),
        "UInt8" => CellValue::Int(s.parse::<u8>().ok()?.into()),
        "UInt16" => CellValue::Int(s.parse::<u16>().ok()?.into()),
        "UInt32" => CellValue::Int(s.parse::<u32>().ok()?.into()),
        "UInt64" => CellValue::Int(i64::try_from(s.parse::<u64>().ok()?).ok()?),
        "Float32" | "Float64" => {
            let n = num_parse::parse_number_relaxed(s)?;
            let n = if ty == "Float32" {
                f64::from(n as f32)
            } else {
                n
            };
            CellValue::Float(n.is_finite().then_some(n)?)
        }
        "Boolean" | "Date32" | "Timestamp(Microsecond, None)" => {
            let text = CellValue::String(s.to_owned());
            if s.is_empty() || !can_convert_value(&text, ty) {
                return None;
            }
            convert_value(&text, ty)
        }
        _ => return None,
    };
    Some(value)
}

impl ValueFill {
    fn start(tab: &TabState, previous_focus: Option<egui::Id>) -> Result<Self, Failure> {
        let mut running_bytes = 0usize;
        let mut columns = BTreeMap::new();
        let mut rows = 0;
        let mut last = None;
        let targets = visible_targets(tab)?
            .into_iter()
            .map(|(row, col)| {
                let before = tab.table.get(row, col).unwrap_or(&CellValue::Null);
                running_bytes = running_bytes.saturating_add(bytes(before));
                if running_bytes > MAX_BYTES {
                    return Err(Failure::TooLarge);
                }
                if last != Some(row) {
                    rows += 1;
                    last = Some(row);
                }
                columns
                    .entry(col)
                    .or_insert_with(|| tab.table.columns[col].clone());
                Ok(Target {
                    address: (row, col),
                    before: before.clone(),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let scope = t("search.replace_review_scope")
            .replace("{cells}", &targets.len().to_string())
            .replace("{rows}", &rows.to_string())
            .replace("{cols}", &columns.len().to_string());
        Ok(Self {
            tab: tab.id,
            title: tab.title_display(),
            stamp: DataStamp::of(&tab.table),
            scope,
            targets,
            columns,
            input: String::new(),
            plan: Err(Failure::Empty),
            stale: false,
            focus_set: false,
            previous_focus,
        })
    }

    fn prepare(&self) -> Result<Plan, Failure> {
        if self.input.is_empty() {
            return Err(Failure::Empty);
        }
        if self.input.len() > MAX_INPUT
            || self.input.len().saturating_mul(self.targets.len()) > MAX_BYTES
        {
            return Err(Failure::TooLarge);
        }
        let mut values = BTreeMap::new();
        for (&col, info) in &self.columns {
            let val =
                value_for_type(&self.input, &info.data_type).ok_or(Failure::BadColumn(col))?;
            values.insert(col, val);
        }
        let mut result = Plan {
            values,
            changed: 0,
            samples: Vec::new(),
        };
        for (i, target) in self.targets.iter().enumerate() {
            if !same(&target.before, &result.values[&target.address.1]) {
                result.changed += 1;
                if result.samples.len() < SAMPLE {
                    result.samples.push(i);
                }
            }
        }
        if result.changed == 0 {
            return Err(Failure::NoChange);
        }
        Ok(result)
    }

    fn source_matches(&self, tab: &TabState) -> bool {
        !self.stale
            && self.tab == tab.id
            && tab.view_mode == ViewMode::Table
            && !tab.filter_dirty
            && self.stamp == DataStamp::of(&tab.table)
    }

    fn still_matches(&self, tab: &TabState) -> bool {
        self.source_matches(tab)
            && self.columns.iter().all(|(&col, old)| {
                tab.table
                    .columns
                    .get(col)
                    .is_some_and(|c| c.name == old.name && c.data_type == old.data_type)
            })
            && visible_targets(tab).is_ok_and(|cells| {
                cells
                    .into_iter()
                    .eq(self.targets.iter().map(|target| target.address))
            })
            && self.targets.iter().all(|target| {
                let (r, c) = target.address;
                same(
                    tab.table.get(r, c).unwrap_or(&CellValue::Null),
                    &target.before,
                )
            })
    }

    fn explanation(&self, error: &Failure) -> String {
        match error {
            Failure::BadColumn(col) => {
                let info = &self.columns[col];
                t("fill.value_bad_type")
                    .replace("{column}", &excerpt(&info.name))
                    .replace("{type}", &excerpt(&info.data_type))
            }
            other => t(other.key()),
        }
    }
}

impl Failure {
    fn key(&self) -> &'static str {
        match self {
            Self::Selection => "greyed.need_selection",
            Self::Empty => "fill.value_input_hint",
            Self::TooLarge => "fill.value_large",
            Self::NoChange => "search.replace_review_empty",
            Self::Stale => "fill.value_stale",
            Self::BadColumn(_) => "fill.value_bad_type",
        }
    }
}

fn excerpt(s: &str) -> String {
    let mut chars = s.chars();
    let mut out: String = chars
        .by_ref()
        .take(52)
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect();
    if chars.next().is_some() {
        out.push_str("...");
    }
    out
}

fn sample(value: &CellValue) -> String {
    match value {
        CellValue::Null => t("dialog.cf_empty"),
        CellValue::String(s) | CellValue::Nested(s) => format!("{:?}", excerpt(s)),
        CellValue::Binary(b) => format!("{:?}", &b[..b.len().min(8)]),
        other => excerpt(&other.to_string()),
    }
}

fn modal_id() -> egui::Id {
    egui::Id::new("dbviewy_fill_value")
}

impl OctaApp {
    pub(crate) fn fill_value_unavailable(&self) -> Option<String> {
        let tab = &self.tabs[self.active_tab];
        let st = &tab.table_state;
        if tab.table.col_count() == 0 {
            Some(t("greyed.need_table"))
        } else if tab.view_mode != ViewMode::Table {
            Some(t("greyed.need_table_view"))
        } else if self.is_readonly() {
            Some(t("context_menu.header_row_readonly"))
        } else if !st.selected_rows.is_empty()
            || st.editing_cell.is_some()
            || (st.selected_cells.is_empty()
                && st.selected_cols.is_empty()
                && st.selected_cell.is_none())
        {
            Some(t("greyed.need_selection"))
        } else {
            None
        }
    }

    pub(crate) fn open_value_fill(&mut self, ctx: &egui::Context) {
        if self.value_fill.is_some()
            || self.paste_special.is_some()
            || self.replace_review.is_some()
            || self.fill_value_unavailable().is_some()
        {
            return;
        }
        if self.tabs[self.active_tab].filter_dirty {
            self.recompute_filter();
        }
        match ValueFill::start(&self.tabs[self.active_tab], ctx.memory(|m| m.focused())) {
            Ok(state) => self.value_fill = Some(state),
            Err(e) => self.status_message = Some((t(e.key()), Instant::now())),
        }
    }

    fn apply_value_fill(&mut self, state: &ValueFill) -> Result<usize, Failure> {
        if self.is_readonly() {
            return Err(Failure::Stale);
        }
        let tab = &mut self.tabs[self.active_tab];
        if !state.still_matches(tab) {
            return Err(Failure::Stale);
        }
        // Validate all columns before the first put. Preview and commit
        // use the same plan builder, including the byte and type limits.
        let plan = state.prepare()?;
        let undo = tab.table.undo_stack.len();
        for target in &state.targets {
            let (row, col) = target.address;
            let value = &plan.values[&col];
            if !same(&target.before, value) {
                tab.table.set(row, col, value.clone());
            }
        }
        tab.table.coalesce_undo_since(undo);
        tab.table_state.invalidate_row_heights();
        tab.filter_dirty = true;
        Ok(plan.changed)
    }

    pub(crate) fn render_value_fill(&mut self, ctx: &egui::Context) {
        let id = modal_id();
        let restore = id.with("restore_focus");
        if let Some(focus) = ctx.data(|d| d.get_temp::<egui::Id>(restore)) {
            let modal = ctx.memory(|m| m.top_modal_layer());
            if modal == Some(egui::Modal::default_area(id).layer()) {
                ctx.request_repaint();
            } else {
                ctx.data_mut(|d| d.remove::<egui::Id>(restore));
                if modal.is_none() {
                    ctx.memory_mut(|m| {
                        if m.focused().is_none() {
                            m.request_focus(focus);
                        }
                    });
                }
            }
        }
        let Some(mut st) = self.value_fill.take() else {
            return;
        };
        let tab = &self.tabs[self.active_tab];
        st.stale |= !st.source_matches(tab);
        let read_only = self.is_readonly();
        let response = egui::Modal::new(id).show(ctx, |ui| {
            let viewport = ctx.content_rect();
            let width = (viewport.width() - 40.0).clamp(190.0, 476.0);
            ui.set_width(width);
            let title = t("edit_menu.fill_value");
            ui.heading(title.trim_end_matches("..."));
            let scope = format!("{}  |  {}", excerpt(&st.title), st.scope);
            ui.add(egui::Label::new(egui::RichText::new(&scope).strong()).wrap());
            ui.separator();
            let hint = t("fill.value_input_hint");
            let label = ui.label(t("fill.kind")).on_hover_text(&hint);
            let field = ui
                .add_enabled_ui(!st.stale && !read_only, |ui| {
                    control_text_edit(
                        ui,
                        width,
                        egui::TextEdit::singleline(&mut st.input)
                            .id(id.with("value"))
                            .desired_width(width)
                            .char_limit(2048),
                    )
                    .labelled_by(label.id)
                })
                .inner
                .on_hover_text(&hint);
            if !st.focus_set && !st.stale && !read_only {
                field.request_focus();
                st.focus_set = true;
            }
            if field.changed() {
                st.plan = st.prepare();
            }
            let why = if read_only {
                Some(t("context_menu.header_row_readonly"))
            } else if st.stale {
                Some(t("fill.value_stale"))
            } else {
                st.plan.as_ref().err().map(|e| st.explanation(e))
            };
            if let Some(why) = &why {
                ui.add(egui::Label::new(why).wrap());
            } else if let Ok(plan) = &st.plan {
                ui.strong(
                    t("fill.value_will_change").replace("{count}", &plan.changed.to_string()),
                );
                egui::ScrollArea::vertical()
                    .max_height((viewport.height() - 220.0).clamp(52.0, 170.0))
                    .show(ui, |ui| {
                        for &i in &plan.samples {
                            let target = &st.targets[i];
                            let (row, col) = target.address;
                            let info = &st.columns[&col];
                            ui.weak(format!(
                                "{} / {}",
                                row.saturating_add(tab.table.row_offset).saturating_add(1),
                                excerpt(&info.name)
                            ));
                            ui.monospace(format!(
                                "{}: {}",
                                t("datadrift.side_a"),
                                sample(&target.before)
                            ));
                            ui.monospace(format!(
                                "{}: {}",
                                t("datadrift.side_b"),
                                sample(&plan.values[&col])
                            ));
                        }
                    });
            }
            ui.separator();
            control_line(ui, |ui| {
                let cancel = ui
                    .button(t("common.cancel"))
                    .on_hover_text(t("fill.cancel_hint"));
                let hint = t("edit_menu.fill_value_hint");
                let apply = ui.add_enabled(why.is_none(), egui::Button::new(t("fill.fill")));
                let apply = if let Some(why) = why {
                    apply.on_disabled_hover_text(disabled_tip(&hint, &why))
                } else {
                    apply.on_hover_text(hint)
                };
                (cancel.clicked(), apply.clicked())
            })
            .inner
        });
        let closed = response.should_close() || response.inner.0;
        let mut applied = false;
        if !closed && response.inner.1 {
            match self.apply_value_fill(&st) {
                Ok(count) => {
                    self.status_message = Some((
                        t("fill.done").replace("{n}", &count.to_string()),
                        Instant::now(),
                    ));
                    applied = true;
                }
                Err(Failure::Stale) => st.stale = true,
                Err(e) => st.plan = Err(e),
            }
        }
        if closed || applied {
            ctx.input_mut(|i| {
                i.events.retain(|event| {
                    !matches!(
                        event,
                        egui::Event::Key {
                            key: egui::Key::Enter | egui::Key::Space | egui::Key::Escape,
                            ..
                        }
                    )
                })
            });
            ctx.memory_mut(|m| m.stop_text_input());
            if let Some(focus) = st.previous_focus {
                ctx.data_mut(|d| d.insert_temp(restore, focus));
                ctx.request_repaint();
            }
        } else {
            self.value_fill = Some(st);
        }
    }
}

#[cfg(test)]
#[path = "fill_value_tests.rs"]
mod tests;
