//! Kit editor v0 (WP19 slice 1): rename + machine-independent parameter
//! edits on an existing kit. Editing is an explicit mode — the reading
//! surface stays untouched until "Edit kit" is pressed — and every change is
//! collected into a [`KitEditDraft`]; nothing reaches the device until the
//! save plan is confirmed and executed through `ops/`.

use eframe::egui::{self, RichText};

use crate::gui::app::{KitsState, KitsmithApp};
use crate::gui::{theme, widgets};
use crate::model::edit::{FieldValue, KitEditDraft, KitField, NAME_MAX_LEN};
use crate::model::{names, KitModel, ProjectModel, TrackParams};

/// Whether the draft (if any) targets this kit.
pub fn is_editing(state: &KitsState, kit_index: usize) -> bool {
    state
        .edit_draft
        .as_ref()
        .is_some_and(|d| d.kit_index == kit_index)
}

/// The edit-mode bar under the kit header: kit rename, pending-change count,
/// and the Save / Discard actions. Returns nothing; state changes live in
/// `state.edit_draft` / `state.save_modal_open`.
pub fn edit_bar(ui: &mut egui::Ui, kit: &KitModel, state: &mut KitsState) {
    let mut discard = false;
    let mut open_save = false;
    let Some(draft) = state.edit_draft.as_mut() else {
        return;
    };
    egui::Frame::new()
        .fill(theme::SURFACE)
        .stroke(egui::Stroke::new(1.0, theme::WARNING))
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                widgets::badge(ui, "EDITING", theme::WARNING);
                ui.label(RichText::new("Kit name").color(theme::TEXT_SECONDARY));
                name_field(ui, draft, KitField::KitName, names::clean(&kit.name));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Discard").clicked() {
                        discard = true;
                    }
                    let pending = draft.edits.len();
                    if ui
                        .add_enabled(
                            pending > 0,
                            egui::Button::new(format!("Save to device… ({pending})")),
                        )
                        .on_disabled_hover_text("No changes yet")
                        .clicked()
                    {
                        open_save = true;
                    }
                });
            });
            ui.label(
                RichText::new(
                    "Changes stay local until saved. Select a pad to edit its parameters.",
                )
                .color(theme::TEXT_MUTED)
                .size(theme::FONT_META),
            );
        });
    if discard {
        state.edit_draft = None;
    }
    if open_save {
        state.save_modal_open = true;
    }
}

