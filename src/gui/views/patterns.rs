//! Patterns page (PRD §26): the 8-bank × 16-pattern map with kit links.
//! Read-only — it explains why kits and samples are "in use" without being a
//! sequencer editor.

use eframe::egui::{self, CornerRadius, Frame, Margin, RichText, ScrollArea, Sense, Stroke};

use crate::gui::app::{ProjectTab, RytmdashApp};
use crate::gui::{theme, widgets};
use crate::model::names;
use crate::model::{pattern_display_name, ProjectModel};

pub fn show(app: &mut RytmdashApp, ui: &mut egui::Ui) {
    let Some(model) = app.model.as_ref() else {
        return;
    };
    let selected = &mut app.ui_state.selected_pattern;
    let mut jump_kit = None;

    egui::Panel::right("pattern-inspector")
        .default_size(theme::INSPECTOR_WIDTH)
        .resizable(false)
        .frame(Frame::new().inner_margin(Margin::symmetric(10, 4)))
        .show(ui, |ui| {
            if let Some(index) = *selected {
                if let Some(kit) = pattern_inspector(ui, model, index, selected) {
                    jump_kit = Some(kit);
                }
            } else {
                widgets::section_label(ui, "PATTERN DETAILS");
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Select a pattern to see which kit it uses.")
                        .color(theme::TEXT_MUTED),
                );
            }
        });

    egui::CentralPanel::default()
        .frame(Frame::new().inner_margin(Margin::symmetric(0, 4)))
        .show(ui, |ui| {
            ui.label(
                RichText::new(format!("{} patterns link to a kit", model.patterns.len()))
                    .color(theme::TEXT_SECONDARY),
            );
            ui.add_space(theme::GAP_S);
            ScrollArea::vertical().show(ui, |ui| {
                pattern_map(ui, model, selected);
            });
        });

    if let Some(kit_index) = jump_kit {
        app.ui_state.tab = ProjectTab::Kits;
        app.ui_state.kits.selected_kit = Some(kit_index);
        app.ui_state.kits.selected_track = None;
    }
}

/// 8 banks (A–H) × 16 patterns; cells with a synced link show their kit.
fn pattern_map(ui: &mut egui::Ui, model: &ProjectModel, selected: &mut Option<usize>) {
    for bank in 0..8 {
        widgets::section_label(ui, &format!("BANK {}", (b'A' + bank as u8) as char));
        ui.add_space(2.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
            for pos in 0..16 {
                let index = bank * 16 + pos;
                pattern_cell(ui, model, index, selected);
            }
        });
        ui.add_space(theme::GAP_M);
    }
}

fn pattern_cell(
    ui: &mut egui::Ui,
    model: &ProjectModel,
    index: usize,
    selected: &mut Option<usize>,
) {
    let link = model.patterns.iter().find(|p| p.index == index);
    let is_selected = *selected == Some(index);
    let (fill, stroke) = if is_selected {
        (
            theme::SURFACE_ELEVATED,
            Stroke::new(1.5, theme::TEXT_SECONDARY),
        )
    } else if link.is_some() {
        (theme::SURFACE, Stroke::new(1.0, theme::BORDER))
    } else {
        (theme::BG_APP, Stroke::new(1.0, theme::BORDER))
    };
    let response = Frame::new()
        .fill(fill)
        .stroke(stroke)
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.set_min_width(64.0);
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(pattern_display_name(index))
                        .color(if link.is_some() {
                            theme::TEXT_PRIMARY
                        } else {
                            theme::TEXT_MUTED
                        })
                        .strong(),
                );
                match link {
                    Some(p) => {
                        ui.label(
                            RichText::new(format!("Kit {:02}", p.kit_index + 1))
                                .color(theme::TEXT_SECONDARY)
                                .size(theme::FONT_META),
                        );
                    }
                    None => {
                        ui.label(
                            RichText::new("—")
                                .color(theme::TEXT_MUTED)
                                .size(theme::FONT_META),
                        );
                    }
                }
            });
        })
        .response
        .interact(Sense::click());
    let response = if link.is_some() {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    };
    if response.clicked() && link.is_some() {
        *selected = (!is_selected).then_some(index);
    }
}

/// Inspector for one pattern. Returns the kit index when "Show in Kits" is
/// clicked.
fn pattern_inspector(
    ui: &mut egui::Ui,
    model: &ProjectModel,
    index: usize,
    selected: &mut Option<usize>,
) -> Option<usize> {
    let mut jump = None;
    ui.horizontal(|ui| {
        widgets::section_label(ui, &format!("PATTERN {}", pattern_display_name(index)));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("×").on_hover_text("Close").clicked() {
                *selected = None;
            }
        });
    });
    ui.add_space(theme::GAP_S);

    widgets::inspector_group(ui, "LINKED KIT", |ui| {
        match model
            .patterns
            .iter()
            .find(|p| p.index == index)
            .and_then(|p| model.kits.get(p.kit_index))
        {
            Some(kit) => {
                widgets::detail_line(ui, "Kit", &format!("{:02}", kit.index + 1));
                widgets::detail_line(
                    ui,
                    "Name",
                    names::kit_display_name(&kit.name, kit.index).text(),
                );
                if ui.link("Show in Kits").clicked() {
                    jump = Some(kit.index);
                }
            }
            None => {
                ui.label(RichText::new("Kit data unavailable").color(theme::TEXT_MUTED));
            }
        }
    });

    widgets::inspector_group(ui, "DEPENDENCIES", |ui| {
        let Some(pattern) = model.patterns.iter().find(|p| p.index == index) else {
            ui.label(RichText::new("Pattern data unavailable").color(theme::TEXT_MUTED));
            return;
        };
        if !model.dependency_scan {
            ui.label(
                RichText::new("Old cache — re-sync to index Sound Locks and sample plocks.")
                    .color(theme::TEXT_MUTED)
                    .size(theme::FONT_META),
            );
            return;
        }
        if pattern.sound_locks.is_empty() && pattern.plocked_sample_slots.is_empty() {
            ui.label(RichText::new("No Sound Locks or sample plocks.").color(theme::TEXT_MUTED));
        }
        for pool_index in &pattern.sound_locks {
            let name = model
                .pool_sounds
                .get(usize::from(*pool_index))
                .map(|s| s.name.trim())
                .filter(|n| !n.is_empty())
                .map_or_else(
                    || format!("Pool Sound {:03}", pool_index + 1),
                    str::to_owned,
                );
            ui.label(format!(
                "Sound-locks pool sound {:03} {name}",
                pool_index + 1
            ));
        }
        for slot in &pattern.plocked_sample_slots {
            let name = match crate::model::names::sample_display_name(model.sample_name(*slot)) {
                crate::model::names::ResolvedName::Native(name) => name,
                _ => "name unavailable".to_owned(),
            };
            ui.label(format!("Sample plock on slot {slot:02} · {name}"));
        }
    });
    jump
}
