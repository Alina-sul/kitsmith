//! Project Overview tab: at-a-glance counts of what the active project
//! contains, with honest completeness notes.

use eframe::egui::{self, CornerRadius, Frame, Margin, RichText, Stroke};

use crate::gui::app::RytmdashApp;
use crate::gui::{theme, widgets};
use crate::model::SAMPLE_SLOT_COUNT;

pub fn show(app: &mut RytmdashApp, ui: &mut egui::Ui) {
    let Some(model) = app.model.as_ref() else {
        return;
    };
    let linked_kits = model.linked_kit_indices().len();
    let used_slots = model.slot_usage().len();
    let loaded = model.samples.len();
    let ram_bytes: u64 = model.samples.iter().map(|s| u64::from(s.size)).sum();

    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(theme::GAP_M, theme::GAP_M);
        stat_card(ui, "Kits in use", &linked_kits.to_string());
        stat_card(ui, "Patterns linked", &model.patterns.len().to_string());
        stat_card(
            ui,
            "Sample slots loaded",
            &format!("{loaded} / {SAMPLE_SLOT_COUNT}"),
        );
        stat_card(ui, "Slots referenced by kits", &used_slots.to_string());
        stat_card(
            ui,
            "Sample RAM (loaded slots)",
            &format!("{:.1} / 64 MB", ram_bytes as f64 / (1024.0 * 1024.0)),
        );
        stat_card(
            ui,
            "Sound Pool entries",
            &model.pool_sounds.len().to_string(),
        );
        stat_card(
            ui,
            "+Drive project slots",
            &model.projects.len().to_string(),
        );
    });

    ui.add_space(theme::GAP_L);
    widgets::inspector_group(ui, "DATA COMPLETENESS", |ui| {
        ui.label(format!(
            "Sample names resolved for {loaded} loaded slots; Sound Lock and Scene/Performance \
             data are not indexed yet."
        ));
        ui.label(
            RichText::new("Full details live in Device & Diagnostics.")
                .color(theme::TEXT_MUTED)
                .size(theme::FONT_META),
        );
    });
}

fn stat_card(ui: &mut egui::Ui, label: &str, value: &str) {
    Frame::new()
        .fill(theme::SURFACE)
        .stroke(Stroke::new(1.0, theme::BORDER))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(14))
        .show(ui, |ui| {
            ui.set_min_width(170.0);
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(value)
                        .size(theme::FONT_PAGE_TITLE)
                        .color(theme::TEXT_PRIMARY)
                        .strong(),
                );
                ui.label(
                    RichText::new(label)
                        .color(theme::TEXT_MUTED)
                        .size(theme::FONT_META),
                );
            });
        });
}