/// Editable inspector for one track: Sound name, track level, and the
/// machine-independent SMPL / AMP / FLTR pages.
pub fn track_editor(ui: &mut egui::Ui, model: &ProjectModel, state: &mut KitsState) {
    let Some(kit) = state.selected_kit.and_then(|k| model.kits.get(k)) else {
        return;
    };
    let Some(track) = state.selected_track else {
        return;
    };
    let Some(sound) = kit.sounds.get(track) else {
        return;
    };
    let mut close = false;
    let Some(draft) = state.edit_draft.as_mut() else {
        return;
    };
    let params = &sound.params;

    ui.horizontal(|ui| {
        ui.label(
            RichText::new(names::track_label(track))
                .color(theme::TRACK_COLORS[track])
                .size(theme::FONT_SECTION)
                .strong(),
        );
        ui.label(
            RichText::new("Editing")
                .color(theme::WARNING)
                .size(theme::FONT_META),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("×").on_hover_text("Close inspector").clicked() {
                close = true;
            }
        });
    });
    ui.add_space(theme::GAP_M);

    egui::ScrollArea::vertical()
        .id_salt("track-editor-scroll")
        .show(ui, |ui| {
            widgets::inspector_group(ui, "SOUND", |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Name").color(theme::TEXT_SECONDARY));
                    name_field(
                        ui,
                        draft,
                        KitField::SoundName(track),
                        names::clean(&sound.name),
                    );
                });
                int_field(ui, draft, params, KitField::TrackLevel(track), "Level");
            });
            widgets::inspector_group(ui, "SMPL", |ui| {
                int_field(ui, draft, params, KitField::SampleTune(track), "Tune");
                int_field(
                    ui,
                    draft,
                    params,
                    KitField::SampleFineTune(track),
                    "Fine tune",
                );
                int_field(
                    ui,
                    draft,
                    params,
                    KitField::SampleBitReduction(track),
                    "Bit reduction",
                );
                float_field(ui, draft, params, KitField::SampleStart(track), "Start");
                float_field(ui, draft, params, KitField::SampleEnd(track), "End");
                toggle_field(ui, draft, params, KitField::SampleLoop(track), "Loop");
                int_field(ui, draft, params, KitField::SampleLevel(track), "Level");
            });
            widgets::inspector_group(ui, "AMP", |ui| {
                int_field(ui, draft, params, KitField::AmpAttack(track), "Attack");
                int_field(ui, draft, params, KitField::AmpHold(track), "Hold");
                int_field(ui, draft, params, KitField::AmpDecay(track), "Decay");
                int_field(
                    ui,
                    draft,
                    params,
                    KitField::AmpOverdrive(track),
                    "Overdrive",
                );
                int_field(
                    ui,
                    draft,
                    params,
                    KitField::AmpDelaySend(track),
                    "Delay send",
                );
                int_field(
                    ui,
                    draft,
                    params,
                    KitField::AmpReverbSend(track),
                    "Reverb send",
                );
                int_field(ui, draft, params, KitField::AmpPan(track), "Pan");
                int_field(ui, draft, params, KitField::AmpVolume(track), "Volume");
            });
            widgets::inspector_group(ui, "FLTR", |ui| {
                int_field(ui, draft, params, KitField::FltAttack(track), "Attack");
                int_field(ui, draft, params, KitField::FltSustain(track), "Sustain");
                int_field(ui, draft, params, KitField::FltDecay(track), "Decay");
                int_field(ui, draft, params, KitField::FltRelease(track), "Release");
                int_field(ui, draft, params, KitField::FltCutoff(track), "Cutoff");
                int_field(
                    ui,
                    draft,
                    params,
                    KitField::FltResonance(track),
                    "Resonance",
                );
                int_field(
                    ui,
                    draft,
                    params,
                    KitField::FltEnvAmount(track),
                    "Env amount",
                );
            });
        });
    if close {
        state.selected_track = None;
    }
}

