//! Project Samples workspace (PRD §21–§22): all 127 project sample slots as
//! a table, with a usage inspector and honest cleanup wording. Read-only —
//! destructive cleanup stays disabled until the full dependency graph exists.

use eframe::egui::{self, Frame, Id, Margin, RichText, ScrollArea};

use crate::gui::app::{ProjectTab, RytmdashApp};
use crate::gui::{theme, widgets};
use crate::model::names::{self, ResolvedName};
use crate::model::{ProjectModel, SlotUse, SAMPLE_SLOT_COUNT};

pub fn show(app: &mut RytmdashApp, ui: &mut egui::Ui) {
    let Some(model) = app.model.as_ref() else {
        return;
    };
    let usage = model.slot_usage();
    let selected = &mut app.ui_state.selected_sample_slot;
    let mut jump: Option<(usize, u8)> = None;

    egui::Panel::right("sample-inspector")
        .default_size(theme::INSPECTOR_WIDTH)
        .resizable(false)
        .frame(Frame::new().inner_margin(Margin::symmetric(10, 4)))
        .show(ui, |ui| {
            if let Some(slot) = *selected {
                if let Some(target) = slot_inspector(ui, model, slot, selected) {
                    jump = Some(target);
                }
            } else {
                widgets::section_label(ui, "SLOT DETAILS");
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Select a slot to see its file and every known use.")
                        .color(theme::TEXT_MUTED),
                );
            }
        });

    egui::CentralPanel::default()
        .frame(Frame::new().inner_margin(Margin::symmetric(0, 4)))
        .show(ui, |ui| {
            let loaded = model.samples.len();
            ui.label(
                RichText::new(format!(
                    "{loaded} of {SAMPLE_SLOT_COUNT} project sample slots loaded · {} referenced by kits",
                    usage.len()
                ))
                .color(theme::TEXT_SECONDARY),
            );
            ui.add_space(theme::GAP_S);
            cleanup_notice(ui);
            ui.add_space(theme::GAP_S);

            ScrollArea::vertical().show(ui, |ui| {
                for slot in 1..=SAMPLE_SLOT_COUNT {
                    slot_row(ui, model, slot, usage.get(&slot).map_or(0, Vec::len), selected);
                }
            });
        });

    if let Some((kit_index, slot)) = jump {
        app.ui_state.tab = ProjectTab::Kits;
        app.ui_state.kits.selected_kit = Some(kit_index);
        app.ui_state.kits.selected_track = None;
        app.ui_state.kits.traced_slot = Some(slot);
    }
}

/// Safe-vs-incomplete cleanup wording (PRD §22): no destructive `unused`
/// action while Sound Lock dependencies are not indexed.
fn cleanup_notice(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.add_enabled(false, egui::Button::new("Unload unused…"))
            .on_disabled_hover_text(
                "Cleanup safety incomplete — Sound Lock dependencies are not indexed yet, so \
                 “no kit-track uses” does not prove a sample is safe to unload.",
            );
        ui.label(
            RichText::new(
                "Cleanup safety incomplete — Sound Lock dependencies are not indexed yet.",
            )
            .color(theme::TEXT_MUTED)
            .size(theme::FONT_META),
        );
    });
}

