//! The three table zoom controls keep their scope and accessible names in
//! sync without taking more room in the status bar or the ribbon.

use egui::{Button, Color32, RichText, Ui};

use super::control_row::control_height;
use super::icons::{Icon, action_button, icon_image};
use super::menu_entry::disabled_tip;
use super::toolbar::ToolbarAction;
use crate::i18n::t;

enum ZoomControl {
    In,
    Out,
    Reset,
}

impl ZoomControl {
    fn name(&self, visible_percent: Option<u32>) -> String {
        let scope = t("view_menu.table");
        let action = match self {
            Self::In => t("command.zoom_in"),
            Self::Out => t("command.zoom_out"),
            Self::Reset if visible_percent.is_some() => t("view_menu.zoom_reset_hint"),
            Self::Reset => t("view_menu.zoom_reset"),
        };
        if let Some(percent) = visible_percent {
            format!("{scope}: {percent}%, {action}")
        } else {
            format!("{scope}: {action}")
        }
    }

    fn describe(
        &self,
        response: egui::Response,
        visible_percent: Option<u32>,
        pointer_hint: Option<String>,
    ) -> egui::Response {
        let name = self.name(visible_percent);
        let enabled = response.enabled();
        response
            .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, &name));
        let hint = pointer_hint.unwrap_or(name);
        if enabled {
            response.on_hover_text(hint)
        } else {
            response.on_disabled_hover_text(hint)
        }
    }

    fn apply(&self, action: &mut ToolbarAction) {
        match self {
            Self::In => action.zoom_in = true,
            Self::Out => action.zoom_out = true,
            Self::Reset => action.zoom_reset = true,
        }
    }
}

/// Draw right to left in the status bar. The visible percentage is also
/// the reset button's name, so it is discoverable without a pointer.
pub(super) fn status_controls(ui: &mut Ui, percent: u32, step: u32, muted: Color32) -> Option<u32> {
    let scope = t("view_menu.table");
    let step_text = step.to_string();
    let choices = [
        (
            ZoomControl::In,
            "+".into(),
            None,
            percent.saturating_add(step),
            format!(
                "{scope}: {}",
                t("status_bar.zoom_in_hint").replace("{n}", &step_text)
            ),
        ),
        (
            ZoomControl::Reset,
            format!("{percent}%"),
            Some(percent),
            100,
            t("status_bar.zoom_hint"),
        ),
        (
            ZoomControl::Out,
            "-".into(),
            None,
            percent.saturating_sub(step),
            format!(
                "{scope}: {}",
                t("status_bar.zoom_out_hint").replace("{n}", &step_text)
            ),
        ),
    ];
    let mut target = None;
    for (control, text, visible_percent, value, hint) in choices {
        let button = Button::new(RichText::new(text).size(11.0).color(muted)).frame(false);
        if control
            .describe(ui.add(button), visible_percent, Some(hint))
            .clicked()
        {
            target = Some(value);
        }
    }
    target
}

pub(super) fn ribbon_controls(ui: &mut Ui, percent: u32, action: &mut ToolbarAction) {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let size = egui::Vec2::splat(control_height(ui));
        let zoom_in = Button::image(icon_image(ui, Icon::ZoomIn)).min_size(size);
        if ZoomControl::In
            .describe(ui.add(zoom_in), None, None)
            .clicked()
        {
            ZoomControl::In.apply(action);
        }
        if ZoomControl::Reset
            .describe(ui.small_button(format!("{percent}%")), Some(percent), None)
            .clicked()
        {
            ZoomControl::Reset.apply(action);
        }
        let zoom_out = Button::image(icon_image(ui, Icon::ZoomOut)).min_size(size);
        if ZoomControl::Out
            .describe(ui.add(zoom_out), None, None)
            .clicked()
        {
            ZoomControl::Out.apply(action);
        }
    });
}

pub(super) fn view_controls(ui: &mut Ui, percent: u32, muted: Color32, action: &mut ToolbarAction) {
    ui.label(
        RichText::new(format!("{}: {}", t("view_menu.table"), t("view_menu.zoom")))
            .strong()
            .size(11.0)
            .color(muted),
    );
    super::control_row::control_line(ui, |ui| {
        if ZoomControl::Out
            .describe(ui.button("-"), None, None)
            .clicked()
        {
            ZoomControl::Out.apply(action);
        }
        ui.label(format!("{percent}%"));
        if ZoomControl::In
            .describe(ui.button("+"), None, None)
            .clicked()
        {
            ZoomControl::In.apply(action);
        }
    });
    let name = format!(
        "{}: {}",
        t("view_menu.table"),
        t("view_menu.zoom_reset_hint")
    );
    let enabled = percent != 100;
    let hint = if enabled {
        name
    } else {
        disabled_tip(&name, &t("greyed.zoom_is_100"))
    };
    let button = ui.add_enabled(enabled, action_button("view_menu.zoom_reset"));
    if ZoomControl::Reset
        .describe(button, None, Some(hint))
        .clicked()
    {
        ZoomControl::Reset.apply(action);
        ui.close();
    }
}

#[cfg(test)]
#[path = "table_zoom_tests.rs"]
mod tests;
