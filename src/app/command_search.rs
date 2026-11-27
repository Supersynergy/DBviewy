//! Find a command (Ctrl+K): a window searching every command. Picking one
//! queues it (`shortcuts::queue_command`) and it runs next frame as its shortcut
//! would, except that a focused text field does not hold it back
//! (`shortcuts::command_firing`): the user asked for it by name.

use eframe::egui;
use egui::text::{LayoutJob, TextFormat};
use strum::IntoEnumIterator;

use octa::i18n::{t, t_english};
use octa::ui::command_match::rank;
use octa::ui::settings::{
    DialogSize, center_on_first_show, dialog_header, remember_dialog_rect, size_dialog_window,
};
use octa::ui::shortcuts::{ShortcutAction as SA, queue_command};

use super::state::OctaApp;

fn window_id() -> egui::Id {
    egui::Id::new("octa_command_search")
}

fn size_key() -> egui::Id {
    window_id().with("octa_dlg_size")
}

/// The search field. Its focus decides who gets the arrow keys, Enter and Escape.
fn field_id() -> egui::Id {
    window_id().with("query")
}

#[derive(Default)]
pub(crate) struct CommandSearchState {
    query: String,
    selected: usize,
    scroll_y: f32,
    prev_focus: Option<egui::Id>,
    focus_set: bool,
}

/// Features that live inside a dialog and have no command of their own: the
/// name the dialog gives them, and the command that opens that dialog. Find a
/// command lists each under its own name. A new kind, method or option inside
/// a dialog gets a line here (CLAUDE.md, "Findable by name").
pub(crate) const INSIDE: &[(&str, SA)] = &[
    ("instant_search.placeholder", SA::FocusSearch),
    ("chart.seasonality_to_table", SA::OpenChart),
    ("gui.kind_pareto", SA::OpenChart),
    ("dialog.tc_kind_working_days", SA::OpenTimeCalc),
    ("dialog.vf_count_words", SA::ColumnValueFrequency),
    ("dialog.vf_count_pairs", SA::ColumnValueFrequency),
    ("outliers.method_neighbours", SA::OpenOutliers),
    ("calc.kind_previous", SA::OpenCalculatedColumn),
    ("calc.kind_next", SA::OpenCalculatedColumn),
    ("calc.kind_session", SA::OpenCalculatedColumn),
    ("calc.kind_abc", SA::OpenCalculatedColumn),
    ("refdata.mode_currency", SA::OpenReferenceData),
    ("refdata.mode_inflation", SA::OpenReferenceData),
    ("online.inflation", SA::OpenReferenceData),
    ("refdata.mode_places", SA::OpenReferenceData),
    ("refdata.key_postcode", SA::OpenReferenceData),
    ("refdata.key_kreis", SA::OpenReferenceData),
    ("refdata.key_nuts", SA::OpenReferenceData),
    // Cell right-click file actions on a path the cell text points at.
    ("context_menu.open_file", SA::OpenFile),
    ("context_menu.reveal_file", SA::OpenFile),
    ("context_menu.open_file_in_tab", SA::OpenFile),
    ("context_menu.copy_file_path", SA::OpenFile),
];

/// One listed command, in rank order.
struct Row {
    action: SA,
    /// The catalogue key the hover text hangs off (`<key>_hint`).
    hint_of: &'static str,
    name: String,
    place: String,
    keys: String,
    why: Option<String>,
}

struct RowStyle {
    title: egui::FontId,
    context: egui::FontId,
    height: f32,
}

impl RowStyle {
    fn new(ui: &egui::Ui) -> Self {
        Self {
            title: egui::TextStyle::Button.resolve(ui.style()),
            context: egui::TextStyle::Small.resolve(ui.style()),
            // A long name may use both lines. Reserve two lines of the
            // larger font so wrapping never changes the virtual row pitch.
            height: (2.0
                * ui.text_style_height(&egui::TextStyle::Button)
                    .max(ui.text_style_height(&egui::TextStyle::Small))
                + 2.0 * ui.spacing().button_padding.y
                + 4.0)
                .ceil()
                .max(ui.spacing().interact_size.y),
        }
    }

