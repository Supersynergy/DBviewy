//! The filter chip row: every active filter narrowing the view, in one row.

use eframe::egui;

use octa::data::SearchResultMode;
use octa::i18n::t;
use octa::ui;

use crate::app::db_view::search_hides_rows;
use crate::app::state::{OctaApp, TabState};

impl OctaApp {
    pub(super) fn render_filter_chips(&mut self, ui: &mut egui::Ui) {
        let chips = filter_chips(&self.tabs[self.active_tab], self.search_result_mode);
        if chips.is_empty() {
            return;
        }
        let max_text_width = (ui.available_width() - 44.0).clamp(48.0, 240.0);
        let mut action = None;
        let mut keyboard = false;
        ui.horizontal_wrapped(|ui| {
            ui.add_space(8.0);
            let mut ask_heading = false;
            for chip in &chips {
                // Ask comparisons come last so their heading cannot make
                // ordinary search or column filters look assistant-generated.
                if matches!(chip.removal, ChipRemoval::Predicate(_)) && !ask_heading {
                    ui.label(
                        egui::RichText::new(t("search.ask_filters"))
                            .size(11.0)
                            .color(ui::theme::ThemeColors::for_mode(self.theme_mode).text_muted),
                    );
                    ask_heading = true;
                }
                let response = ui
                    .push_id(chip.removal, |ui| chip_button(ui, chip, max_text_width))
                    .inner;
                if response.clicked() {
                    action = Some(ChipAction::Remove(chip.removal));
                    keyboard = !response.clicked_by(egui::PointerButton::Primary);
                }
            }
            if chips.len() > 1 {
                let response = ui
                    .small_button(t("search.ask_filters_clear"))
                    .on_hover_text(t("search.ask_filters_clear_hint"));
                if response.clicked() {
                    action = Some(ChipAction::ClearAll);
                    keyboard = !response.clicked_by(egui::PointerButton::Primary);
                }
            }
        });
        ui.add_space(4.0);

        if let Some(action) = action {
            self.apply_chip_action(action);
            // The focused button is gone. Use the existing toolbar focus
            // route on the next frame; mouse clicks keep the grid's focus.
            self.search_focus_requested |= keyboard;
            ui.ctx().request_repaint();
        }
    }

    fn apply_chip_action(&mut self, action: ChipAction) {
        use ChipAction::{ClearAll, Remove};
        let tab = &self.tabs[self.active_tab];
        if tab.mark_filter_active && matches!(action, ClearAll | Remove(ChipRemoval::Marked)) {
            // This restores columns hidden by the mark filter without
            // revealing columns that were already manually hidden.
            self.toggle_filter_to_marked();
        }
        let tab = &mut self.tabs[self.active_tab];
        let clear_search = matches!(action, ClearAll | Remove(ChipRemoval::Search))
            && search_hides_rows(tab, self.search_result_mode);
        if clear_search {
            tab.search_text.clear();
            tab.search_nav.reset();
        }
        match action {
            ClearAll => {
                tab.predicate_filters.clear();
                tab.duplicate_filter = None;
                tab.duplicate_filter_cache = None;
                tab.column_filters.clear();
            }
            Remove(ChipRemoval::Predicate(i)) if i < tab.predicate_filters.len() => {
                tab.predicate_filters.remove(i);
            }
            Remove(ChipRemoval::Duplicate) => {
                tab.duplicate_filter = None;
                tab.duplicate_filter_cache = None;
            }
            Remove(ChipRemoval::Column(col)) => {
                tab.column_filters.remove(&col);
            }
            _ => {}
        }
        tab.filter_dirty = true;
    }
}

#[derive(Clone, Copy)]
enum ChipAction {
    Remove(ChipRemoval),
    ClearAll,
}

/// Which view filter a chip removes. No chip owns table rows or marks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ChipRemoval {
    Search,
    Marked,
    Predicate(usize),
    Duplicate,
    Column(usize),
}

struct FilterChip {
    label: String,
    removal: ChipRemoval,
    hint: &'static str,
}

impl FilterChip {
    fn new(label: String, removal: ChipRemoval, hint: &'static str) -> Self {
        Self {
            label,
            removal,
            hint,
        }
    }
}

