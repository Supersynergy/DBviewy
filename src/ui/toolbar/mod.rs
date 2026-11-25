use egui::{RichText, Ui};

use crate::data::SearchMode;

mod analyse_menu;
mod columns_menu;
mod data_menu;
mod edit_menu;
mod file_menu;
pub use file_menu::clipboard_source;
mod help_menu;
mod instant_search;
#[cfg(test)]
mod instant_search_tests;
mod menu_button;
mod ribbon;
#[cfg(test)]
mod ribbon_tests;
mod search_menu;
mod types;
mod view_menu;

pub use ribbon::RIBBON_HEIGHT;
pub use types::{AskControls, FullScanKind, ParseScope, SearchControls, ToolbarAction, ToolbarCtx};

/// Width reserved for the ribbon fold button at the right edge of row 1.
const TOGGLE_W: f32 = 26.0;
/// Narrowest the search group is allowed to get before the menus scroll.
const SEARCH_MIN_W: f32 = 150.0;
/// Narrowest the command entry (Smartchat) is drawn at. It is never left out:
/// the menus scroll before it or the search give way.
const COMMAND_MIN_W: f32 = 108.0;

/// Formats offered by **File -> Open as...** and **View -> Reopen as...**:
/// `(i18n label key, reader name as registered in `FormatRegistry::new`)`.
///
/// These are the text-shaped readers, the ones worth forcing a file through when
/// its extension is misleading. Kept in step with the registry by
/// `open_as_tests::every_open_as_reader_name_resolves`.
const OPEN_AS_FORMATS: &[(&str, &str)] = &[
    ("open_as.json", "JSON"),
    ("open_as.jsonl", "JSON Lines"),
    ("open_as.csv", "CSV"),
    ("open_as.tsv", "TSV"),
    ("open_as.yaml", "YAML"),
    ("open_as.toml", "TOML"),
    ("open_as.xml", "XML"),
    ("open_as.markdown", "Markdown"),
    ("open_as.log", "Log (any format)"),
    ("open_as.text", "Text"),
    ("open_as.sql_dump", "SQL dump"),
];

/// Draw the toolbar and report what the user chose.
///
/// Takes two bundles rather than the 53 positional parameters it used to: a
/// `Copy` [`ToolbarCtx`] of everything the menus read, and a
/// [`SearchControls`] holding the search bar's `&mut` state. That is what
/// retired the `#[allow(clippy::too_many_arguments)]` this function carried,
/// and it means a new toolbar input is a named field rather than one more
/// `bool` in a positional list of a dozen others.
pub fn draw_toolbar(ui: &mut Ui, cx: ToolbarCtx<'_>, search: SearchControls<'_>) -> ToolbarAction {
    draw_toolbar_with_command(ui, cx, search, 0.0, |_| {})
}