    fn show(&self, ui: &mut egui::Ui, row: &Row, selected: bool) -> egui::Response {
        let weak = ui.visuals().weak_text_color();
        let key = (!row.keys.is_empty()).then(|| {
            ui.fonts_mut(|f| f.layout_no_wrap(row.keys.clone(), self.context.clone(), weak))
        });
        let gap = 12.0;
        // The shortcut adds both a growing spacer and the shortcut atom.
        let reserved = key.as_ref().map_or(0.0, |g| g.size().x + 2.0 * gap);
        let mut job = LayoutJob::default();
        job.wrap.max_width =
            (ui.available_width() - 2.0 * ui.spacing().button_padding.x - reserved - 4.0).max(1.0);
        job.wrap.max_rows = 2;
        let primary = TextFormat::simple(self.title.clone(), ui.visuals().strong_text_color());
        let secondary = TextFormat::simple(self.context.clone(), weak);
        job.append(&row.name, 0.0, primary);
        if !row.place.is_empty() {
            job.append("\n", 0.0, secondary.clone());
            job.append(&row.place, 0.0, secondary);
        }
        // Pre-layout so the button does not replace the two-line wrap limit.
        let text = ui.fonts_mut(|f| f.layout_job(job));
        let mut button = egui::Button::selectable(selected, egui::WidgetText::Galley(text))
            .min_size(egui::vec2(0.0, self.height))
            .gap(gap);
        if let Some(key) = key {
            button = button.shortcut_text(egui::WidgetText::Galley(key));
        }
        // A command keeps its UI identity if filtering moves it to another
        // position. A mouse-up cannot activate a new result in the old slot.
        let response = ui
            .push_id((row.action, row.hint_of), |ui| {
                ui.add_enabled(row.why.is_none(), button)
            })
            .inner;
        let hint_key = format!("{}_hint", row.hint_of);
        let hint = t(&hint_key);
        if let Some(why) = &row.why {
            response.on_disabled_hover_text(format!("{}\n{why}", row.name))
        } else if hint != hint_key {
            response.on_hover_text(format!("{}\n{hint}", row.name))
        } else {
            response.on_hover_text(&row.name)
        }
    }
}

impl OctaApp {
    pub(crate) fn open_command_search(&mut self, ctx: &egui::Context) {
        // Already open, perhaps minimised or with focus elsewhere: bring it back.
        if let Some(st) = &mut self.command_search {
            st.focus_set = false;
            if ctx.data(|d| d.get_temp::<DialogSize>(size_key())) == Some(DialogSize::Minimized) {
                ctx.data_mut(|d| d.insert_temp(size_key(), DialogSize::Normal));
            }
            return;
        }
        self.command_search = Some(CommandSearchState {
            prev_focus: ctx.memory(|m| m.focused()),
            ..Default::default()
        });
    }

