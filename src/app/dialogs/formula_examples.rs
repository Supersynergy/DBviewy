//! Select an example formula without applying it to the table.

use eframe::egui;
use egui::RichText;

use octa::i18n::{current_language, t};
use octa::ui::command_match::rank;
use octa::ui::control_row::{control_line, control_text_edit};
use octa::ui::menu_entry::disabled_tip;

use super::TabState;

#[derive(Clone, Copy)]
enum Form {
    Running,
    Whole,
    Round,
    Truncate,
    Percentile,
    Workday,
    NumberValue,
}

#[derive(Clone, Copy)]
struct Recipe {
    english: &'static str,
    german: &'static str,
    form: Form,
}

const RECIPES: &[Recipe] = &[
    Recipe {
        english: "SUM",
        german: "SUMME",
        form: Form::Running,
    },
    Recipe {
        english: "AVERAGE",
        german: "MITTELWERT",
        form: Form::Running,
    },
    Recipe {
        english: "COUNT",
        german: "ANZAHL",
        form: Form::Running,
    },
    Recipe {
        english: "COUNTBLANK",
        german: "ANZAHLLEEREZELLEN",
        form: Form::Running,
    },
    Recipe {
        english: "ROUND",
        german: "RUNDEN",
        form: Form::Round,
    },
    Recipe {
        english: "ROUNDUP",
        german: "AUFRUNDEN",
        form: Form::Round,
    },
    Recipe {
        english: "ROUNDDOWN",
        german: "ABRUNDEN",
        form: Form::Round,
    },
    Recipe {
        english: "TRUNC",
        german: "KÜRZEN",
        form: Form::Truncate,
    },
    Recipe {
        english: "PERCENTILE.INC",
        german: "QUANTIL.INKL",
        form: Form::Percentile,
    },
    Recipe {
        english: "STDEV.S",
        german: "STABW.S",
        form: Form::Running,
    },
    Recipe {
        english: "VAR.P",
        german: "VAR.P",
        form: Form::Whole,
    },
    Recipe {
        english: "NUMBERVALUE",
        german: "ZAHLENWERT",
        form: Form::NumberValue,
    },
    Recipe {
        english: "WORKDAY",
        german: "ARBEITSTAG",
        form: Form::Workday,
    },
];

impl Recipe {
    fn label(self) -> String {
        if self.english == self.german {
            self.english.to_owned()
        } else {
            format!("{} / {}", self.english, self.german)
        }
    }

