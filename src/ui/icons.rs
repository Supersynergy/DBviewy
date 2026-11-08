//! Hand-drawn toolbar icons.
//!
//! Octa ships four fonts and none of them promises symbol coverage, so
//! glyphs like "⛶" or "🖼" render as tofu. These icons are painted as egui
//! strokes instead: they cannot be missing, they scale with the widget,
//! and they inherit the theme's foreground colour.

use eframe::egui::{self, Color32, CornerRadius, Painter, Pos2, Rect, Stroke, Vec2};

/// The pictograms [`paint_icon`] and [`icon_button`] understand.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icon {
    /// Chevron pointing left.
    Prev,
    /// Chevron pointing right.
    Next,
    /// Bar plus chevron: jump to the first item.
    First,
    /// Chevron plus bar: jump to the last item.
    Last,
    /// Magnifier with a minus.
    ZoomOut,
    /// Magnifier with a plus.
    ZoomIn,
    /// Four corner brackets: fit content to the window.
    Fit,
    /// Box with an arrow leaving top-right: external link.
    OpenUrl,
    /// Two overlapping sheets: copy.
    Copy,
    /// Window with a side column: toggle the details panel.
    Details,
    /// Cross: close.
    Close,
}

/// Draw `icon` centred inside `rect`, stroked in `color`. The stroke width
/// follows the icon size so a badge and a toolbar glyph read the same.
pub fn paint_icon(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let r = rect.shrink(rect.width().min(rect.height()) * 0.08);
    let w = (r.width() / 9.0).clamp(0.8, 2.4);
    let s = Stroke::new(w, color);
    let (l, t, rr, b) = (r.left(), r.top(), r.right(), r.bottom());
    let (cx, cy) = (r.center().x, r.center().y);
    let line = |painter: &Painter, a: Pos2, bb: Pos2| painter.line_segment([a, bb], s);
    match icon {
        Icon::Prev => {
            line(painter, Pos2::new(rr * 0.7 + cx * 0.3, t), Pos2::new(l, cy));
            line(painter, Pos2::new(l, cy), Pos2::new(rr * 0.7 + cx * 0.3, b));
        }
        Icon::Next => {
            line(painter, Pos2::new(l * 0.7 + cx * 0.3, t), Pos2::new(rr, cy));
            line(painter, Pos2::new(rr, cy), Pos2::new(l * 0.7 + cx * 0.3, b));
        }
        Icon::First => {
            line(
                painter,
                Pos2::new(l + w * 0.5, t),
                Pos2::new(l + w * 0.5, b),
            );
            line(
                painter,
                Pos2::new(rr * 0.65 + cx * 0.35, t),
                Pos2::new(l + w, cy),
            );
            line(
                painter,
                Pos2::new(l + w, cy),
                Pos2::new(rr * 0.65 + cx * 0.35, b),
            );
        }
        Icon::Last => {
            line(
                painter,
                Pos2::new(rr - w * 0.5, t),
                Pos2::new(rr - w * 0.5, b),
            );
            line(
                painter,
                Pos2::new(l * 0.65 + cx * 0.35, t),
                Pos2::new(rr - w, cy),
            );
            line(
                painter,
                Pos2::new(rr - w, cy),
                Pos2::new(l * 0.65 + cx * 0.35, b),
            );
        }
        Icon::ZoomIn | Icon::ZoomOut => {
            let ring = Rect::from_min_size(
                Pos2::new(l, t),
                egui::vec2(r.width() * 0.62, r.height() * 0.62),
            );
            painter.circle_stroke(ring.center(), ring.width() / 2.0, s);
            let handle_from = ring.right_bottom() - Vec2::splat(ring.width() * 0.08);
            line(
                painter,
                handle_from,
                Pos2::new(
                    handle_from.x + r.width() * 0.24,
                    handle_from.y + r.height() * 0.24,
                ),
            );
            let m = ring.center();
            let arm = ring.width() * 0.26;
            line(
                painter,
                Pos2::new(m.x - arm, m.y),
                Pos2::new(m.x + arm, m.y),
            );
            if icon == Icon::ZoomIn {
                line(
                    painter,
                    Pos2::new(m.x, m.y - arm),
                    Pos2::new(m.x, m.y + arm),
                );
            }
        }
        Icon::Fit => {
            let arm = r.width() * 0.28;
            for (sx, sy) in [(l, t), (rr, t), (l, b), (rr, b)] {
                let dx = if sx == l { arm } else { -arm };
                let dy = if sy == t { arm } else { -arm };
                line(painter, Pos2::new(sx + dx, sy), Pos2::new(sx, sy));
                line(painter, Pos2::new(sx, sy + dy), Pos2::new(sx, sy));
            }
        }
        Icon::OpenUrl => {
            let boxr = Rect::from_min_max(
                Pos2::new(l, t + r.height() * 0.3),
                Pos2::new(l + r.width() * 0.7, b),
            );
            painter.rect_stroke(boxr, CornerRadius::same(2), s, egui::StrokeKind::Inside);
            line(
                painter,
                Pos2::new(cx - r.width() * 0.1, cy + r.height() * 0.1),
                Pos2::new(rr, t),
            );
            line(
                painter,
                Pos2::new(rr - r.width() * 0.3, t),
                Pos2::new(rr, t),
            );
            line(
                painter,
                Pos2::new(rr, t + r.height() * 0.3),
                Pos2::new(rr, t),
            );
        }
        Icon::Copy => {
            let back = Rect::from_min_max(
                Pos2::new(l + r.width() * 0.22, t),
                Pos2::new(rr, b - r.height() * 0.22),
            );
            let front = Rect::from_min_max(
                Pos2::new(l, t + r.height() * 0.22),
                Pos2::new(rr - r.width() * 0.22, b),
            );
            painter.rect_stroke(back, CornerRadius::same(1), s, egui::StrokeKind::Inside);
            let fill = painter
                .ctx()
                .style_of(painter.ctx().theme())
                .visuals
                .panel_fill;
            painter.rect(
                front,
                CornerRadius::same(1),
                fill,
                s,
                egui::StrokeKind::Inside,
            );
        }
        Icon::Details => {
            painter.rect_stroke(r, CornerRadius::same(2), s, egui::StrokeKind::Inside);
            let x = l + r.width() * 0.62;
            line(painter, Pos2::new(x, t), Pos2::new(x, b));
        }
        Icon::Close => {
            line(painter, Pos2::new(l, t), Pos2::new(rr, b));
            line(painter, Pos2::new(rr, t), Pos2::new(l, b));
        }
    }
}

/// A square toolbar button showing only an icon; `enabled` greys it like
/// a normal button. The hover text carries the label plus the key hint.
pub fn icon_button(ui: &mut egui::Ui, enabled: bool, icon: Icon, tip: impl Into<String>) -> bool {
    let h = crate::ui::control_row::control_height(ui);
    let (rect, mut response) = ui.allocate_exact_size(
        egui::vec2(h, h),
        if enabled {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        },
    );
    response = response.on_hover_text(tip.into());
    let visuals = if enabled {
        ui.style().interact(&response)
    } else {
        &ui.visuals().widgets.inactive
    };
    if enabled {
        ui.painter().rect(
            rect,
            visuals.corner_radius,
            visuals.weak_bg_fill,
            visuals.bg_stroke,
            egui::StrokeKind::Inside,
        );
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
    }
    let fg = if enabled {
        visuals.fg_stroke.color
    } else {
        ui.visuals().weak_text_color()
    };
    paint_icon(ui.painter(), rect.shrink(h * 0.18), icon, fg);
    response.clicked()
}