/// Keep the command entry pinned while menus and search remain scrollable.
pub fn draw_toolbar_with_command(
    ui: &mut Ui,
    cx: ToolbarCtx<'_>,
    search: SearchControls<'_>,
    command_width: f32,
    command: impl FnOnce(&mut Ui),
) -> ToolbarAction {
    let mut action = ToolbarAction::default();
    // Destructured into locals with the original names so the bar's own body
    // below reads as it always did; `cx` stays whole (it is `Copy`) and is
    // handed to each menu.
    let ToolbarCtx {
        colors,
        logo_texture,
        show_window_controls,
        // The bar itself is hidden without a table; the menus stay visible.
        ribbon_open,
        ..
    } = cx;

    // Custom title bar: the toolbar background doubles as the window's drag
    // handle. Without system decorations this is the only way to move the
    // window (left-drag) or toggle maximise (double-click), matching the
    // standard title-bar gestures. We register this interaction FIRST, over
    // the whole toolbar rect, so the menus / search box / window-control
    // buttons added afterwards sit "on top" (egui breaks hit-test ties in
    // favour of the last-registered widget) and keep their own clicks - only
    // the leftover empty background drags the window.
    if show_window_controls {
        let title_rect = ui.max_rect();
        let drag = ui.interact(
            title_rect,
            ui.id().with("custom_title_bar_drag"),
            egui::Sense::click_and_drag(),
        );
        if drag.double_clicked() {
            let is_max = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Maximized(!is_max));
        } else if drag.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
    }

    // Row 1, left to right: logo, menus, search (field + one options menu +
    // match stepper), command entry, ribbon toggle, window buttons. Menus get
    // their measured width first, the search takes what is left, and only a
    // window too narrow for both scrolls the menus (wheel only, no bar).
    ui.horizontal(|ui| {
        // Pin a uniform row height for the whole toolbar so menus, field and
        // buttons share one centre line.
        ui.spacing_mut().interact_size.y = 24.0;
        // A plain (vertical) mouse wheel only reaches a horizontal-only scroll
        // area when this is set; only the narrow-window fallback needs it.
        ui.style_mut().always_scroll_the_only_direction = true;

        if let Some(tex) = logo_texture {
            let img = egui::Image::new(egui::load::SizedTexture::new(tex.id(), [20.0, 20.0]))
                .sense(egui::Sense::click());
            let resp = ui.add(img);
            if resp.clicked() {
                action.logo_clicked = true;
            }
        }
        ui.label(
            RichText::new("DBviewy")
                .strong()
                .size(15.0)
                .color(colors.accent),
        );
        ui.add_space(4.0);

        let sp = ui.spacing().item_spacing.x;
        // Window buttons plus the ribbon toggle stay pinned to the right edge.
        let reserved = if show_window_controls {
            3.0 * 28.0 + 3.0 * sp
        } else {
            0.0
        } + TOGGLE_W
            + sp;
        let total = (ui.available_width() - reserved - sp).max(0.0);
        let menus_id = ui.id().with("toolbar_menus_w");
        let menus_w: f32 = ui.data(|d| d.get_temp(menus_id)).unwrap_or(380.0);
        // Smartchat and the search keep their minimum; only the menus give way.
        let command_min = if command_width > 0.0 { COMMAND_MIN_W } else { 0.0 };
        let menus_fit = menus_w + SEARCH_MIN_W + command_min + 2.0 * sp <= total;
        let lane_w = if menus_fit {
            menus_w
        } else {
            (total - SEARCH_MIN_W - command_min - 2.0 * sp).max(60.0)
        };
        let room = (total - lane_w - sp).max(0.0);
        let command_w = if command_width > 0.0 {
            command_width
                .min(room - SEARCH_MIN_W - sp)
                .max(command_min)
                .min(room)
        } else {
            0.0
        };
        let search_block_w = (room - command_w - if command_w > 0.0 { sp } else { 0.0 }).max(0.0);

        // The eight top-level menus, in bar order. Each takes the same
        // `ToolbarCtx` and writes into the same `ToolbarAction`. Every menu
        // stays visible even before a table is open.
        let draw_menus = |ui: &mut Ui, action: &mut ToolbarAction| {
            let measured = ui
                .horizontal(|ui| {
                    file_menu::file_menu(ui, cx, action);
                    edit_menu::edit_menu(ui, cx, action);
                    columns_menu::columns_menu(ui, cx, action);
                    data_menu::data_menu(ui, cx, action);
                    view_menu::view_menu(ui, cx, action);
                    search_menu::search_menu(ui, cx, action);
                    analyse_menu::analyse_menu(ui, cx, action);
                    help_menu::help_menu(ui, cx, action);
                })
                .response
                .rect
                .width();
            ui.data_mut(|d| d.insert_temp(menus_id, measured));
        };
        if menus_fit {
            draw_menus(ui, &mut action);
        } else {
            egui::ScrollArea::horizontal()
                .id_salt("toolbar_scroll")
                .max_width(lane_w)
                .auto_shrink([false, false])
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                // Wheel only: the empty toolbar background is the window's
                // drag handle, and drag-to-scroll would swallow that gesture.
                .scroll_source(egui::scroll_area::ScrollSource {
                    drag: egui::scroll_area::DragScroll::Never,
                    ..egui::scroll_area::ScrollSource::ALL
                })
                .show(ui, |ui| draw_menus(ui, &mut action));
        }

        if search_block_w > 0.0 {
            ui.separator();
            ui.allocate_ui_with_layout(
                egui::vec2((search_block_w - sp).max(0.0), 24.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    draw_search(
                        ui,
                        cx,
                        search,
                        search_block_w,
                        &mut action,
                    );
                },
            );
        }
        ui.add_space(
            (ui.max_rect().right() - reserved - command_w - ui.cursor().left() - sp).max(0.0),
        );
        if command_w > 0.0 {
            ui.allocate_ui_with_layout(
                egui::vec2(command_w, 24.0),
                egui::Layout::left_to_right(egui::Align::Center),
                command,
            );
        }

        // Pinned to the far right: window buttons (custom title bar only, see
        // Settings -> Appearance) and, left of them, the ribbon fold button.
        // `right_to_left` lays them out in visual order `[_] [□] [x]`.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if show_window_controls {
                use crate::ui::dialog_chrome::{Caption, caption_button};
                let btn_size = egui::vec2(28.0, 24.0);
                let ctx = ui.ctx().clone();
                if caption_button(ui, Caption::Close, false, btn_size)
                    .on_hover_text(crate::i18n::t("settings_hint.ctrl_close"))
                    .clicked()
                {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                let is_max = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                if caption_button(ui, Caption::Maximise, is_max, btn_size)
                    .on_hover_text(if is_max {
                        crate::i18n::t("settings_hint.ctrl_restore")
                    } else {
                        crate::i18n::t("sql.maximise")
                    })
                    .clicked()
                {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!is_max));
                }
                if caption_button(ui, Caption::Minimise, false, btn_size)
                    .on_hover_text(crate::i18n::t("settings_hint.ctrl_minimise"))
                    .clicked()
                {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                }
            }
            let (icon, tip) = if ribbon_open {
                (crate::ui::icons::Icon::ArrowUp, "ribbon.hide")
            } else {
                (crate::ui::icons::Icon::ArrowDown, "ribbon.show")
            };
            if crate::ui::icons::icon_button(ui, true, icon, crate::i18n::t(tip)) {
                action.toggle_ribbon = true;
            }
        });
    });

    if ribbon_open {
        ribbon::draw(ui, cx, &mut action);
    }

    action
}

