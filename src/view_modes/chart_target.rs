//! One transient target series for a linear line chart.
use eframe::egui;
use octa::data::chart::{ChartConfig, ChartData, ChartKind, ChartPrep, ChartSeries, SeriesStyle};
use octa::i18n::t;

use crate::app::state::TabState;
use crate::ui::theme::ThemeColors;

// Amber is reserved for the reference here, not a colour from the
// automatically cycled data series. The export uses the same RGB.
const AMBER: [u8; 4] = [174, 107, 25, 255];

pub(super) fn color() -> egui::Color32 {
    egui::Color32::from_rgb(AMBER[0], AMBER[1], AMBER[2])
}

pub(super) fn style() -> SeriesStyle {
    SeriesStyle {
        color: Some(AMBER),
        ..SeriesStyle::default()
    }
}

fn supports(cfg: &ChartConfig) -> bool {
    cfg.kind == ChartKind::Line && !cfg.y_log_scale
}

fn number(text: &str) -> Option<f64> {
    let text = text.trim();
    let value = text.parse::<f64>().ok().or_else(|| {
        // An entered decimal comma, not a thousands separator.
        (!text.contains('.') && text.contains(','))
            .then(|| text.replace(',', "."))
            .and_then(|value| value.parse().ok())
    })?;
    value.is_finite().then_some(value)
}

pub(super) fn draw_controls(ui: &mut egui::Ui, tab: &mut TabState, colors: &ThemeColors) {
    let supported = supports(&tab.chart_config);
    let hint = t("chart.target_line_hint");
    octa::ui::control_row::control_row(ui, |ui| {
        let label = ui.label(t("chart.target_line")).on_hover_text(&hint);
        ui.add_enabled_ui(supported, |ui| {
            octa::ui::control_row::control_text_edit(
                ui,
                112.0,
                egui::TextEdit::singleline(&mut tab.chart_buffers.target_line)
                    .id_salt("chart_target_value")
                    .hint_text(t("chart.placeholder_none"))
                    .char_limit(40),
            )
            .labelled_by(label.id)
            .on_hover_text(&hint)
            .on_disabled_hover_text(&hint);
        });
        if supported
            && !tab.chart_buffers.target_line.trim().is_empty()
            && number(&tab.chart_buffers.target_line).is_none()
        {
            ui.colored_label(colors.warning, t("chart.target_line_invalid"));
        }
    });
}

/// Append only after overlay analyses are done. No source table, forecast
/// input, server cache key or serialized chart configuration is changed.
pub(super) fn append(prep: &mut ChartPrep, cfg: &ChartConfig, input: &str) -> Option<usize> {
    if !supports(cfg) {
        return None;
    }
    let target = number(input)?;
    if let (Some(min), Some(max)) = (cfg.y_min, cfg.y_max)
        && min.is_finite()
        && max.is_finite()
        && min < max
        && !(min..=max).contains(&target)
    {
        // Respect explicit bounds in both the view and export.
        return None;
    }
    let ChartData::Lines { categories, series } = &mut prep.data else {
        return None;
    };
    if !series.iter().any(|s| !s.points.is_empty()) {
        return None;
    }
    let (mut low, mut high) = if let Some(categories) = categories {
        (0.0, categories.len().checked_sub(1)? as f64)
    } else {
        // Numeric/date line series are sorted at preparation. Categorical
        // series can revisit a category, so their range comes from labels.
        let mut low = f64::INFINITY;
        let mut high = f64::NEG_INFINITY;
        for s in series.iter() {
            if let (Some(first), Some(last)) = (s.points.first(), s.points.last()) {
                low = low.min(first[0]);
                high = high.max(last[0]);
            }
        }
        (low, high)
    };
    if !low.is_finite() || !high.is_finite() || low > high {
        return None;
    }
    if low == high {
        let padding = (low.abs() * f64::EPSILON * 4.0).max(1.0);
        low = (low - padding).max(-f64::MAX);
        high = (high + padding).min(f64::MAX);
    }
    let slot = series.len();
    series.push(ChartSeries {
        name: format!("{}: {}", t("chart.target_line"), input.trim()),
        points: vec![[low, target], [high, target]],
    });
    Some(slot)
}

#[cfg(test)]
#[path = "chart_target_tests.rs"]
mod tests;