    /// Why `a` cannot run now, in the menus' own words. The dispatch
    /// conditions in `shortcuts_dispatch.rs` stay authoritative; this only
    /// greys the row.
    pub(crate) fn command_unavailable(&self, a: SA) -> Option<String> {
        let tab = self.tabs.get(self.active_tab);
        let has_table = tab.is_some_and(|t| t.table.col_count() > 0);
        let need_table = || (!has_table).then(|| t("greyed.need_table"));
        let need_edit = || {
            need_table().or_else(|| {
                self.is_readonly()
                    .then(|| t("context_menu.header_row_readonly"))
            })
        };
        let need_db =
            || (!tab.is_some_and(|t| t.db_origin.is_some())).then(|| t("greyed.need_db_tab"));
        let need_file =
            || (!tab.is_some_and(|t| t.table.source_path.is_some())).then(|| t("greyed.need_file"));
        match a {
            SA::ToggleFocusGuides => need_table().or_else(|| tab?.focus_guides_block().map(t)),
            SA::FillDown | SA::FillRight => {
                use super::fill_down::Direction;
                let direction = if a == SA::FillRight {
                    Direction::Right
                } else {
                    Direction::Down
                };
                need_edit().or_else(|| tab?.fill_block(direction).map(t))
            }
            SA::DeleteRow
            | SA::DuplicateRow
            | SA::InsertRowBelow
            | SA::EditCell
            | SA::LowercaseSelection
            | SA::UppercaseSelection
            | SA::Cut
            | SA::Paste
            | SA::OpenTransform
            | SA::OpenConditionalColumn
            | SA::OpenHashColumns
            | SA::OpenCalculatedColumn
            | SA::OpenFormulaExamples
            | SA::OpenClassify
            | SA::OpenAnonymize
            | SA::OpenDedupe
            | SA::OpenImpute
            | SA::OpenRetypeColumn
            | SA::OpenRenameColumns
            | SA::OpenTidyUp
            | SA::OpenTimeCalc
            | SA::FillSeries
            | SA::ApplyRecipe => need_edit(),
            SA::OpenPivot
            | SA::OpenColumnFilter
            | SA::OpenConditionalFormat
            | SA::OpenValidation
            | SA::OpenMultiSort
            | SA::OpenNumberFormat
            | SA::OpenCleanupPanel
            | SA::OpenTimeseries
            | SA::OpenCorrelation
            | SA::OpenLookups
            | SA::OpenDistCompare
            | SA::OpenReferential
            | SA::OpenTranspose
            | SA::OpenRowCompare
            | SA::OpenRandomSample
            | SA::OpenChart
            | SA::OpenSummary
            | SA::OpenQualityReport
            | SA::ColumnValueFrequency
            | SA::FindDuplicates
            | SA::OpenFuzzyDuplicates
            | SA::OpenOutliers
            | SA::OpenPii
            | SA::ExportPdf
            | SA::SaveFileAs
            | SA::SaveTableToDb
            | SA::ExportSchema
            | SA::CopyAsMarkdown
            | SA::CopyAsInList
            | SA::CopyAsJson
            | SA::CopyAsInsert
            | SA::Copy
            | SA::Mark
            | SA::FilterToMarked
            | SA::FitAllColumns
            | SA::FitAllRows
            | SA::SelectAllRows
            | SA::GoToCell
            | SA::OpenPartition
            | SA::OpenTestData => need_table(),
            SA::FilterToCellValue => self.selected_cell_filter().err().map(t),
            SA::SaveDbSql | SA::RunSqlOnServer => need_db(),
            SA::ToggleFollow
            | SA::OpenFileVersions
            | SA::OpenTableFolder
            | SA::OpenGitCompare
            | SA::OpenFileInternals => need_file(),
            _ => None,
        }
    }

    /// The commands matching the query, best first: names starting with it,
    /// then the rest, each in the order the actions are declared.
    fn command_rows(&self, query: &str) -> Vec<Row> {
        let shortcuts = &self.settings.shortcuts;
        // (action, key of the name, name, where it lives, English name)
        let commands = SA::iter().filter(|a| a.in_command_search()).map(|a| {
            let place = t(a.place_key());
            (
                a,
                a.name_key(),
                a.display_name(),
                place,
                a.label().to_string(),
            )
        });
        let inside = INSIDE
            .iter()
            .map(|&(key, a)| (a, key, t(key), a.display_name(), t_english(key)));
        let mut rows: Vec<(u8, usize, Row)> = commands
            .chain(inside)
            .enumerate()
            .filter_map(|(i, (a, hint_of, name, place, english))| {
                let r = rank(query, &name, &place, &english)?;
                let combo = shortcuts.combo(a);
                let keys = if combo.key.is_some() {
                    combo.label()
                } else {
                    String::new()
                };
                let why = self.command_unavailable(a);
                Some((
                    r,
                    i,
                    Row {
                        action: a,
                        hint_of,
                        name,
                        place,
                        keys,
                        why,
                    },
                ))
            })
            .collect();
        rows.sort_by_key(|r| (r.0, r.1));
        rows.into_iter().map(|(_, _, row)| row).collect()
    }

