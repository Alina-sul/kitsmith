//! Confirmation modals for file-protocol write operations: native project
//! rename, project copy to an empty slot, and loading a +Drive sample into a
//! project slot. Intent-named titles, explicit consequences, one confirm.

use eframe::egui::{self, RichText};

use crate::gui::app::RytmdashApp;
use crate::gui::fileops::FileOp;
use crate::gui::theme;
use crate::model::names;

pub fn show(app: &mut RytmdashApp, ctx: &egui::Context) {
    rename_modal(app, ctx);
    copy_modal(app, ctx);
    load_sample_modal(app, ctx);
}

fn start_op(app: &mut RytmdashApp, ctx: &egui::Context, op: FileOp) {
    let Some(port) = app.connected_in_port() else {
        app.toast("Connect the Rytm first".to_owned());
        return;
    };
    let repaint = ctx.clone();
    app.fileops
        .start(port, op, move || repaint.request_repaint());
}

fn rename_modal(app: &mut RytmdashApp, ctx: &egui::Context) {
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

fn copy_modal(app: &mut RytmdashApp, ctx: &egui::Context) {
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

fn load_sample_modal(app: &mut RytmdashApp, ctx: &egui::Context) {
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
