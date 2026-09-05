//! Dashboard view of a synced project model.
//!
//! Sample slots are drag sources; kit track rows are drop zones. A completed
//! drag returns a [`PendingAssign`] for the app to preview and execute — the
//! dashboard itself never mutates anything.

use eframe::egui::{self, Color32, Frame, Id, RichText};

use crate::gui::execute::PendingAssign;
use crate::model::{pattern_display_name, KitModel, ProjectModel};

/// Payload carried by a slot drag.
struct SlotDrag {
    slot: u8,
}

const SLOT_COLOR: Color32 = Color32::from_rgb(0x9B, 0x59, 0xB6);

/// Renders the model. Returns a pending assignment when a slot was dropped
/// on a kit track.
pub fn show(ui: &mut egui::Ui, model: &ProjectModel) -> Option<PendingAssign> {
    let usage = model.slot_usage();
    let linked_kits = model.linked_kit_indices();
    let mut dropped = None;

    ui.label(format!(
        "{} sample slots referenced · {} kits linked by patterns · {} pool sounds",
        usage.len(),
        linked_kits.len(),
        model.pool_sounds.len(),
    ));
    ui.label(
        RichText::new("Drag a slot chip onto a kit track to re-point that track's sample.").weak(),
    );
    ui.add_space(8.0);

    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.collapsing(
            RichText::new(format!("Sample slots in use ({})", usage.len())).strong(),
            |ui| {
                if usage.is_empty() {
                    ui.label(RichText::new("No kit references any sample slot.").weak());
                }
                for (slot, uses) in &usage {
                    ui.horizontal(|ui| {
                        slot_chip(ui, *slot);
                        ui.label(format!("{} use(s)", uses.len()));
                    });
                    ui.indent(("slot-uses", slot), |ui| {
                        for u in uses {
                            ui.label(
                                RichText::new(format!(
                                    "kit {:>3} {} · track {:>2} · {}",
                                    u.kit_index + 1,
                                    u.kit_name,
                                    u.track + 1,
                                    u.sound_name,
                                ))
                                .weak(),
                            );
                        }
                    });
                }
            },
        );

        ui.add_space(4.0);
        ui.collapsing(
            RichText::new(format!("Kits linked by patterns ({})", linked_kits.len())).strong(),
            |ui| {
                for &kit_index in &linked_kits {
                    let Some(kit) = model.kits.get(kit_index) else {
                        continue;
                    };
                    let patterns: Vec<String> = model
                        .patterns
                        .iter()
                        .filter(|p| p.kit_index == kit_index)
                        .map(|p| pattern_display_name(p.index))
                        .collect();
                    ui.collapsing(
                        format!(
                            "kit {:>3} · {} · patterns {}",
                            kit.index + 1,
                            kit.name,
                            patterns.join(" ")
                        ),
                        |ui| {
                            if let Some(assign) = kit_tracks(ui, kit) {
                                dropped = Some(assign);
                            }
                        },
                    );
                }
            },
        );
    });

    dropped
}

/// A draggable chip for one sample slot.
fn slot_chip(ui: &mut egui::Ui, slot: u8) {
    let id = Id::new(("slot-chip", slot));
    ui.dnd_drag_source(id, SlotDrag { slot }, |ui| {
        Frame::new()
            .fill(SLOT_COLOR.gamma_multiply(0.25))
            .corner_radius(4.0)
            .inner_margin(egui::Margin::symmetric(6, 2))
            .show(ui, |ui| {
                ui.label(
                    RichText::new(format!("slot {slot}"))
                        .color(SLOT_COLOR)
                        .strong(),
                );
            });
    });
}

/// One kit's 12 tracks, each row a drop zone for slot chips.
fn kit_tracks(ui: &mut egui::Ui, kit: &KitModel) -> Option<PendingAssign> {
    let mut dropped = None;
    for (track, sound) in kit.sounds.iter().enumerate() {
        let frame = Frame::new().inner_margin(egui::Margin::symmetric(4, 2));
        let (_, payload) = ui.dnd_drop_zone::<SlotDrag, ()>(frame, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{:>2}", track + 1)).weak());
                ui.label(&sound.name);
                ui.label(RichText::new(&sound.machine).weak());
                match sound.sample_slot {
                    Some(slot) => slot_chip(ui, slot),
                    None => {
                        ui.label(RichText::new("no sample").weak());
                    }
                }
            });
        });
        if let Some(drag) = payload {
            dropped = Some(PendingAssign {
                kit_index: kit.index,
                track,
                slot: drag.slot,
            });
        }
    }
    dropped
}
