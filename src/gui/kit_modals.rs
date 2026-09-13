//! Quick-rename modal for kits and track Sounds: a one-edit draft through
//! the same `Plan::edit_kit` pipeline as the kit editor (session backup,
//! drift guard, read-back verify) — no need to enter edit mode for a rename.

use eframe::egui::{self, RichText};

use crate::gui::app::{KitRenameTarget, KitsmithApp};
use crate::gui::theme;
use crate::model::edit::{FieldValue, KitEditDraft, KitField, NAME_MAX_LEN};
use crate::model::names;
use crate::ops::Plan;

pub fn show(app: &mut KitsmithApp, ctx: &egui::Context) {
    let Some(target) = app.ui_state.kit_rename else {
        return;
    };
    let (kit_index, field) = match target {
        KitRenameTarget::Kit(kit) => (kit, KitField::KitName),
        KitRenameTarget::Sound { kit, track } => (kit, KitField::SoundName(track)),
    };
    let current = current_name(app, target).unwrap_or_default();
    let title = match target {
        KitRenameTarget::Kit(kit) => format!("Rename kit {:02}?", kit + 1),
        KitRenameTarget::Sound { track, .. } => {
            format!("Rename the {} Sound?", names::track_label(track))
        }
    };
    let connected = app.connected_in_port().is_some();
    let busy = app.device_busy();
    let mut close = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    let mut confirm = false;

    egui::Window::new(title)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("New name").color(theme::TEXT_SECONDARY));
                let response = ui.add(
                    egui::TextEdit::singleline(&mut app.ui_state.kit_rename_draft)
                        .desired_width(theme::NAME_FIELD_WIDTH),
                );
                if response.changed() {
                    // Same constraints the device accepts (ASCII, ≤15).
                    app.ui_state
                        .kit_rename_draft
                        .retain(|c| c.is_ascii() && !c.is_control());
                    app.ui_state.kit_rename_draft.truncate(NAME_MAX_LEN);
                }
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new(
                    "Writes into whatever project is ACTIVE on the device, through the \
                     same backed-up, read-back-verified pipeline as the kit editor.",
                )
                .color(theme::TEXT_SECONDARY)
                .size(theme::FONT_META),
            );
            ui.add_space(theme::GAP_S);
            ui.horizontal(|ui| {
                let new_name = app.ui_state.kit_rename_draft.trim();
                let blocker = if !connected {
                    Some("Connect the Analog Rytm first")
                } else if busy {
                    Some("Another device operation is still running")
                } else if new_name.is_empty() || new_name == current {
                    Some("Type a different name")
                } else {
                    None
                };
                if ui
                    .add_enabled(blocker.is_none(), egui::Button::new("Rename on device"))
                    .on_disabled_hover_text(blocker.unwrap_or_default())
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
        let new_name = app.ui_state.kit_rename_draft.trim().to_owned();
        let mut draft = KitEditDraft::new(kit_index);
        draft.push(field, FieldValue::Text(new_name));
        match Plan::edit_kit(&draft, app.execute.backup_taken()) {
            Ok(plan) => {
                if let Some(port) = app.connected_in_port() {
                    if app.execute.launch(plan, port, ctx.clone(), None) {
                        // Rides the kit editor's save bookkeeping: a verified
                        // save mirrors into the model, a failed one hands the
                        // edit back as a draft.
                        app.ui_state.kits.pending_save = Some(draft);
                        app.ui_state.kit_rename = None;
                    } else {
                        app.toast(
                            "Another device operation is still running — try again when \
                             it finishes."
                                .to_owned(),
                        );
                    }
                }
            }
            Err(e) => app.toast(e.to_string()),
        }
    } else if close {
        app.ui_state.kit_rename = None;
    }
}

/// The target's current name (cleaned), for prefill and the unchanged check.
fn current_name(app: &KitsmithApp, target: KitRenameTarget) -> Option<String> {
    let model = app.model.as_ref()?;
    match target {
        KitRenameTarget::Kit(kit) => model
            .kits
            .get(kit)
            .map(|k| names::clean(&k.name).to_owned()),
        KitRenameTarget::Sound { kit, track } => model
            .kits
            .get(kit)
            .and_then(|k| k.sounds.get(track))
            .map(|s| names::clean(&s.name).to_owned()),
    }
}
