//! Dashboard: the selected kit as a hardware-mirroring pad grid, a kit strip,
//! and a sample-slot rail with cross-highlighting.
//!
//! Slots are drag sources; pads are drop zones. A completed drag returns a
//! [`PendingAssign`] for the app to preview — the dashboard never mutates
//! anything itself.

use eframe::egui::{self, Color32, CornerRadius, Frame, Id, Margin, RichText, Stroke, Vec2};

use crate::gui::execute::PendingAssign;
use crate::gui::theme::{SLOT_COLOR, TRACK_COLORS, TRACK_LABELS};
use crate::model::{pattern_display_name, KitModel, ProjectModel};

/// Payload carried by a slot drag.
struct SlotDrag {
    slot: u8,
}

/// UI selection state, owned by the app so it survives frames.
#[derive(Default)]
pub struct DashboardState {
    pub selected_kit: Option<usize>,
    /// Slot selected in the rail — pads using it light up.
    pub selected_slot: Option<u8>,
}

const PAD_SIZE: Vec2 = Vec2::new(148.0, 84.0);

/// Renders the dashboard. Returns a pending assignment when a slot chip was
/// dropped on a pad.
pub fn show(
    ui: &mut egui::Ui,
    model: &ProjectModel,
    state: &mut DashboardState,
) -> Option<PendingAssign> {
    let mut dropped = None;

    egui::Panel::right("slot-rail")
        .resizable(false)
        .default_size(230.0)
        .show(ui, |ui| {
            slot_rail(ui, model, state);
        });

    egui::CentralPanel::default().show(ui, |ui| {
        let linked = model.linked_kit_indices();
        // Keep a valid kit selected (first linked kit by default).
        let selected = state
            .selected_kit
            .filter(|k| model.kits.get(*k).is_some())
            .or_else(|| linked.first().copied())
            .unwrap_or(0);
        state.selected_kit = Some(selected);

        kit_strip(ui, model, &linked, state);
        ui.add_space(10.0);

        if let Some(kit) = model.kits.get(selected) {
            if let Some(assign) = pad_grid(ui, model, kit, state) {
                dropped = Some(assign);
            }
        }
    });

    dropped
}

/// Horizontal strip of selectable kit chips with their pattern badges.
fn kit_strip(
    ui: &mut egui::Ui,
    model: &ProjectModel,
    linked: &[usize],
    state: &mut DashboardState,
) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("KITS").weak().small());
        ui.label(
            RichText::new("· drag a slot from the right rail onto a pad to re-point it")
                .weak()
                .small(),
        );
    });
    ui.add_space(2.0);
    ui.horizontal_wrapped(|ui| {
        for &kit_index in linked {
            let Some(kit) = model.kits.get(kit_index) else {
                continue;
            };
            let patterns: Vec<String> = model
                .patterns
                .iter()
                .filter(|p| p.kit_index == kit_index)
                .map(|p| pattern_display_name(p.index))
                .collect();
            let selected = state.selected_kit == Some(kit_index);
            let label = format!("{}  {}", kit.name.trim(), patterns.join(" "));
            if ui.selectable_label(selected, label).clicked() {
                state.selected_kit = Some(kit_index);
            }
        }
    });
}

/// The 12 pads, laid out like the hardware: 3 rows of 4, BD row at the bottom.
fn pad_grid(
    ui: &mut egui::Ui,
    model: &ProjectModel,
    kit: &KitModel,
    state: &mut DashboardState,
) -> Option<PendingAssign> {
    let mut dropped = None;
    ui.label(RichText::new(format!("kit {:>3} · {}", kit.index + 1, kit.name)).strong());
    ui.add_space(6.0);
    // Display rows top→bottom = CH-row, BT-row, BD-row (mirrors the device).
    for row in [[8, 9, 10, 11], [4, 5, 6, 7], [0, 1, 2, 3]] {
        ui.horizontal(|ui| {
            for track in row {
                if let Some(assign) = pad(ui, model, kit, track, state) {
                    dropped = Some(assign);
                }
            }
        });
        ui.add_space(6.0);
    }
    dropped
}

