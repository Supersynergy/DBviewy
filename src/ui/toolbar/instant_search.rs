use egui::{Response, Ui};

use super::ToolbarAction;

/// The always-visible entry shares the active tab's existing search state.
pub(super) struct Entry {
    pub asking: bool,
    pub has_data: bool,
    pub focus_requested: bool,
    pub width: f32,
}

pub(super) fn draw(
    ui: &mut Ui,
    text: &mut String,
    entry: Entry,
    action: &mut ToolbarAction,
) -> Response {
    let Entry {
        asking,
        has_data,
        focus_requested,
        width,
    } = entry;
    let hint = crate::i18n::t(if asking {
        "search.ask_placeholder"
    } else {
        "instant_search.placeholder"
    });
    let search_id = ui.id().with("dbviewy_instant_search");
    // Consume Escape before TextEdit handles it by surrendering focus.
    let escape = ui.memory(|m| m.has_focus(search_id))
        && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    let response = ui
        .add_enabled(
            has_data,
            egui::TextEdit::singleline(text)
                .id(search_id)
                .event_filter(egui::EventFilter {
                    escape: true,
                    horizontal_arrows: true,
                    vertical_arrows: true,
                    ..Default::default()
                })
                .desired_width((width - 32.0).max(40.0))
                .hint_text(hint),
        )
        .on_hover_text(crate::i18n::t("instant_search.hint"))
        .on_disabled_hover_text(crate::i18n::t("instant_search.empty_hint"));
    if response.changed() && !asking {
        action.search_changed = true;
    }
    if response.lost_focus() && !text.is_empty() {
        action.commit_search_history = true;
    }
    if focus_requested && has_data {
        response.request_focus();
    }
    let clear = ui
        .add_enabled(has_data && !text.is_empty(), egui::Button::new("x"))
        .on_hover_text(crate::i18n::t("instant_search.clear_hint"))
        .on_disabled_hover_text(crate::i18n::t("instant_search.clear_hint"))
        .clicked();
    if (escape || clear) && !text.is_empty() {
        text.clear();
        action.search_changed = true;
        response.request_focus();
    }
    response
}