/// The search group of row 1: the instant-search field, one options menu that
/// holds everything that used to sit beside it (mode, case, whole word, column,
/// Ask, recent searches, bookmarks, filter or highlight), the match stepper and
/// the "searched only part of the table" note.
fn draw_search(
    ui: &mut Ui,
    cx: ToolbarCtx<'_>,
    search: SearchControls<'_>,
    width: f32,
    action: &mut ToolbarAction,
) {
    let ToolbarCtx {
        colors, has_data, ..
    } = cx;
    let SearchControls {
        text: search_text,
        mode: search_mode,
        case_sensitive: search_case_sensitive,
        whole_word: search_whole_word,
        scope_col: search_scope_col,
        column_names,
        ask,
        history: search_history,
        result_mode: search_result_mode,
        highlight_active: search_highlight_active,
        match_count: search_match_count,
        match_current: search_match_current,
        focus_requested: search_focus_requested,
        bookmarks,
        partial: search_partial,
        full_scan,
    } = search;

    // Field + clear button take about half the block, the rest is for the
    // options button, the stepper and the partial-search note.
    let field_w = (width * 0.55).clamp(150.0, 340.0).min(width);
    let response = instant_search::draw(
        ui,
        search_text,
        instant_search::Entry {
            asking: *ask.mode && ask.enabled,
            has_data,
            focus_requested: search_focus_requested,
            width: field_w,
        },
        action,
    );

    // One options menu instead of eight loose controls. The button is tinted
    // when anything differs from the plain default, so a narrowed search is
    // never invisible.
    let narrowed = *search_mode != SearchMode::Plain
        || *search_case_sensitive
        || *search_whole_word
        || search_scope_col.is_some()
        || (*ask.mode && ask.enabled);
    let mut options_button = egui::Button::image(crate::ui::icons::icon_image(
        ui,
        crate::ui::icons::Icon::Filter,
    ));
    if narrowed {
        options_button = options_button.fill(colors.accent.gamma_multiply(0.25));
    }
    let options = egui::containers::menu::MenuButton::from_button(options_button).ui(ui, |ui| {
        ui.set_min_width(190.0);
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
        ui.add_enabled_ui(has_data, |ui| {
            // Mode: how the text is read.
            ui.horizontal(|ui| {
                let old_mode = *search_mode;
                for m in [SearchMode::Plain, SearchMode::Wildcard, SearchMode::Regex] {
                    ui.selectable_value(search_mode, m, m.label_t());
                }
                if *search_mode != old_mode {
                    action.search_changed = true;
                }
            });
            // Case-sensitive (`Aa`) and whole-word toggles.
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(*search_case_sensitive, "Aa")
                    .on_hover_text(crate::i18n::t("search.case_sensitive"))
                    .clicked()
                {
                    *search_case_sensitive = !*search_case_sensitive;
                    action.search_changed = true;
                }
                if ui
                    .selectable_label(*search_whole_word, "W")
                    .on_hover_text(crate::i18n::t("search.whole_word"))
                    .clicked()
                {
                    *search_whole_word = !*search_whole_word;
                    action.search_changed = true;
                }
            });
            // Scope: whole table or one column.
            let scope_label = match *search_scope_col {
                None => crate::i18n::t("search.scope_all"),
                Some(c) => column_names
                    .get(c)
                    .cloned()
                    .unwrap_or_else(|| crate::i18n::t("search.scope_all")),
            };
            ui.menu_button(scope_label, |ui| {
                ui.set_max_height(320.0);
                egui::ScrollArea::vertical().show(ui, |ui| {
                    if ui
                        .selectable_label(
                            search_scope_col.is_none(),
                            crate::i18n::t("search.scope_all"),
                        )
                        .clicked()
                    {
                        *search_scope_col = None;
                        action.search_changed = true;
                        ui.close();
                    }
                    for (c, name) in column_names.iter().enumerate() {
                        if ui
                            .selectable_label(*search_scope_col == Some(c), name)
                            .clicked()
                        {
                            *search_scope_col = Some(c);
                            action.search_changed = true;
                            ui.close();
                        }
                    }
                });
            })
            .response
            .on_hover_text(crate::i18n::t("search.scope_hint"));

            // Filter / Highlight behaviour for the session; the table honours
            // it, text/tree views always highlight regardless.
            if ui
                .button(search_result_mode.label_t())
                .on_hover_text(crate::i18n::t("search.mode_toggle_hint"))
                .clicked()
            {
                *search_result_mode = match *search_result_mode {
                    crate::data::SearchResultMode::Filter => {
                        crate::data::SearchResultMode::Highlight
                    }
                    crate::data::SearchResultMode::Highlight => {
                        crate::data::SearchResultMode::Filter
                    }
                };
                action.search_result_mode_changed = true;
            }
        });
        ui.separator();

        // Ask: turn a plain-language sentence into filters via the chosen
        // assistant. Which profile answers is always visible, never implicit.
        let ask_resp = ui
            .add_enabled_ui(ask.enabled && has_data, |ui| {
                ui.selectable_label(*ask.mode, crate::i18n::t("search.ask"))
            })
            .inner;
        let ask_resp = if ask.enabled {
            let profile_name = ask
                .profiles
                .iter()
                .find(|(id, _)| id == ask.profile_id)
                .map(|(_, name)| name.clone())
                .unwrap_or_default();
            ask_resp.on_hover_text(
                crate::i18n::t("search.ask_hint").replace("{profile}", &profile_name),
            )
        } else {
            // A disabled control must say why, not repeat its label.
            ask_resp.on_disabled_hover_text(crate::i18n::t("search.ask_needs_profile"))
        };
        if ask_resp.clicked() {
            *ask.mode = !*ask.mode;
            // Switching modes changes what the box means, so it starts empty
            // either way, and any filter the old text was applying is dropped.
            // Focus jumps to the box so there is somewhere obvious to type.
            if !search_text.is_empty() {
                search_text.clear();
                action.search_changed = true;
            }
            if *ask.mode {
                response.request_focus();
            }
            ui.close();
        }
        if *ask.mode && ask.enabled {
            let selected = ask
                .profiles
                .iter()
                .find(|(id, _)| id == ask.profile_id)
                .map(|(_, name)| name.clone())
                .unwrap_or_default();
            ui.menu_button(selected, |ui| {
                for (id, name) in ask.profiles {
                    if ui
                        .selectable_value(ask.profile_id, id.clone(), name)
                        .clicked()
                    {
                        ui.close();
                    }
                }
            })
            .response
            .on_hover_text(crate::i18n::t("search.ask_profile_hint"));
        }
        ui.separator();

        // Recent queries: picking one fills the search box.
        ui.add_enabled_ui(has_data, |ui| {
            ui.menu_button(crate::i18n::t("search.history_btn"), |ui| {
                ui.set_min_width(160.0);
                ui.label(
                    RichText::new(crate::i18n::t("search.history"))
                        .size(10.0)
                        .color(colors.text_secondary),
                );
                if search_history.is_empty() {
                    ui.add_enabled(false, egui::Button::new(crate::i18n::t("greyed.none")));
                }
                for entry in search_history {
                    if ui.button(entry).clicked() {
                        *search_text = entry.clone();
                        action.search_changed = true;
                        ui.close();
                    }
                }
            })
            .response
            .on_hover_text(crate::i18n::t("search.history_hint"));

            // Bookmarks: jump to a named row/cell, delete one, or add one at
            // the current selection. Session-only per tab.
            ui.menu_button(crate::i18n::t("bookmarks.title"), |ui| {
                ui.set_min_width(180.0);
                if bookmarks.is_empty() {
                    ui.add_enabled(false, egui::Button::new(crate::i18n::t("greyed.none")));
                } else {
                    for (i, (name, row, col)) in bookmarks.iter().enumerate() {
                        ui.horizontal(|ui| {
                            let pos = match col {
                                Some(c) => format!("R{}:C{}", row + 1, c + 1),
                                None => format!("R{}", row + 1),
                            };
                            if ui.button(format!("{name}  ({pos})")).clicked() {
                                action.jump_bookmark = Some(i);
                                ui.close();
                            }
                            if ui
                                .small_button("x")
                                .on_hover_text(crate::i18n::t("bookmarks.delete"))
                                .clicked()
                            {
                                action.delete_bookmark = Some(i);
                                ui.close();
                            }
                        });
                    }
                }
                ui.separator();
                if ui
                    .button(crate::i18n::t("bookmarks.add"))
                    .on_hover_text(crate::i18n::t("bookmarks.add_hint"))
                    .clicked()
                {
                    action.add_bookmark = true;
                    ui.close();
                }
            })
            .response
            .on_hover_text(crate::i18n::t("bookmarks.title"));
        });
    });
    options
        .0
        .on_hover_text(crate::i18n::t("menu.search"));

    // With Ask on, Enter sends the sentence to the assistant rather than
    // stepping through matches.
    if *ask.mode && ask.enabled && !search_text.is_empty() {
        let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
        if enter && (response.lost_focus() || response.has_focus()) {
            action.ask_submitted = true;
            response.request_focus();
        }
    }
    // Enter / Shift+Enter while the search box is focused step through matches
    // (highlight mode only). Re-grab focus so repeated presses keep navigating
    // instead of dropping focus after the first Enter.
    if !*ask.mode && search_highlight_active && !search_text.is_empty() {
        let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
        if enter && (response.lost_focus() || response.has_focus()) {
            if ui.input(|i| i.modifiers.shift) {
                action.find_prev = true;
            } else {
                action.find_next = true;
            }
            response.request_focus();
        }
    }

    // Match count + previous/next, shown only when matches are highlighted in
    // place (so the user can step through them).
    if search_highlight_active && !search_text.is_empty() {
        let count_label = if search_match_count == 0 {
            crate::i18n::t("search.no_matches")
        } else {
            format!("{} / {}", search_match_current, search_match_count)
        };
        ui.label(RichText::new(count_label).color(colors.text_secondary));
        let has_matches = search_match_count > 0;
        if ui
            .add_enabled(has_matches, egui::Button::new("<"))
            .on_hover_text(crate::i18n::t("search.prev_match"))
            .clicked()
        {
            action.find_prev = true;
        }
        if ui
            .add_enabled(has_matches, egui::Button::new(">"))
            .on_hover_text(crate::i18n::t("search.next_match"))
            .clicked()
        {
            action.find_next = true;
        }
    }

    // Say what the search actually covered. A table holding a window of its
    // source searched that window and reported the hits as if they were all of
    // them. Truncated to the room left, with the full text on hover.
    if let Some((loaded, known_total)) = search_partial
        && !search_text.is_empty()
    {
        if let Some(kind) = full_scan {
            let usable = kind != FullScanKind::Unsupported;
            let hint = crate::i18n::t(kind.hint_key());
            if ui.available_width() > 120.0
                && ui
                    .add_enabled(usable, egui::Button::new(crate::i18n::t(kind.label_key())))
                    .on_hover_text(&hint)
                    .on_disabled_hover_text(&hint)
                    .clicked()
            {
                action.search_whole_source = true;
            }
        } else {
            ui.add(egui::Spinner::new().size(12.0));
        }
        let note = crate::ui::message::partial_note_text(loaded, known_total);
        if ui.available_width() > 40.0 {
            ui.add(
                egui::Label::new(
                    RichText::new(&note)
                        .small()
                        .color(crate::ui::message::PARTIAL_NOTE_COLOR),
                )
                .truncate(),
            )
            .on_hover_text(note);
        }
    }
}
