//! Sound Pool page (PRD §25): the 128 per-project Sound slots that pattern
//! Sound Locks select from — dependencies invisible in the kit view.
//! Read-only except renaming (right-click), which rides the verified
//! object-write pipeline.

use eframe::egui::{self, RichText, ScrollArea};

use crate::gui::app::KitsmithApp;
use crate::gui::{theme, widgets};
use crate::model::names::{self, ResolvedName};
use crate::model::pattern_display_name;

pub fn show(app: &mut KitsmithApp, ui: &mut egui::Ui) {
    if app.is_browsing_snapshot() {
        ui.label(
            RichText::new(
                "The Sound Pool region of project backups is not decoded yet — load the \
                 project on the device and sync to inspect its pool.",
            )
            .color(theme::TEXT_MUTED),
        );
        return;
    }
    let Some(model) = app.model.as_ref() else {
        return;
    };
    let filled = model
        .pool_sounds
        .iter()
        .filter(|s| !s.name.trim().is_empty() || s.sample_slot.is_some())
        .count();
    ui.label(
        RichText::new(format!(
            "{filled} of {} Sound Pool slots in use",
            model.pool_sounds.len()
        ))
        .color(theme::TEXT_SECONDARY),
    );
    if !model.dependency_scan {
        ui.label(
            RichText::new("Old cache — re-sync to index which patterns Sound-lock these sounds.")
                .color(theme::TEXT_MUTED)
                .size(theme::FONT_META),
        );
    }
    ui.add_space(theme::GAP_S);

    let mut rename_req: Option<(u8, String)> = None;
    ScrollArea::vertical().show(ui, |ui| {
        for (index, sound) in model.pool_sounds.iter().enumerate() {
            let pool_index = index as u8;
            let empty = sound.name.trim().is_empty() && sound.sample_slot.is_none();
            if empty {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("   {:03}", index + 1))
                            .color(theme::TEXT_MUTED)
                            .monospace(),
                    );
                    ui.label(
                        RichText::new("Empty")
                            .color(theme::TEXT_MUTED)
                            .size(theme::FONT_META),
                    );
                });
                continue;
            }
            let row = ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("{:03}", index + 1))
                        .color(theme::TEXT_MUTED)
                        .monospace(),
                );
                let name = match names::sound_display_name(&sound.name, 0) {
                    ResolvedName::Native(name) => {
                        RichText::new(name).color(theme::TEXT_PRIMARY).strong()
                    }
                    _ => RichText::new(format!("Pool Sound {:03}", index + 1))
                        .color(theme::TEXT_SECONDARY),
                };
                ui.label(name);
                ui.label(
                    RichText::new(&sound.machine)
                        .color(theme::TEXT_SECONDARY)
                        .size(theme::FONT_META),
                );
                if let Some(slot) = sound.sample_slot {
                    let label = match names::sample_display_name(model.sample_name(slot)) {
                        ResolvedName::Native(name) => format!("{slot:02} · {name}"),
                        _ => format!("{slot:02} · name unavailable"),
                    };
                    let _ = widgets::sample_pill(ui, &label, false);
                }
                let patterns = model.patterns_sound_locking(pool_index);
                if !patterns.is_empty() {
                    let list = patterns
                        .iter()
                        .map(|i| pattern_display_name(*i))
                        .collect::<Vec<_>>()
                        .join(" · ");
                    ui.label(
                        RichText::new(format!("Sound-locked in {list}"))
                            .color(theme::SAMPLE_PURPLE)
                            .size(theme::FONT_META),
                    );
                }
            });
            row.response
                .interact(egui::Sense::click())
                .context_menu(|ui| {
                    if ui
                        .button("Rename…")
                        .on_hover_text(
                            "Renames this pool Sound on the device — same verified \
                             pipeline as kit renames.",
                        )
                        .clicked()
                    {
                        rename_req = Some((pool_index, sound.name.clone()));
                        ui.close();
                    }
                });
        }
    });
    if let Some((slot, _)) = rename_req {
        crate::gui::kit_modals::open(app, crate::gui::app::KitRenameTarget::PoolSound(slot));
    }
}