/// One pad tile: identity stripe, sound name, machine, slot chip. Drop zone.
fn pad(
    ui: &mut egui::Ui,
    model: &ProjectModel,
    kit: &KitModel,
    track: usize,
    state: &mut DashboardState,
) -> Option<PendingAssign> {
    let sound = kit.sounds.get(track)?;
    let color = TRACK_COLORS[track];
    let highlighted = state.selected_slot.is_some() && sound.sample_slot == state.selected_slot;

    let frame = Frame::new()
        .fill(ui.visuals().extreme_bg_color)
        .stroke(if highlighted {
            Stroke::new(2.0, SLOT_COLOR)
        } else {
            Stroke::new(1.0, ui.visuals().weak_text_color().gamma_multiply(0.3))
        })
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::same(8));

    let mut clicked_slot = None;
    let (_, payload) = ui.dnd_drop_zone::<SlotDrag, ()>(frame, |ui| {
        ui.set_min_size(PAD_SIZE);
        ui.set_max_width(PAD_SIZE.x);
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(TRACK_LABELS[track]).color(color).strong());
                ui.with_layout(
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| match sound.sample_slot {
                        Some(slot) => {
                            if slot_chip(ui, slot, state.selected_slot == Some(slot)).clicked() {
                                clicked_slot = Some(slot);
                            }
                        }
                        None => {
                            ui.label(RichText::new("synth").weak().small());
                        }
                    },
                );
            });
            ui.add_space(2.0);
            ui.label(RichText::new(sound.name.trim()).size(14.0));
            ui.label(RichText::new(&sound.machine).weak().small());
            if let Some(name) = sound.sample_slot.and_then(|s| model.sample_name(s)) {
                ui.label(RichText::new(name).color(SLOT_COLOR).small());
            }
        });
    });

    if let Some(slot) = clicked_slot {
        state.selected_slot = (state.selected_slot != Some(slot)).then_some(slot);
    }

    payload.map(|drag| PendingAssign {
        kit_index: kit.index,
        track,
        slot: drag.slot,
    })
}

/// The right rail: every used slot as a draggable chip with a use count;
/// selecting one shows its users and highlights pads.
fn slot_rail(ui: &mut egui::Ui, model: &ProjectModel, state: &mut DashboardState) {
    let usage = model.slot_usage();
    ui.label(RichText::new("SAMPLE SLOTS").weak().small());
    ui.label(
        RichText::new(format!(
            "{} in use · click to trace, drag onto a pad",
            usage.len()
        ))
        .weak()
        .small(),
    );
    ui.add_space(6.0);

    egui::ScrollArea::vertical().show(ui, |ui| {
        if usage.is_empty() {
            ui.label(RichText::new("No kit references any sample slot yet.").weak());
        }
        for (slot, uses) in &usage {
            let selected = state.selected_slot == Some(*slot);
            ui.horizontal(|ui| {
                if slot_chip(ui, *slot, selected).clicked() {
                    state.selected_slot = (!selected).then_some(*slot);
                }
                match model.sample_name(*slot) {
                    Some(name) => {
                        ui.label(RichText::new(name).small());
                    }
                    None => {
                        ui.label(RichText::new("(name unknown — sync)").weak().small());
                    }
                }
                ui.label(RichText::new(format!("×{}", uses.len())).weak());
            });
            if selected {
                ui.indent(("slot-uses", slot), |ui| {
                    for u in uses {
                        ui.label(
                            RichText::new(format!(
                                "kit {} {} · {}",
                                u.kit_index + 1,
                                u.kit_name.trim(),
                                TRACK_LABELS[u.track.min(11)],
                            ))
                            .weak()
                            .small(),
                        );
                    }
                });
            }
            ui.add_space(2.0);
        }
    });
}

/// A draggable, clickable chip for one sample slot. Returns its response.
fn slot_chip(ui: &mut egui::Ui, slot: u8, selected: bool) -> egui::Response {
    let id = Id::new(("slot-chip", slot, ui.id()));
    let fill = if selected {
        SLOT_COLOR.gamma_multiply(0.55)
    } else {
        SLOT_COLOR.gamma_multiply(0.22)
    };
    ui.dnd_drag_source(id, SlotDrag { slot }, |ui| {
        Frame::new()
            .fill(fill)
            .corner_radius(CornerRadius::same(4))
            .inner_margin(Margin::symmetric(6, 2))
            .show(ui, |ui| {
                ui.label(
                    RichText::new(format!("⬦ {slot}"))
                        .color(if selected { Color32::WHITE } else { SLOT_COLOR })
                        .strong(),
                );
            });
    })
    .response
    .on_hover_text(format!("sample slot {slot} — drag onto a pad"))
}