/// The save-plan preview modal: what changes, what it affects, backup and
/// verification notices. Confirming launches the plan through `ExecutePanel`.
pub fn save_modal(app: &mut KitsmithApp, ctx: &egui::Context) {
    if !app.ui_state.kits.save_modal_open {
        return;
    }
    let Some(draft) = app.ui_state.kits.edit_draft.clone() else {
        app.ui_state.kits.save_modal_open = false;
        return;
    };
    let connected = app.connected_in_port();
    let used_by: Vec<String> = app.model.as_ref().map_or_else(Vec::new, |m| {
        m.patterns
            .iter()
            .filter(|p| p.kit_index == draft.kit_index)
            .map(|p| crate::model::pattern_display_name(p.index))
            .collect()
    });
    let plan = crate::ops::Plan::edit_kit(&draft, app.execute.backup_taken());
    let mut close = false;
    let mut confirm = false;

    egui::Window::new(format!("Save changes to kit {:02}?", draft.kit_index + 1))
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(
                RichText::new(
                    "Saving writes into whatever project is ACTIVE on the device. Make sure \
                     the right project is loaded and synced.",
                )
                .color(theme::WARNING),
            );
            ui.add_space(theme::GAP_S);
            widgets::section_label(ui, "CHANGES");
            for edit in &draft.edits {
                ui.label(edit.describe());
            }
            if !used_by.is_empty() {
                ui.add_space(theme::GAP_S);
                ui.label(
                    RichText::new(format!(
                        "This kit is used by {} — the changes affect what those patterns play.",
                        used_by.join(" · ")
                    ))
                    .color(theme::WARNING),
                );
            }
            ui.add_space(theme::GAP_S);
            match &plan {
                Ok(plan) => {
                    if !app.execute.backup_taken() {
                        ui.label(
                            RichText::new("A backup will be created before writing.")
                                .color(theme::TEXT_SECONDARY),
                        );
                    }
                    ui.label(
                        RichText::new("Every edited field is verified by reading the kit back.")
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
                Err(e) => {
                    ui.label(RichText::new(e.to_string()).color(theme::DANGER));
                }
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        connected.is_some() && plan.is_ok(),
                        egui::Button::new("Save to device"),
                    )
                    .on_disabled_hover_text("Connect the Rytm first")
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
        if let (Ok(plan), Some(port)) = (plan, connected) {
            app.ui_state.kits.save_modal_open = false;
            // The draft rides along so a verified save can update the local
            // model (and a failed one restores the editor).
            app.ui_state.kits.pending_save = Some(draft);
            app.ui_state.kits.edit_draft = None;
            app.execute.launch(
                plan,
                port,
                ctx.clone(),
                Some(
                    "If this kit is loaded on the device, reload it there to hear the \
                     changes. Save the project on the device (PROJECT → SAVE) to persist them."
                        .to_owned(),
                ),
            );
        }
    } else if close {
        app.ui_state.kits.save_modal_open = false;
    }
}

/// Applies a verified draft to the local model, so the dashboard shows the
/// state the device just confirmed without a full re-sync.
pub fn apply_draft_to_model(model: &mut ProjectModel, draft: &KitEditDraft) {
    let Some(kit) = model.kits.get_mut(draft.kit_index) else {
        return;
    };
    for edit in &draft.edits {
        if let KitField::KitName = edit.field {
            if let FieldValue::Text(name) = &edit.value {
                kit.name = name.clone();
            }
            continue;
        }
        let Some(sound) = edit.field.track().and_then(|t| kit.sounds.get_mut(t)) else {
            continue;
        };
        let p = &mut sound.params;
        match (&edit.value, edit.field) {
            (FieldValue::Text(name), KitField::SoundName(_)) => sound.name = name.clone(),
            (FieldValue::Int(v), KitField::TrackLevel(_)) => p.level = *v,
            (FieldValue::Int(v), KitField::SampleTune(_)) => p.smpl_tune = *v,
            (FieldValue::Int(v), KitField::SampleFineTune(_)) => p.smpl_fine_tune = *v,
            (FieldValue::Int(v), KitField::SampleBitReduction(_)) => p.smpl_bit_reduction = *v,
            (FieldValue::Float(v), KitField::SampleStart(_)) => p.smpl_start = *v,
            (FieldValue::Float(v), KitField::SampleEnd(_)) => p.smpl_end = *v,
            (FieldValue::Toggle(v), KitField::SampleLoop(_)) => p.smpl_loop = *v,
            (FieldValue::Int(v), KitField::SampleLevel(_)) => p.smpl_level = *v,
            (FieldValue::Int(v), KitField::AmpAttack(_)) => p.amp_attack = *v,
            (FieldValue::Int(v), KitField::AmpHold(_)) => p.amp_hold = *v,
            (FieldValue::Int(v), KitField::AmpDecay(_)) => p.amp_decay = *v,
            (FieldValue::Int(v), KitField::AmpOverdrive(_)) => p.amp_overdrive = *v,
            (FieldValue::Int(v), KitField::AmpDelaySend(_)) => p.amp_delay_send = *v,
            (FieldValue::Int(v), KitField::AmpReverbSend(_)) => p.amp_reverb_send = *v,
            (FieldValue::Int(v), KitField::AmpPan(_)) => p.amp_pan = *v,
            (FieldValue::Int(v), KitField::AmpVolume(_)) => p.amp_volume = *v,
            (FieldValue::Int(v), KitField::FltAttack(_)) => p.flt_attack = *v,
            (FieldValue::Int(v), KitField::FltSustain(_)) => p.flt_sustain = *v,
            (FieldValue::Int(v), KitField::FltDecay(_)) => p.flt_decay = *v,
            (FieldValue::Int(v), KitField::FltRelease(_)) => p.flt_release = *v,
            (FieldValue::Int(v), KitField::FltCutoff(_)) => p.flt_cutoff = *v,
            (FieldValue::Int(v), KitField::FltResonance(_)) => p.flt_resonance = *v,
            (FieldValue::Int(v), KitField::FltEnvAmount(_)) => p.flt_env_amount = *v,
            _ => {}
        }
    }
}

// ---- Field widgets --------------------------------------------------------

/// Integer parameter as a slider seeded from the draft (falling back to the
/// model's current value). Returning to the current value drops the edit.
fn int_field(
    ui: &mut egui::Ui,
    draft: &mut KitEditDraft,
    params: &TrackParams,
    field: KitField,
    label: &str,
) {
    let Some((lo, hi)) = field.int_range() else {
        return;
    };
    let current = match params.value(field) {
        Some(FieldValue::Int(v)) => v,
        _ => 0,
    };
    let mut value = match draft.value(field) {
        Some(FieldValue::Int(v)) => *v,
        _ => current,
    };
    let response = ui.add(egui::Slider::new(&mut value, lo..=hi).text(label));
    if response.changed() {
        if value == current {
            draft.clear_field(field);
        } else {
            draft.push(field, FieldValue::Int(value));
        }
    }
}

/// Sample position (start/end) as a float slider; wire quantization means
/// "back to current" compares within the read-back tolerance.
fn float_field(
    ui: &mut egui::Ui,
    draft: &mut KitEditDraft,
    params: &TrackParams,
    field: KitField,
    label: &str,
) {
    let Some((lo, hi)) = field.float_range() else {
        return;
    };
    let current = match params.value(field) {
        Some(FieldValue::Float(v)) => v,
        _ => 0.0,
    };
    let mut value = match draft.value(field) {
        Some(FieldValue::Float(v)) => *v,
        _ => current,
    };
    let response = ui.add(
        egui::Slider::new(&mut value, lo..=hi)
            .text(label)
            .fixed_decimals(2),
    );
    if response.changed() {
        if (value - current).abs() < crate::model::edit::SAMPLE_POS_TOLERANCE {
            draft.clear_field(field);
        } else {
            draft.push(field, FieldValue::Float(value));
        }
    }
}

fn toggle_field(
    ui: &mut egui::Ui,
    draft: &mut KitEditDraft,
    params: &TrackParams,
    field: KitField,
    label: &str,
) {
    let current = match params.value(field) {
        Some(FieldValue::Toggle(v)) => v,
        _ => false,
    };
    let mut value = match draft.value(field) {
        Some(FieldValue::Toggle(v)) => *v,
        _ => current,
    };
    if ui.checkbox(&mut value, label).changed() {
        if value == current {
            draft.clear_field(field);
        } else {
            draft.push(field, FieldValue::Toggle(value));
        }
    }
}

/// Name text field, constrained to what the device accepts (ASCII, ≤15).
fn name_field(ui: &mut egui::Ui, draft: &mut KitEditDraft, field: KitField, current: &str) {
    let mut text = match draft.value(field) {
        Some(FieldValue::Text(s)) => s.clone(),
        _ => current.to_owned(),
    };
    let response = ui.add(egui::TextEdit::singleline(&mut text).desired_width(140.0));
    if response.changed() {
        text.retain(|c| c.is_ascii() && !c.is_control());
        text.truncate(NAME_MAX_LEN);
        if text == current {
            draft.clear_field(field);
        } else {
            draft.push(field, FieldValue::Text(text));
        }
    }
}
