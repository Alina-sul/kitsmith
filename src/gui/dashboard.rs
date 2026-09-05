//! Read-only dashboard view of a synced project model.

use eframe::egui::{self, Color32, RichText};

use crate::model::{pattern_display_name, ProjectModel};

/// Renders the project model: summary, sample-slot usage, kits.
pub fn show(ui: &mut egui::Ui, model: &ProjectModel) {
    let usage = model.slot_usage();
    let linked_kits = model.linked_kit_indices();

    ui.label(format!(
        "{} sample slots referenced · {} kits linked by patterns · {} pool sounds",
        usage.len(),
        linked_kits.len(),
        model.pool_sounds.len(),
    ));
    ui.add_space(8.0);

    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.collapsing(
            RichText::new(format!("Sample slots in use ({})", usage.len())).strong(),
            |ui| {
                if usage.is_empty() {
                    ui.label(RichText::new("No kit references any sample slot.").weak());
                }
                for (slot, uses) in &usage {
                    ui.collapsing(format!("slot {slot:>3} — {} use(s)", uses.len()), |ui| {
                        for u in uses {
                            ui.label(format!(
                                "kit {:>3} {:<16} track {:>2} · {}",
                                u.kit_index + 1,
                                u.kit_name,
                                u.track + 1,
                                u.sound_name,
                            ));
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
                        |ui| kit_sounds(ui, kit),
                    );
                }
            },
        );
    });
}

fn kit_sounds(ui: &mut egui::Ui, kit: &crate::model::KitModel) {
    egui::Grid::new(("kit-sounds", kit.index))
        .striped(true)
        .show(ui, |ui| {
            ui.label(RichText::new("track").weak());
            ui.label(RichText::new("sound").weak());
            ui.label(RichText::new("machine").weak());
            ui.label(RichText::new("sample slot").weak());
            ui.end_row();
            for (track, sound) in kit.sounds.iter().enumerate() {
                ui.label(format!("{}", track + 1));
                ui.label(&sound.name);
                ui.label(&sound.machine);
                match sound.sample_slot {
                    Some(slot) => ui.label(
                        RichText::new(format!("{slot}")).color(Color32::from_rgb(0x9B, 0x59, 0xB6)),
                    ),
                    None => ui.label(RichText::new("—").weak()),
                };
                ui.end_row();
            }
        });
}
