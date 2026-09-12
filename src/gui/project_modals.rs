//! Confirmation modals for file-protocol write operations: native project
//! rename, project copy to an empty slot, and loading a +Drive sample into a
//! project slot. Intent-named titles, explicit consequences, one confirm.

use eframe::egui::{self, RichText};

use crate::gui::app::KitsmithApp;
use crate::gui::fileops::FileOp;
use crate::gui::theme;
use crate::model::names;

pub fn show(app: &mut KitsmithApp, ctx: &egui::Context) {
    rename_modal(app, ctx);
    copy_modal(app, ctx);
    move_modal(app, ctx);
    swap_modal(app, ctx);
    clear_modal(app, ctx);
    load_sample_modal(app, ctx);
}

/// The framing for slot moves/swaps/clears comes from the protocol spec and
/// is not hardware-verified yet — the modals say so.
const SLOT_OP_UNVERIFIED: &str = "This operation's wire format is not hardware-verified yet — \
     first use should target scratch slots (docs/hardware-verification.md).";

/// The one guard the hardware allows: the active project slot is unreadable,
/// so the user attests the touched slots aren't the loaded project.
fn slot_op_guards(app: &mut KitsmithApp, ui: &mut egui::Ui, slots: &[u32]) {
    for &slot in slots {
        if app.active_guess == Some(slot) {
            ui.label(
                RichText::new(format!(
                    "Slot {slot:03} looks like the project currently loaded on the \
                     device — saving the project on the device afterwards would \
                     write to its old slot number.",
                ))
                .color(theme::WARNING),
            );
        }
    }
    let label = match slots {
        [slot] => format!("The project in slot {slot:03} is not currently loaded on the device"),
        _ => "Neither project is currently loaded on the device".to_owned(),
    };
    ui.checkbox(&mut app.ui_state.slot_op_ack, label);
    ui.label(
        RichText::new(SLOT_OP_UNVERIFIED)
            .color(theme::TEXT_MUTED)
            .size(theme::FONT_META),
    );
}

/// The project's display name for modal titles.
fn slot_display_name(app: &KitsmithApp, slot: u32) -> String {
    app.model
        .as_ref()
        .and_then(|m| m.projects.iter().find(|p| p.id == slot))
        .map(|p| names::project_display_name(&p.name, slot).text().to_owned())
        .unwrap_or_else(|| format!("Project {slot:03}"))
}

fn move_modal(app: &mut KitsmithApp, ctx: &egui::Context) {
    let Some((src, mut dst)) = app.ui_state.move_modal else {
        return;
    };
    let connected = app.connected_in_port().is_some();
    let busy = app.fileops.is_running();
    let src_name = slot_display_name(app, src);
    let dst_occupied = app
        .model
        .as_ref()
        .is_some_and(|m| m.projects.iter().any(|p| p.id == dst));
    let mut close = false;
    let mut confirm = false;
    let mut copy_instead = false;

    egui::Window::new(format!("Move {src_name} to another slot?"))
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Destination slot:");
                ui.add(egui::DragValue::new(&mut dst).range(1..=128));
            });
            if dst_occupied {
                ui.label(
                    RichText::new(format!("Slot {dst:03} is occupied — pick an empty slot."))
                        .color(theme::DANGER),
                );
            }
            if dst == src {
                ui.label(RichText::new("Destination is the source slot.").color(theme::DANGER));
            }
            ui.add_space(4.0);
            ui.label(
                RichText::new(
                    "The whole project is downloaded and kept as a local backup first \
                     (~10 s); after the device-side move, the destination is downloaded \
                     again and compared byte-for-byte.",
                )
                .color(theme::TEXT_SECONDARY)
                .size(theme::FONT_META),
            );
            slot_op_guards(app, ui, &[src]);
            ui.add_space(theme::GAP_S);
            ui.horizontal(|ui| {
                let enabled =
                    connected && !busy && !dst_occupied && dst != src && app.ui_state.slot_op_ack;
                if ui
                    .add_enabled(enabled, egui::Button::new(format!("Move to slot {dst:03}")))
                    .on_disabled_hover_text(
                        "Connect the Rytm, pick an empty destination, and confirm the \
                         project isn't loaded",
                    )
                    .clicked()
                {
                    confirm = true;
                }
                if ui.button("Copy instead").clicked() {
                    copy_instead = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });

    app.ui_state.move_modal = Some((src, dst));
    if confirm {
        app.ui_state.move_modal = None;
        start_op(app, ctx, FileOp::MoveProject { src, dst });
    } else if copy_instead {
        app.ui_state.move_modal = None;
        app.ui_state.copy_modal = Some((src, dst));
    } else if close {
        app.ui_state.move_modal = None;
    }
}

