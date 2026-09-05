//! Small shared UI components: pills, badges, drag handles, section labels.
//!
//! Everything here renders from theme tokens so states read the same across
//! the app, and every colored state also carries a textual cue (PRD §43).

use eframe::egui::{
    self, Color32, CornerRadius, Frame, Margin, Response, RichText, Sense, Stroke, Ui,
};

use crate::gui::theme;

/// A colored status dot followed by a label.
pub fn status_dot(ui: &mut Ui, color: Color32, label: &str) {
    ui.label(RichText::new("●").color(color).size(10.0));
    ui.label(label);
}

/// A small uppercase badge, e.g. `ACTIVE`, `CACHED`, `PARTIAL`.
pub fn badge(ui: &mut Ui, text: &str, color: Color32) {
    Frame::new()
        .fill(color.gamma_multiply(0.18))
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::symmetric(6, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(text).color(color).size(theme::FONT_META));
        });
}

/// The purple sample pill: icon + label. Returns its response (clickable).
pub fn sample_pill(ui: &mut Ui, label: &str, selected: bool) -> Response {
    let fill = if selected {
        theme::SAMPLE_PURPLE.gamma_multiply(0.45)
    } else {
        theme::SAMPLE_PURPLE.gamma_multiply(0.16)
    };
    let response = Frame::new()
        .fill(fill)
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::symmetric(6, 2))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.label(RichText::new("~").color(theme::SAMPLE_PURPLE).strong());
                ui.label(
                    RichText::new(label)
                        .color(if selected {
                            theme::TEXT_PRIMARY
                        } else {
                            theme::SAMPLE_PURPLE
                        })
                        .size(theme::FONT_META),
                );
            });
        })
        .response;
    response.interact(Sense::click())
}

/// The six-dot drag affordance (PRD §14.4). Purely visual; pair it with a
/// `dnd_drag_source` wrapping the row.
pub fn drag_handle(ui: &mut Ui) {
    ui.label(
        RichText::new("⠿")
            .color(theme::TEXT_MUTED)
            .size(theme::FONT_BODY),
    );
}

/// An uppercase section label, e.g. `PROJECTS`.
pub fn section_label(ui: &mut Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .color(theme::TEXT_MUTED)
            .size(theme::FONT_META)
            .strong(),
    );
}

/// A full-width selectable list row. Returns true when clicked.
pub fn list_row(
    ui: &mut Ui,
    selected: bool,
    min_height: f32,
    content: impl FnOnce(&mut Ui),
) -> bool {
    let fill = if selected {
        theme::SURFACE_ELEVATED
    } else {
        Color32::TRANSPARENT
    };
    let stroke = if selected {
        Stroke::new(1.0, theme::BORDER)
    } else {
        Stroke::NONE
    };
    let response = Frame::new()
        .fill(fill)
        .stroke(stroke)
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.set_min_height(min_height - 12.0);
            ui.set_width(ui.available_width());
            content(ui);
        })
        .response
        .interact(Sense::click());
    if response.hovered() {
        ui.painter().rect_stroke(
            response.rect,
            CornerRadius::same(6),
            Stroke::new(1.0, theme::BORDER),
            egui::StrokeKind::Inside,
        );
    }
    response.clicked()
}

/// Header line for an inspector group.
pub fn inspector_group(ui: &mut Ui, title: &str, content: impl FnOnce(&mut Ui)) {
    section_label(ui, title);
    ui.add_space(4.0);
    Frame::new()
        .fill(theme::SURFACE)
        .stroke(Stroke::new(1.0, theme::BORDER))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            content(ui);
        });
    ui.add_space(theme::GAP_M);
}

/// A `label: value` detail line inside an inspector group.
pub fn detail_line(ui: &mut Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(label)
                .color(theme::TEXT_MUTED)
                .size(theme::FONT_META),
        );
        ui.label(RichText::new(value).color(theme::TEXT_PRIMARY));
    });
}
