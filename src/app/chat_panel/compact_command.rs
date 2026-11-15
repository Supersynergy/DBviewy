use super::ChatPanelState;

impl ChatPanelState {
    /// The toolbar and expanded assistant share one editable draft.
    pub(crate) fn compact_command(
        &mut self,
        ui: &mut egui::Ui,
        source: Option<&str>,
    ) -> (bool, bool) {
        let running = self.session.lock().unwrap().is_running();
        let response = ui.add_enabled(
            !running,
            egui::TextEdit::singleline(&mut self.input)
                .id_salt("smartchat_command")
                .desired_width((ui.available_width() - 76.0).clamp(36.0, 190.0))
                .hint_text("Smartchat"),
        );
        let enter = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let send = ui
            .add_enabled(
                !running && !self.input.trim().is_empty(),
                egui::Button::image(octa::ui::icons::icon_image(ui, octa::ui::icons::Icon::Send)),
            )
            .on_hover_text(octa::i18n::t("chat.send"))
            .clicked();
        self.voice
            .show_with_label(ui, &mut self.input, &mut self.focus_input, source, "🎙");
        if self.focus_input && !self.visible {
            response.request_focus();
            self.focus_input = false;
        }
        (
            !running && !self.input.trim().is_empty() && (enter || send),
            !running && self.voice.completed_command,
        )
    }
}
