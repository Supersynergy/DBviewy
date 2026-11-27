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
    Trimmed,
    Round,
    Scalar,
    Exact,
    Find(&'static str),
    Rank,
    Nth,
    Percentile,
    Paired,
    ResponseFirst,
    Forecast,
    Workday,
    MonthShift,
    MonthEnd,
    NumberValue,
    PeriodicValue,
    DatedValue,
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
        english: "GEOMEAN",
        german: "GEOMITTEL",
        form: Form::Whole,
    },
    Recipe {
        english: "HARMEAN",
        german: "HARMITTEL",
        form: Form::Whole,
    },
    Recipe {
        english: "TRIMMEAN",
        german: "GESTUTZTMITTEL",
        form: Form::Trimmed,
    },
    Recipe {
        english: "SUMSQ",
        german: "QUADRATESUMME",
        form: Form::Whole,
    },
    Recipe {
        english: "SQRT",
        german: "WURZEL",
        form: Form::Scalar,
    },
    Recipe {
        english: "SUMXMY2",
        german: "SUMMEXMY2",
        form: Form::Paired,
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
        form: Form::Scalar,
    },
    Recipe {
        english: "NPV",
        german: "NBW",
        form: Form::PeriodicValue,
    },
    Recipe {
        english: "XNPV",
        german: "XKAPITALWERT",
        form: Form::DatedValue,
    },
    Recipe {
        english: "LEN",
        german: "LÄNGE",
        form: Form::Scalar,
    },
    Recipe {
        english: "EXACT",
        german: "IDENTISCH",
        form: Form::Exact,
    },
    Recipe {
        english: "FIND",
        german: "FINDEN",
        form: Form::Find("-"),
    },
    Recipe {
        english: "SEARCH",
        german: "SUCHEN",
        form: Form::Find("sku-??"),
    },
    Recipe {
        english: "RANK.EQ",
        german: "RANG.GLEICH",
        form: Form::Rank,
    },
    Recipe {
        english: "RANK.AVG",
        german: "RANG.MITTELW",
        form: Form::Rank,
    },
    Recipe {
        english: "LARGE",
        german: "KGRÖSSTE",
        form: Form::Nth,
    },
    Recipe {
        english: "SMALL",
        german: "KKLEINSTE",
        form: Form::Nth,
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
        english: "CORREL",
        german: "KORREL",
        form: Form::Paired,
    },
    Recipe {
        english: "COVARIANCE.P",
        german: "KOVARIANZ.P",
        form: Form::Paired,
    },
    Recipe {
        english: "COVARIANCE.S",
        german: "KOVARIANZ.S",
        form: Form::Paired,
    },
    Recipe {
        english: "SLOPE",
        german: "STEIGUNG",
        form: Form::ResponseFirst,
    },
    Recipe {
        english: "INTERCEPT",
        german: "ACHSENABSCHNITT",
        form: Form::ResponseFirst,
    },
    Recipe {
        english: "RSQ",
        german: "BESTIMMTHEITSMASS",
        form: Form::ResponseFirst,
    },
    Recipe {
        english: "FORECAST.LINEAR",
        german: "PROGNOSE.LINEAR",
        form: Form::Forecast,
    },
    Recipe {
        english: "NUMBERVALUE",
        german: "ZAHLENWERT",
        form: Form::NumberValue,
    },
    Recipe {
        english: "EDATE",
        german: "EDATUM",
        form: Form::MonthShift,
    },
    Recipe {
        english: "EOMONTH",
        german: "MONATSENDE",
        form: Form::MonthEnd,
    },
    Recipe {
        english: "ISOWEEKNUM",
        german: "ISOKALENDERWOCHE",
        form: Form::Scalar,
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

    fn uses_pair(self) -> bool {
        matches!(
            self.form,
            Form::Paired | Form::ResponseFirst | Form::Forecast | Form::DatedValue
        )
    }

    fn expression(
        self,
        col: usize,
        partner: Option<usize>,
        loaded: usize,
        german: bool,
    ) -> Option<String> {
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
            Form::Trimmed => format!("${c}$1:${c}${n}{sep}20%"),
            Form::Round => format!("{c}1{sep}2"),
            Form::Scalar => format!("{c}1"),
            Form::Exact => format!(r#"{c}1{sep}"SKU-01""#),
            Form::Find(pattern) => {
                let fallback = if german { "WENNFEHLER" } else { "IFERROR" };
                return Some(format!(
                    r#"={fallback}({name}("{pattern}"{sep}{c}1){sep}0)"#
                ));
            }
            Form::Rank => format!("{c}1{sep}${c}$1:${c}${n}"),
            Form::Nth => format!("${c}$1:${c}${n}{sep}{}", n.min(2)),
            Form::PeriodicValue => format!("10%{sep}${c}$1:${c}${n}"),
            Form::Paired | Form::ResponseFirst | Form::Forecast | Form::DatedValue => {
                let other = partner.filter(|&other| other != col)?;
                let y = letters(other);
                let x_range = format!("${c}$1:${c}${n}");
                let y_range = format!("${y}$1:${y}${n}");
                match self.form {
                    // Excel uses the response first, then the predictor.
                    Form::ResponseFirst => format!("{y_range}{sep}{x_range}"),
                    Form::Forecast => format!("{c}1{sep}{y_range}{sep}{x_range}"),
                    Form::DatedValue => format!("10%{sep}{x_range}{sep}{y_range}"),
                    _ => format!("{x_range}{sep}{y_range}"),
                }
            }
            Form::Percentile => format!("${c}$1:${c}${n}{sep}95%"),
            Form::Workday => format!("{c}1{sep}5"),
            Form::MonthShift => format!("{c}1{sep}1"),
            Form::MonthEnd => format!("{c}1{sep}0"),
            Form::NumberValue if german => format!(r#"{c}1;",";".""#),
            Form::NumberValue => format!(r#"{c}1,".",",""#),
        };
        Some(format!("={name}({args})"))
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
    // Excel's German formula names spell the sharp s as SS in caps.
    let folded;
    let query = if query.contains(['ß', 'ẞ']) {
        folded = query.replace(['ß', 'ẞ'], "ss");
        &folded
    } else {
        query
    };
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
    partner: Option<usize>,
    needs_focus: bool,
}

impl FormulaExamples {
    pub(super) fn open(tab: &TabState) -> Option<Self> {
        Some(Self {
            query: String::new(),
            selected: 0,
            source: first_source(tab)?,
            partner: None,
            needs_focus: true,
        })
    }
}

fn source_label(tab: &TabState, col: usize) -> String {
    format!("{}  |  {}", letters(col), tab.table.columns[col].name)
}

fn partner_source(tab: &TabState, primary: usize, preferred: Option<usize>) -> Option<usize> {
    preferred
        .filter(|&col| col != primary && visible(tab, col))
        .or_else(|| (0..tab.table.col_count()).find(|&col| col != primary && visible(tab, col)))
}

fn choose_column(
    ui: &mut egui::Ui,
    tab: &TabState,
    title: &str,
    id: &str,
    selected: Option<usize>,
    excluded: Option<usize>,
) -> Option<usize> {
    let mut choice = selected;
    control_line(ui, |ui| {
        let hint = t("dialog.formula_examples_column_hint");
        let label = ui.label(title).on_hover_text(&hint);
        let width = (ui.available_width() - 8.0).max(80.0);
        let response = ui
            .add_enabled_ui(selected.is_some(), |ui| {
                egui::ComboBox::from_id_salt(id)
                    .width(width)
                    .truncate()
                    .selected_text(
                        selected.map_or_else(|| t("greyed.none"), |col| source_label(tab, col)),
                    )
                    .show_ui(ui, |ui| {
                        let columns: Vec<_> = (0..tab.table.col_count())
                            .filter(|&col| Some(col) != excluded && visible(tab, col))
                            .collect();
                        egui::ScrollArea::vertical().max_height(190.0).show_rows(
                            ui,
                            ui.spacing().interact_size.y,
                            columns.len(),
                            |ui, rows| {
                                for row in rows {
                                    let col = columns[row];
                                    ui.selectable_value(
                                        &mut choice,
                                        Some(col),
                                        source_label(tab, col),
                                    );
                                }
                            },
                        );
                    })
                    .response
            })
            .inner
            .labelled_by(label.id);
        if selected.is_some() {
            response.on_hover_text(hint);
        } else {
            response.on_disabled_hover_text(disabled_tip(&hint, &missing_partner(tab)));
        }
    });
    choice
}

fn missing_partner(tab: &TabState) -> String {
    if tab.table.col_count() > 1 {
        t("edit_menu.show_hidden_columns")
    } else {
        t("greyed.need_two_cols")
    }
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
        let sample_partner = partner_source(tab, st.source, st.partner);
        let reserve = if recipes.get(st.selected).is_some_and(|r| r.uses_pair()) {
            312.0
        } else {
            278.0
        };
        let max_height = (ctx.content_rect().height() - reserve).clamp(44.0, 188.0);
        egui::ScrollArea::vertical()
            .max_height(max_height)
            .show(ui, |ui| {
                ui.with_layout(egui::Layout::top_down_justified(egui::Align::Min), |ui| {
                    if recipes.is_empty() {
                        ui.weak(t("search.no_matches"));
                    }
                    for (i, recipe) in recipes.iter().copied().enumerate() {
                        let row =
                            ui.add(egui::Button::selectable(i == st.selected, recipe.label()));
                        let row = match recipe.expression(st.source, sample_partner, loaded, german)
                        {
                            Some(sample) => row.on_hover_text(sample),
                            None => row.on_hover_text(missing_partner(tab)),
                        };
                        if row.clicked() {
                            st.selected = i;
                        }
                        if (up || down) && i == st.selected {
                            row.scroll_to_me(None);
                        }
                    }
                });
            });
        let recipe = recipes.get(st.selected).copied();
        let paired = recipe.is_some_and(Recipe::uses_pair);
        let column_label = t("dialog.date_column");
        if let Some(recipe) = recipe {
            let dated = matches!(recipe.form, Form::DatedValue);
            let x_label = if matches!(recipe.form, Form::PeriodicValue | Form::DatedValue) {
                t("rfm.amount")
            } else if paired {
                format!("{column_label} (X)")
            } else {
                column_label.clone()
            };
            if let Some(primary) = choose_column(
                ui,
                tab,
                &x_label,
                "formula_example_source",
                Some(st.source),
                None,
            ) {
                st.source = primary;
            }
            if paired {
                let available = partner_source(tab, st.source, st.partner);
                let y_label = if dated {
                    t("rfm.date")
                } else {
                    format!("{column_label} (Y)")
                };
                st.partner = choose_column(
                    ui,
                    tab,
                    &y_label,
                    "formula_example_partner",
                    available,
                    Some(st.source),
                );
            }
        }
        let expression = recipe.and_then(|r| r.expression(st.source, st.partner, loaded, german));
        if let Some(draft) = &expression {
            ui.add(egui::Label::new(RichText::new(draft).monospace()).truncate())
                .on_hover_text(draft);
        } else if paired {
            ui.weak(missing_partner(tab));
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
                let reason = if paired {
                    missing_partner(tab)
                } else {
                    t("search.no_matches")
                };
                apply.on_disabled_hover_text(disabled_tip(&hint, &reason))
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