fn swap_modal(app: &mut KitsmithApp, ctx: &egui::Context) {
    let Some((a, mut b)) = app.ui_state.swap_modal else {
        return;
    };
    let connected = app.connected_in_port().is_some();
    let busy = app.fileops.is_running();
    let a_name = slot_display_name(app, a);
    let b_occupied = app
        .model
        .as_ref()
        .is_some_and(|m| m.projects.iter().any(|p| p.id == b));
    let mut close = false;
    let mut confirm = false;

    egui::Window::new(format!("Swap {a_name} with another project?"))
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Swap with slot:");
                ui.add(egui::DragValue::new(&mut b).range(1..=128));
                ui.label(RichText::new(slot_display_name(app, b)).color(theme::TEXT_SECONDARY));
            });
            if !b_occupied {
                ui.label(
                    RichText::new(format!("Slot {b:03} is empty — use Move instead of Swap."))
                        .color(theme::DANGER),
                );
            }
            if b == a {
                ui.label(RichText::new("Pick two different slots.").color(theme::DANGER));
            }
            ui.add_space(4.0);
            ui.label(
                RichText::new(
                    "Both projects are downloaded and kept as local backups first \
                     (~20 s); after the device-side swap, both slots are downloaded \
                     again and compared crosswise byte-for-byte.",
                )
                .color(theme::TEXT_SECONDARY)
                .size(theme::FONT_META),
            );
            slot_op_guards(app, ui, &[a, b]);
            ui.add_space(theme::GAP_S);
            ui.horizontal(|ui| {
                let enabled =
                    connected && !busy && b_occupied && b != a && app.ui_state.slot_op_ack;
                if ui
                    .add_enabled(enabled, egui::Button::new(format!("Swap with slot {b:03}")))
                    .on_disabled_hover_text(
                        "Connect the Rytm, pick an occupied slot, and confirm neither \
                         project is loaded",
                    )
                    .clicked()
                {
                    confirm = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });

    app.ui_state.swap_modal = Some((a, b));
    if confirm {
        app.ui_state.swap_modal = None;
        start_op(app, ctx, FileOp::SwapProjects { a, b });
    } else if close {
        app.ui_state.swap_modal = None;
    }
}

fn clear_modal(app: &mut KitsmithApp, ctx: &egui::Context) {
    let Some(slot) = app.ui_state.clear_modal_slot else {
        return;
    };
    let connected = app.connected_in_port().is_some();
    let busy = app.fileops.is_running();
    let name = slot_display_name(app, slot);
    let mut close = false;
    let mut confirm = false;

    egui::Window::new(format!("Clear slot {slot:03} ({name})?"))
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(
                RichText::new(
                    "The project is removed from the device. It is downloaded and \
                     stored as a local backup first — if that backup cannot be \
                     saved, nothing is cleared.",
                )
                .color(theme::WARNING),
            );
            ui.add_space(4.0);
            slot_op_guards(app, ui, &[slot]);
            ui.add_space(theme::GAP_S);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("Type “{slot:03}” to confirm"))
                        .color(theme::TEXT_SECONDARY),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut app.ui_state.clear_confirm_draft)
                        .desired_width(60.0),
                );
            });
            ui.add_space(theme::GAP_S);
            ui.horizontal(|ui| {
                let typed = app.ui_state.clear_confirm_draft.trim() == format!("{slot:03}");
                let enabled = connected && !busy && typed && app.ui_state.slot_op_ack;
                if ui
                    .add_enabled(enabled, egui::Button::new("Clear slot"))
                    .on_disabled_hover_text(
                        "Connect the Rytm, type the slot number, and confirm the \
                         project isn't loaded",
                    )
                    .clicked()
                {
                    confirm = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });

    if confirm {
        app.ui_state.clear_modal_slot = None;
        app.ui_state.clear_confirm_draft.clear();
        start_op(app, ctx, FileOp::ClearProject { slot });
    } else if close {
        app.ui_state.clear_modal_slot = None;
        app.ui_state.clear_confirm_draft.clear();
    }
}

fn start_op(app: &mut KitsmithApp, ctx: &egui::Context, op: FileOp) {
    let Some(port) = app.connected_in_port() else {
        app.toast("Connect the Rytm first".to_owned());
        return;
    };
    let repaint = ctx.clone();
    app.fileops
        .start(port, op, move || repaint.request_repaint());
}