    fn expression(self, col: usize, loaded: usize, german: bool) -> String {
        let c = letters(col);
        let n = loaded.max(1);
        let (name, sep) = if german {
            (self.german, ';')
        } else {
            (self.english, ',')
        };
        let args = match self.form {
            Form::Running => format!("{c}$1:{c}1"),
            Form::Whole => format!("${c}$1:${c}${n}"),
            Form::Round => format!("{c}1{sep}2"),
            Form::Truncate => format!("{c}1"),
            Form::Percentile => format!("${c}$1:${c}${n}{sep}95%"),
            Form::Workday => format!("{c}1{sep}5"),
            Form::NumberValue if german => format!(r#"{c}1;",";".""#),
            Form::NumberValue => format!(r#"{c}1,".",",""#),
        };
        format!("={name}({args})")
    }
}

fn letters(mut col: usize) -> String {
    let mut bytes = Vec::new();
    loop {
        bytes.push(b'A' + (col % 26) as u8);
        if col < 26 {
            break;
        }
        col = col / 26 - 1;
    }
    bytes.into_iter().rev().map(char::from).collect()
}

fn matching(query: &str) -> Vec<Recipe> {
    let query = query.trim().trim_start_matches('=');
    let mut hits: Vec<_> = RECIPES
        .iter()
        .copied()
        .filter_map(|r| rank(query, r.english, "", r.german).map(|score| (score, r)))
        .collect();
    hits.sort_by_key(|(score, _)| *score);
    hits.into_iter().map(|(_, r)| r).collect()
}

fn visible(tab: &TabState, col: usize) -> bool {
    col < tab.table.col_count() && !tab.hidden_columns.contains(&col)
}

fn first_source(tab: &TabState) -> Option<usize> {
    tab.table_state
        .selected_cell
        .map(|(_, col)| col)
        .filter(|&c| visible(tab, c))
        .or_else(|| {
            tab.table_state
                .selected_cols
                .iter()
                .copied()
                .filter(|&c| visible(tab, c))
                .min()
        })
        .or_else(|| (0..tab.table.col_count()).find(|&c| visible(tab, c)))
}

pub(crate) struct FormulaExamples {
    query: String,
    selected: usize,
    source: usize,
    needs_focus: bool,
}

impl FormulaExamples {
    pub(super) fn open(tab: &TabState) -> Option<Self> {
        Some(Self {
            query: String::new(),
            selected: 0,
            source: first_source(tab)?,
            needs_focus: true,
        })
    }
}

fn source_label(tab: &TabState, col: usize) -> String {
    format!("{}  |  {}", letters(col), tab.table.columns[col].name)
}

pub(super) fn render(
    ctx: &egui::Context,
    tab: &mut TabState,
    formula_id: egui::Id,
    writable: bool,
) {
    let Some(mut st) = tab.new_col_examples.take() else {
        return;
    };
    if !writable {
        return;
    }
    if !visible(tab, st.source) {
        let Some(source) = first_source(tab) else {
            ctx.memory_mut(|m| m.request_focus(formula_id));
            return;
        };
        st.source = source;
    }
    let id = egui::Id::new(("dbviewy_formula_examples", tab.id));
    let search = id.with("query");
    let (up, down, enter) = if ctx.memory(|m| m.has_focus(search)) {
        ctx.input_mut(|input| {
            (
                input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
                input.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
            )
        })
    } else {
        (false, false, false)
    };
    let german = current_language() == "de";
    let popup = egui::Modal::new(id).show(ctx, |ui| {
        let width = (ctx.content_rect().width() - 44.0).clamp(220.0, 492.0);
        ui.set_width(width);
        let title = t("dialog.formula_examples");
        ui.heading(title.trim_end_matches("..."));
        ui.label(t("dialog.formula_examples_hint"));
        ui.separator();

        control_line(ui, |ui| {
            let hint = t("dialog.formula_examples_column_hint");
            let title = ui.label(t("dialog.date_column")).on_hover_text(&hint);
            egui::ComboBox::from_id_salt("formula_example_source")
                .width((width - 75.0).max(100.0))
                .selected_text(source_label(tab, st.source))
                .show_ui(ui, |ui| {
                    let columns: Vec<_> = (0..tab.table.col_count())
                        .filter(|&c| visible(tab, c))
                        .collect();
                    let row_height = ui.spacing().interact_size.y;
                    egui::ScrollArea::vertical().max_height(190.0).show_rows(
                        ui,
                        row_height,
                        columns.len(),
                        |ui, rows| {
                            for row in rows {
                                let c = columns[row];
                                ui.selectable_value(&mut st.source, c, source_label(tab, c));
                            }
                        },
                    );
                })
                .response
                .labelled_by(title.id)
                .on_hover_text(hint);
        });

        let name = ui.label(t("dialog.formula_examples_search"));
        let field = control_text_edit(
            ui,
            width,
            egui::TextEdit::singleline(&mut st.query)
                .id(search)
                .desired_width(width)
                .event_filter(egui::EventFilter {
                    vertical_arrows: true,
                    ..Default::default()
                }),
        )
        .labelled_by(name.id)
        .on_hover_text(t("dialog.formula_examples_search_hint"));
        if st.needs_focus {
            field.request_focus();
            st.needs_focus = false;
        }
        if field.changed() {
            st.selected = 0;
        }
        let recipes = matching(&st.query);
        if up {
            st.selected = st.selected.saturating_sub(1);
        }
        if down {
            st.selected += 1;
        }
        st.selected = st.selected.min(recipes.len().saturating_sub(1));
        let loaded = tab.table.row_count();
        let max_height = (ctx.content_rect().height() - 278.0).clamp(44.0, 188.0);
        egui::ScrollArea::vertical()
            .max_height(max_height)
            .show(ui, |ui| {
                ui.with_layout(egui::Layout::top_down_justified(egui::Align::Min), |ui| {
                    if recipes.is_empty() {
                        ui.weak(t("search.no_matches"));
                    }
                    for (i, recipe) in recipes.iter().copied().enumerate() {
                        let row = ui
                            .add(egui::Button::selectable(i == st.selected, recipe.label()))
                            .on_hover_text(recipe.expression(st.source, loaded, german));
                        if row.clicked() {
                            st.selected = i;
                        }
                        if (up || down) && i == st.selected {
                            row.scroll_to_me(None);
                        }
                    }
                });
            });
        let expression = recipes
            .get(st.selected)
            .map(|r| r.expression(st.source, loaded, german));
        if let Some(draft) = &expression {
            ui.add(egui::Label::new(RichText::new(draft).monospace()).truncate())
                .on_hover_text(draft);
        }
        ui.separator();
        control_line(ui, |ui| {
            let hint = t("dialog.formula_examples_use_hint");
            let apply = ui.add_enabled(
                expression.is_some(),
                egui::Button::new(t("dialog.formula_examples_use")),
            );
            let apply = if expression.is_some() {
                apply.on_hover_text(hint)
            } else {
                apply.on_disabled_hover_text(disabled_tip(&hint, &t("search.no_matches")))
            };
            let cancel = ui
                .button(t("common.cancel"))
                .on_hover_text(t("fill.cancel_hint"))
                .clicked();
            (
                if apply.clicked() || enter {
                    expression
                } else {
                    None
                },
                cancel,
            )
        })
        .inner
    });
    let dismissed = popup.should_close();
    let (draft, cancel) = popup.inner;
    if dismissed || cancel || draft.is_some() {
        ctx.input_mut(|input| {
            input.events.retain(|event| {
                !matches!(
                    event,
                    egui::Event::Key {
                        key: egui::Key::Enter | egui::Key::Space,
                        ..
                    }
                )
            })
        });
        if !dismissed
            && !cancel
            && let Some(draft) = draft
        {
            tab.new_col_formula = draft;
            tab.new_col_preview = None;
        }
        ctx.memory_mut(|m| m.request_focus(formula_id));
    } else {
        tab.new_col_examples = Some(st);
    }
}

#[cfg(test)]
#[path = "formula_examples_tests.rs"]
mod tests;
