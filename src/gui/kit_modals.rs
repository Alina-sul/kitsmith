//! Quick-rename modal for kits and track Sounds: a one-edit draft through
//! the same `Plan::edit_kit` pipeline as the kit editor (session backup,
//! drift guard, read-back verify) — no need to enter edit mode for a rename.

use eframe::egui::{self, RichText};

use crate::gui::app::{KitRenameTarget, KitsmithApp, SaveOrigin};
use crate::gui::kit_editor::KIT_WRITE_POST_NOTE;
use crate::gui::theme;
use crate::model::edit::{FieldValue, KitEditDraft, KitField, NAME_MAX_LEN};
use crate::model::names;
use crate::ops::Plan;

/// Opens the modal on `target`, prefilled with the current name so an
/// untouched draft reads as "unchanged", never as an empty error.
pub fn open(app: &mut KitsmithApp, target: KitRenameTarget) {
    app.ui_state.kit_rename_draft = current_name(app, target).unwrap_or_default();
    app.ui_state.kit_rename = Some(target);
}

pub fn show(app: &mut KitsmithApp, ctx: &egui::Context) {
    let Some(target) = app.ui_state.kit_rename else {
        return;
    };
    let (kit_index, field) = match target {
        KitRenameTarget::Kit(kit) => (kit, KitField::KitName),
        KitRenameTarget::Sound { kit, track } => (kit, KitField::SoundName(track)),
    };
    let current = current_name(app, target).unwrap_or_default();
    // The old name stays visible in the title even after the field is
    // retyped — intent-with-noun, like every other confirm.
    let title = match target {
        KitRenameTarget::Kit(kit) if current.is_empty() => format!("Rename Kit {:02}?", kit + 1),
        KitRenameTarget::Kit(kit) => format!("Rename Kit {:02} · {current}?", kit + 1),
        KitRenameTarget::Sound { track, .. } if current.is_empty() => {
            format!("Rename the {} Sound?", names::track_label(track))
        }
        KitRenameTarget::Sound { track, .. } => {
            format!("Rename {} · {current}?", names::track_label(track))
        }
    };
    // A draft the user typed in the editor for this same field would win
    // over this rename when it is saved later — say so up front.
    let draft_conflict = app
        .ui_state
        .kits
        .edit_draft
        .as_ref()
        .is_some_and(|d| d.kit_index == kit_index && d.value(field).is_some());
    let connected = app.connected_in_port().is_some();
    let busy = app.device_busy();
    let mut close = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    let mut confirm = false;

    egui::Window::new(title)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            let submitted = ui
                .horizontal(|ui| {
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
                    response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))
                })
                .inner;
            ui.add_space(4.0);
            ui.label(
                RichText::new(
                    "Writes into whatever project is ACTIVE on the device. Make sure \
                     the right project is loaded and synced.",
                )
                .color(theme::WARNING),
            );
            if draft_conflict {
                ui.label(
                    RichText::new(
                        "This kit has an open edit draft that also changes this name — \
                         saving that draft later would overwrite this rename.",
                    )
                    .color(theme::WARNING),
                );
            }
            ui.add_space(theme::GAP_S);

            let new_name = app.ui_state.kit_rename_draft.trim().to_owned();
            let not_ready = new_name.is_empty() || new_name == current;
            let plan = (!not_ready).then(|| {
                let mut draft = KitEditDraft::new(kit_index);
                draft.push(field, FieldValue::Text(new_name.clone()));
                (Plan::edit_kit(&draft, app.execute.backup_taken()), draft)
            });
            match &plan {
                Some((Ok(plan), _)) => {
                    if !app.execute.backup_taken() {
                        ui.label(
                            RichText::new("A backup will be created before writing.")
                                .color(theme::TEXT_SECONDARY),
                        );
                    }
                    ui.label(
                        RichText::new("The rename is verified by reading the kit back.")
                            .color(theme::TEXT_SECONDARY),
                    );
                    ui.collapsing("Technical steps", |ui| {
                        for step in &plan.steps {
                            ui.label(
                                RichText::new(format!("• {}", step.describe()))
                                    .color(theme::TEXT_SECONDARY)
                                    .size(theme::FONT_META),
                            );
                        }
                    });
                }
                Some((Err(e), _)) => {
                    ui.label(RichText::new(e.to_string()).color(theme::DANGER));
                }
                None => {}
            }
            ui.add_space(theme::GAP_S);
            ui.horizontal(|ui| {
                let blocker = if !connected {
                    Some("Connect the Analog Rytm first")
                } else if busy {
                    Some("Another device operation is still running")
                } else if not_ready {
                    Some("Type a different name")
                } else if matches!(&plan, Some((Err(_), _))) {
                    Some("Fix the error above first")
                } else {
                    None
                };
                let click = ui
                    .add_enabled(blocker.is_none(), egui::Button::new("Rename on device"))
                    .on_disabled_hover_text(blocker.unwrap_or_default())
                    .clicked();
                if blocker.is_none() && (click || submitted) {
                    confirm = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
            if confirm {
                if let Some((Ok(plan), draft)) = plan {
                    if let Some(port) = app.connected_in_port() {
                        if app.execute.launch(
                            plan,
                            port,
                            ctx.clone(),
                            Some(KIT_WRITE_POST_NOTE.to_owned()),
                        ) {
                            app.ui_state.kits.pending_save = Some((draft, SaveOrigin::QuickRename));
                            app.ui_state.kit_rename = None;
                        } else {
                            app.toast(
                                "Another device operation is still running — try again \
                                 when it finishes."
                                    .to_owned(),
                            );
                        }
                    }
                }
            }
        });

    if close {
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