fn rename_modal(app: &mut KitsmithApp, ctx: &egui::Context) {
    let Some(slot) = app.ui_state.rename_modal_slot else {
        return;
    };
    let connected = app.connected_in_port().is_some();
    let busy = app.fileops.is_running();
    let mut close = false;
    let mut confirm = false;

    egui::Window::new(format!("Rename project {slot:03} on the device?"))
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label("Writes the new native name into the project slot (up to 15 characters).");
            ui.add(
                egui::TextEdit::singleline(&mut app.ui_state.rename_draft)
                    .char_limit(15)
                    .desired_width(220.0),
            );
            ui.add_space(4.0);
            ui.label(
                RichText::new(
                    "The whole project is downloaded first and kept as a local backup, then \
                     re-uploaded with only the name changed and verified byte-for-byte \
                     (~20 s). Any mismatch restores the original.",
                )
                .color(theme::TEXT_SECONDARY)
                .size(theme::FONT_META),
            );
            if app.active_guess == Some(slot) {
                ui.label(
                    RichText::new(
                        "This looks like the project currently loaded on the device — saving \
                         the project on the device afterwards would overwrite the new name.",
                    )
                    .color(theme::WARNING),
                );
            }
            ui.add_space(theme::GAP_S);
            ui.horizontal(|ui| {
                let enabled =
                    connected && !busy && !names::clean(&app.ui_state.rename_draft).is_empty();
                if ui
                    .add_enabled(enabled, egui::Button::new("Rename project"))
                    .on_disabled_hover_text("Connect the Rytm and enter a name")
                    .clicked()
                {
                    confirm = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });

    if confirm {
        let new_name = names::clean(&app.ui_state.rename_draft).to_owned();
        app.ui_state.rename_modal_slot = None;
        start_op(app, ctx, FileOp::RenameProject { slot, new_name });
    } else if close {
        app.ui_state.rename_modal_slot = None;
    }
}

fn copy_modal(app: &mut KitsmithApp, ctx: &egui::Context) {
    let Some((src, mut dst)) = app.ui_state.copy_modal else {
        return;
    };
    let connected = app.connected_in_port().is_some();
    let busy = app.fileops.is_running();
    let src_name = app
        .model
        .as_ref()
        .and_then(|m| m.projects.iter().find(|p| p.id == src))
        .map(|p| names::project_display_name(&p.name, src).text().to_owned())
        .unwrap_or_else(|| format!("Project {src:03}"));
    let dst_occupied = app
        .model
        .as_ref()
        .is_some_and(|m| m.projects.iter().any(|p| p.id == dst));
    let mut close = false;
    let mut confirm = false;

    egui::Window::new(format!("Copy {src_name} to another slot?"))
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Destination slot:");
                ui.add(egui::DragValue::new(&mut dst).range(1..=128));
            });
            if dst_occupied {
                ui.label(
                    RichText::new(format!("Slot {dst:03} is occupied — pick an empty slot."))
                        .color(theme::DANGER),
                );
            }
            if dst == src {
                ui.label(RichText::new("Destination is the source slot.").color(theme::DANGER));
            }
            ui.add_space(4.0);
            ui.label(
                RichText::new(
                    "The device copies the project internally; the source is not changed. The \
                     copy is verified and indexed afterwards.",
                )
                .color(theme::TEXT_SECONDARY)
                .size(theme::FONT_META),
            );
            ui.add_space(theme::GAP_S);
            ui.horizontal(|ui| {
                let enabled = connected && !busy && !dst_occupied && dst != src;
                if ui
                    .add_enabled(enabled, egui::Button::new(format!("Copy to slot {dst:03}")))
                    .on_disabled_hover_text("Connect the Rytm and pick an empty destination")
                    .clicked()
                {
                    confirm = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });

    app.ui_state.copy_modal = Some((src, dst));
    if confirm {
        app.ui_state.copy_modal = None;
        start_op(app, ctx, FileOp::CopyProject { src, dst });
    } else if close {
        app.ui_state.copy_modal = None;
    }
}

fn load_sample_modal(app: &mut KitsmithApp, ctx: &egui::Context) {
    let Some(path) = app.ui_state.load_sample_modal.clone() else {
        return;
    };
    let connected = app.connected_in_port().is_some();
    let busy = app.fileops.is_running();
    let name = path.rsplit('/').next().unwrap_or(&path).to_owned();
    let mut slot = app.ui_state.load_sample_slot;
    let slot_taken = app
        .model
        .as_ref()
        .is_some_and(|m| m.samples.iter().any(|s| s.slot == slot));
    let mut close = false;
    let mut confirm = false;

    egui::Window::new(format!("Load {name} into this project?"))
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(
                RichText::new(&path)
                    .color(theme::TEXT_MUTED)
                    .size(theme::FONT_META),
            );
            ui.horizontal(|ui| {
                ui.label("Project sample slot:");
                ui.add(egui::DragValue::new(&mut slot).range(1..=127));
            });
            if slot_taken {
                ui.label(
                    RichText::new(format!("Slot {slot:03} already holds a sample."))
                        .color(theme::DANGER),
                );
            }
            ui.add_space(4.0);
            ui.label(
                RichText::new(
                    "Loads into the ACTIVE project's sample RAM (like SAMPLES → LOAD TO PROJ) \
                     and verifies the slot afterwards. Save the project on the device to keep \
                     it. Then drag the new slot onto a pad to assign it.",
                )
                .color(theme::TEXT_SECONDARY)
                .size(theme::FONT_META),
            );
            ui.add_space(theme::GAP_S);
            ui.horizontal(|ui| {
                let enabled = connected && !busy && !slot_taken;
                if ui
                    .add_enabled(
                        enabled,
                        egui::Button::new(format!("Load into slot {slot:03}")),
                    )
                    .on_disabled_hover_text("Connect the Rytm and pick a free slot")
                    .clicked()
                {
                    confirm = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });

    app.ui_state.load_sample_slot = slot;
    if confirm {
        app.ui_state.load_sample_modal = None;
        start_op(
            app,
            ctx,
            FileOp::LoadDriveSample {
                drive_path: path,
                slot,
            },
        );
    } else if close {
        app.ui_state.load_sample_modal = None;
    }
}