    pub(crate) fn render_command_search(&mut self, ctx: &egui::Context) {
        let Some(mut st) = self.command_search.take() else {
            return;
        };
        // Read before the box draws, so its text field never sees them, and only
        // while that field has focus: the box can stay open beside other work,
        // and these are the table's keys too.
        let (down, up, enter, escape) = if ctx.memory(|m| m.has_focus(field_id())) {
            ctx.input_mut(|i| {
                (
                    i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
                    i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                    i.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
                    i.consume_key(egui::Modifiers::NONE, egui::Key::Escape),
                )
            })
        } else {
            Default::default()
        };
        let mut run: Option<SA> = None;
        let mut close = escape;
        let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key()).unwrap_or_default());
        let minimized = size == DialogSize::Minimized;
        let max_content_width = (ctx.content_rect().width() - 24.0).max(120.0);
        let default_width = max_content_width.min(560.0);
        let center = center_on_first_show(ctx, egui::vec2(default_width, 440.0));
        let window = egui::Window::new("octa_command_search")
            .id(window_id())
            .title_bar(false)
            .collapsible(false)
            // Over every other dialog, so a command can be found whatever is open.
            .order(egui::Order::Foreground);
        let window = size_dialog_window(ctx, window_id(), size, window, |w| {
            w.resizable(true)
                .default_width(default_width)
                .default_height(440.0)
                .min_width(max_content_width.min(320.0))
                .max_width(max_content_width)
                .min_height(200.0)
                .default_pos(center)
        });
        let inner = window.show(ctx, |ui| {
            if dialog_header(
                ui,
                "octa_command_search_header",
                egui::RichText::new(t("command_search.title")),
                &mut size,
            ) {
                close = true;
            }
            if minimized {
                return;
            }
            egui::CentralPanel::default().show(ui, |ui| {
                let edit = ui
                    .add(
                        egui::TextEdit::singleline(&mut st.query)
                            .id(field_id())
                            // Claim Escape, or egui drops the focus at the start of
                            // the frame and the check above never sees the key.
                            .event_filter(egui::EventFilter {
                                horizontal_arrows: true,
                                vertical_arrows: true,
                                escape: true,
                                ..Default::default()
                            })
                            .hint_text(t("command_search.placeholder"))
                            .desired_width(f32::INFINITY),
                    )
                    .on_hover_text(t("command_search.box_hint"));
                if !st.focus_set {
                    edit.request_focus();
                    st.focus_set = true;
                }
                if edit.changed() {
                    st.selected = 0;
                    st.scroll_y = 0.0;
                }

                // TextEdit may receive text or a paste in the same frame as
                // Enter. Match the new text before drawing or running a row.
                let rows = self.command_rows(&st.query);
                let last = rows.len().saturating_sub(1);
                st.selected = st.selected.min(last);
                if down {
                    st.selected = (st.selected + 1).min(last);
                }
                if up {
                    st.selected = st.selected.saturating_sub(1);
                }
                if !close
                    && enter
                    && let Some(row) = rows.get(st.selected).filter(|r| r.why.is_none())
                {
                    run = Some(row.action);
                }

                if rows.is_empty() {
                    ui.weak(t("command_search.none"));
                    return;
                }
                let style = RowStyle::new(ui);
                let pitch = style.height + ui.spacing().item_spacing.y;
                let height = ui.available_height();
                if up || down {
                    let top = st.selected as f32 * pitch;
                    let bottom = top + style.height;
                    if top < st.scroll_y {
                        st.scroll_y = top;
                    } else if bottom > st.scroll_y + height {
                        st.scroll_y = (bottom - height).max(0.0);
                    }
                }
                let list = egui::ScrollArea::vertical()
                    .id_salt("command_results")
                    .auto_shrink([false, false])
                    .animated(false)
                    .vertical_scroll_offset(st.scroll_y)
                    .show_rows(ui, style.height, rows.len(), |ui, visible| {
                        ui.with_layout(egui::Layout::top_down_justified(egui::Align::Min), |ui| {
                            for idx in visible {
                                let row = &rows[idx];
                                if style.show(ui, row, idx == st.selected).clicked() && !close {
                                    run = Some(row.action);
                                }
                            }
                        });
                    });
                st.scroll_y = list.state.offset.y;
            });
        });
        if let Some(inner) = inner {
            remember_dialog_rect(ctx, window_id(), size, inner.response.rect);
        }
        if let Some(a) = run {
            queue_command(a);
            close = true;
            ctx.request_repaint();
        }
        ctx.data_mut(|d| d.insert_temp(size_key(), if close { DialogSize::Normal } else { size }));
        if close {
            if let Some(id) = st.prev_focus {
                ctx.memory_mut(|m| m.request_focus(id));
            } else {
                ctx.memory_mut(|m| m.stop_text_input());
            }
            return;
        }
        self.command_search = Some(st);
    }
}

#[cfg(test)]
#[path = "command_search_tests.rs"]
mod tests;