fn slot_row(
    ui: &mut egui::Ui,
    model: &ProjectModel,
    slot: u8,
    uses: usize,
    selected: &mut Option<u8>,
) {
    let info = model.samples.iter().find(|s| s.slot == slot);
    let referenced = uses > 0;
    // An unloaded, unreferenced slot renders as one quiet `Empty` line.
    if info.is_none() && !referenced {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("   {slot:03}"))
                    .color(theme::TEXT_MUTED)
                    .monospace(),
            );
            ui.label(
                RichText::new("Empty")
                    .color(theme::TEXT_MUTED)
                    .size(theme::FONT_META),
            );
        });
        return;
    }

    let is_selected = *selected == Some(slot);
    let clicked = widgets::list_row(ui, is_selected, theme::LIST_ROW_HEIGHT, |ui| {
        ui.horizontal(|ui| {
            widgets::drag_handle(ui);
            ui.label(
                RichText::new(format!("{slot:03}"))
                    .color(theme::TEXT_MUTED)
                    .monospace(),
            );
            let name = match names::sample_display_name(info.map(|i| i.name.as_str())) {
                ResolvedName::Native(name) => {
                    RichText::new(name).color(theme::TEXT_PRIMARY).strong()
                }
                _ => RichText::new("Name unavailable").color(theme::TEXT_MUTED),
            };
            let id = Id::new(("slot-table-row", slot));
            let _ = ui.dnd_drag_source(id, super::SlotDrag { slot }, |ui| {
                ui.label(name);
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if referenced {
                    widgets::badge(ui, "IN USE", theme::SAMPLE_PURPLE);
                    ui.label(
                        RichText::new(format!("{uses} use{}", if uses == 1 { "" } else { "s" }))
                            .color(theme::TEXT_SECONDARY)
                            .size(theme::FONT_META),
                    );
                } else {
                    widgets::badge(ui, "LOADED", theme::TEXT_MUTED);
                    ui.label(
                        RichText::new("no kit-track uses")
                            .color(theme::TEXT_MUTED)
                            .size(theme::FONT_META),
                    );
                }
                if let Some(info) = info {
                    ui.label(
                        RichText::new(format!("{} KB", info.size / 1024))
                            .color(theme::TEXT_MUTED)
                            .size(theme::FONT_META),
                    );
                }
            });
        });
    });
    if clicked {
        *selected = (!is_selected).then_some(slot);
    }
}

/// Right inspector for one slot: file identity + all known references.
/// Returns a `(kit_index, slot)` jump target when a use is clicked.
fn slot_inspector(
    ui: &mut egui::Ui,
    model: &ProjectModel,
    slot: u8,
    selected: &mut Option<u8>,
) -> Option<(usize, u8)> {
    let mut jump = None;
    ui.horizontal(|ui| {
        widgets::section_label(ui, &format!("SLOT {slot:03}"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("×").on_hover_text("Close").clicked() {
                *selected = None;
            }
        });
    });
    ui.add_space(theme::GAP_S);

    widgets::inspector_group(ui, "FILE", |ui| {
        match model.samples.iter().find(|s| s.slot == slot) {
            Some(info) => {
                widgets::detail_line(ui, "Name", &info.name);
                widgets::detail_line(ui, "Size", &format!("{} KB", info.size / 1024));
                ui.label(
                    RichText::new(&info.path)
                        .color(theme::TEXT_MUTED)
                        .size(theme::FONT_META),
                )
                .on_hover_text("+Drive path");
            }
            None => {
                ui.label(RichText::new("Name unavailable").color(theme::TEXT_MUTED));
                ui.label(
                    RichText::new("The file-protocol lookup did not resolve this slot.")
                        .color(theme::TEXT_MUTED)
                        .size(theme::FONT_META),
                );
            }
        }
    });

    let usage = model.slot_usage();
    let uses: &[SlotUse] = usage.get(&slot).map_or(&[], Vec::as_slice);
    widgets::inspector_group(ui, "USED BY", |ui| {
        if uses.is_empty() {
            ui.label(RichText::new("No kit-track uses").color(theme::TEXT_MUTED));
        }
        for u in uses {
            let label = format!(
                "Kit {:02} {} · {}",
                u.kit_index + 1,
                names::kit_display_name(&u.kit_name, u.kit_index).text(),
                names::track_label(u.track),
            );
            if ui.link(label).on_hover_text("Show in Kits").clicked() {
                jump = Some((u.kit_index, slot));
            }
        }
        ui.add_space(4.0);
        ui.label(
            RichText::new("Sound Lock uses are not indexed yet — this list may be incomplete.")
                .color(theme::TEXT_MUTED)
                .size(theme::FONT_META),
        );
    });
    jump
}