fn filter_chips(tab: &TabState, mode: SearchResultMode) -> Vec<FilterChip> {
    let mut chips = Vec::new();
    if search_hides_rows(tab, mode) {
        let scope = tab
            .search_scope_col
            .and_then(|col| tab.table.columns.get(col));
        let title = t("menu.search");
        let needle = chip_excerpt(&tab.search_text);
        let label = match scope {
            Some(col) => format!("{title} / {}: {needle}", chip_excerpt(&col.name)),
            None => format!("{title}: {needle}"),
        };
        chips.push(FilterChip::new(
            label,
            ChipRemoval::Search,
            "search.ask_filter_remove",
        ));
    }
    if tab.mark_filter_active {
        chips.push(FilterChip::new(
            t("edit_menu.filter_to_marked"),
            ChipRemoval::Marked,
            "edit_menu.filter_to_marked_clear",
        ));
    }
    if let Some(f) = &tab.duplicate_filter {
        let label = t(if f.keep_duplicates {
            "search.dupfilter_dups"
        } else {
            "search.dupfilter_unique"
        });
        chips.push(FilterChip::new(
            label,
            ChipRemoval::Duplicate,
            "search.ask_filter_remove",
        ));
    }
    chips.extend(
        column_filter_chip_labels(&tab.table, &tab.column_filters)
            .into_iter()
            .map(|(col, label)| {
                FilterChip::new(label, ChipRemoval::Column(col), "facet.chip_remove")
            }),
    );
    chips.extend(tab.predicate_filters.iter().enumerate().map(|(i, f)| {
        FilterChip::new(
            chip_excerpt(&f.label(&tab.table)),
            ChipRemoval::Predicate(i),
            "search.ask_filter_remove",
        )
    }));
    chips
}

fn chip_excerpt(text: &str) -> String {
    let mut chars = text.chars();
    let mut out: String = chars
        .by_ref()
        .take(120)
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    if chars.next().is_some() {
        out.push_str("...");
    }
    out
}

fn chip_button(ui: &mut egui::Ui, chip: &FilterChip, max_text_width: f32) -> egui::Response {
    let font = egui::TextStyle::Small.resolve(ui.style());
    let fmt = egui::TextFormat::simple(font, ui.visuals().text_color());
    let mut job = egui::text::LayoutJob::simple_format(chip.label.clone(), fmt);
    job.wrap.max_width = max_text_width;
    job.wrap.max_rows = 1;
    let text = ui.fonts_mut(|f| f.layout_job(job));
    let close = ui::icons::icon_image(ui, ui::icons::Icon::Close)
        .fit_to_exact_size(egui::Vec2::splat(11.0));
    let response = ui.add(
        egui::Button::new(egui::WidgetText::Galley(text))
            .small()
            .min_size(egui::vec2(0.0, 22.0))
            .right_text(close),
    );
    let hint = t(chip.hint);
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            true,
            format!("{}: {}", hint.trim_end_matches('.'), chip.label),
        )
    });
    response.on_hover_text(format!("{}\n{hint}", chip.label))
}

/// Chip text for each active column filter, in COLUMN order (a `HashMap`
/// iterates in whatever order it likes, and a row that reshuffles itself
/// between frames is unusable).
///
/// A single allowed value reads as the value itself; several read as a
/// count, because a chip listing fifteen values is not a chip any more. A
/// filter whose column has since been deleted is skipped rather than
/// rendered nameless.
pub(crate) fn column_filter_chip_labels(
    table: &octa::data::DataTable,
    filters: &std::collections::HashMap<usize, std::collections::HashSet<String>>,
) -> Vec<(usize, String)> {
    let mut cols: Vec<usize> = filters.keys().copied().collect();
    cols.sort_unstable();
    cols.into_iter()
        .filter_map(|col| {
            let name = chip_excerpt(&table.columns.get(col)?.name);
            let values = filters.get(&col)?;
            let label = match values.len() {
                1 => {
                    let value = values.iter().next()?;
                    let excerpt = if value.is_empty() {
                        t("dialog.cf_empty")
                    } else {
                        chip_excerpt(value)
                    };
                    format!("{name}: {excerpt}")
                }
                n => format!(
                    "{name}: {}",
                    octa::i18n::t("facet.chip_values").replace("{count}", &n.to_string())
                ),
            };
            Some((col, label))
        })
        .collect()
}

#[cfg(test)]
#[path = "chips_chip_row_tests.rs"]
mod chip_row_tests;
