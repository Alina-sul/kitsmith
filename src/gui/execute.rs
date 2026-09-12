//! Plan preview + execution flow: the modal that stands between a drag
//! gesture and the device. Titles name the user's intent, protocol steps live
//! in a collapsible section (PRD §37), and completion reports what ran.

use std::sync::{Arc, Mutex};
use std::thread;

use eframe::egui::{self, RichText};

use crate::device::detect::is_rytm_port;
use crate::gui::theme;
use crate::midi::link::MidiLink;
use crate::midi::ports;
use crate::model::names::{self, ResolvedName};
use crate::model::{ProjectModel, SAMPLE_SLOT_COUNT};
use crate::ops::{execute, ExecutionReport, LiveBackend, Plan};

/// A drag that landed and awaits confirmation.
pub struct PendingAssign {
    pub kit_index: usize,
    pub track: usize,
    pub slot: u8,
}

/// Progress/result of the (single) running execution.
#[derive(Clone, Default)]
pub enum ExecState {
    #[default]
    Idle,
    Running {
        step: String,
    },
    Done(ExecutionReport),
    Failed(String),
}

/// Owns the pending plan and the execution worker.
#[derive(Default)]
pub struct ExecutePanel {
    pub pending: Option<PendingAssign>,
    state: Arc<Mutex<ExecState>>,
    /// True once a backup ran successfully for the device state we are still
    /// looking at (plans skip re-backup). Cleared by [`Self::invalidate_backup`]
    /// as soon as that assumption can no longer be trusted.
    backup_taken: bool,
    /// Extra guidance shown in the completion dialog of a launched plan.
    post_note: Option<String>,
}

impl ExecutePanel {
    /// Starts executing an externally built plan (e.g. a pattern paste).
    pub fn launch(
        &mut self,
        plan: Plan,
        in_port: String,
        ctx: egui::Context,
        post_note: Option<String>,
    ) {
        self.post_note = post_note;
        self.spawn(plan, in_port, ctx);
    }

    /// Forgets the session backup, so the next plan pulls a fresh one.
    ///
    /// The Rytm's active project can be changed on the hardware at any time,
    /// and a backup of the *previous* project is no protection for a write to
    /// the current one. Anything that means "the device may not be what it
    /// was" — a re-sync, a reconnect — must call this, per the project's
    /// never-write-without-a-fresh-backup rule.
    pub fn invalidate_backup(&mut self) {
        self.backup_taken = false;
    }

    /// Renders the preview modal + execution state. Call every frame.
    /// Returns the report once, when a finished execution is dismissed.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        device_in_port: Option<&str>,
        model: Option<&ProjectModel>,
    ) -> Option<ExecutionReport> {
        let state = self.state.lock().expect("exec state lock poisoned").clone();
        match state {
            ExecState::Running { step } => {
                egui::Window::new("Working…")
                    .collapsible(false)
                    .resizable(false)
                    .show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(step);
                        });
                    });
                return None;
            }
            ExecState::Done(report) => {
                // The session backup counts only once it actually completed.
                self.backup_taken |= report.backed_up;
                let mut close = false;
                egui::Window::new(if report.succeeded() {
                    "Done"
                } else {
                    "Stopped"
                })
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    for line in &report.completed {
                        ui.label(format!("✔ {line}"));
                    }
                    if let Some((step, err)) = &report.failed {
                        ui.label(RichText::new(format!("✖ {step}: {err}")).color(theme::DANGER));
                        ui.label(
                            RichText::new(
                                "Nothing after the failed step ran. The session backup \
                                 (if listed above) is on disk.",
                            )
                            .color(theme::TEXT_SECONDARY),
                        );
                    }
                    if report.succeeded() {
                        if let Some(note) = &self.post_note {
                            ui.add_space(4.0);
                            ui.label(RichText::new(note).color(theme::WARNING));
                        }
                    }
                    if ui.button("Close").clicked() {
                        close = true;
                    }
                });
                if close {
                    *self.state.lock().expect("exec state lock poisoned") = ExecState::Idle;
                    self.post_note = None;
                    return Some(report);
                }
                return None;
            }
            ExecState::Failed(err) => {
                let mut close = false;
                egui::Window::new("Execution failed")
                    .collapsible(false)
                    .show(ctx, |ui| {
                        ui.label(RichText::new(&err).color(theme::DANGER));
                        if ui.button("Close").clicked() {
                            close = true;
                        }
                    });
                if close {
                    *self.state.lock().expect("exec state lock poisoned") = ExecState::Idle;
                }
                return None;
            }
            ExecState::Idle => {}
        }

        let Some(pending) = &mut self.pending else {
            return None;
        };
        let mut close = false;
        let mut launch: Option<Plan> = None;
        let track = names::track_label(pending.track);
        let sample = sample_label(model, pending.slot);
        egui::Window::new(format!("Assign {sample} to {track}?"))
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(format!(
                    "Kit {:02} · point the {track} track Sound at project sample slot:",
                    pending.kit_index + 1,
                ));
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut pending.slot).range(1..=SAMPLE_SLOT_COUNT));
                    ui.label(
                        RichText::new("advanced override — the drop already chose this slot")
                            .color(theme::TEXT_MUTED)
                            .size(theme::FONT_META),
                    );
                });
                match Plan::assign_sample_slot(
                    pending.kit_index,
                    pending.track,
                    pending.slot,
                    self.backup_taken,
                ) {
                    Ok(plan) => {
                        ui.add_space(4.0);
                        if !self.backup_taken {
                            ui.label(
                                RichText::new("A backup will be created before writing.")
                                    .color(theme::TEXT_SECONDARY),
                            );
                        }
                        ui.label(
                            RichText::new("The write is verified by reading the kit back.")
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
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            let connected = device_in_port.is_some();
                            if ui
                                .add_enabled(connected, egui::Button::new("Assign sample"))
                                .on_disabled_hover_text("Connect the Rytm first")
                                .clicked()
                            {
                                launch = Some(plan);
                            }
                            if ui.button("Cancel").clicked() {
                                close = true;
                            }
                        });
                    }
                    Err(e) => {
                        ui.label(RichText::new(e.to_string()).color(theme::DANGER));
                    }
                }
            });

        if let (Some(plan), Some(port)) = (launch, device_in_port) {
            self.pending = None;
            self.spawn(plan, port.to_owned(), ctx.clone());
        } else if close {
            self.pending = None;
        }
        None
    }

    fn spawn(&self, plan: Plan, in_port: String, repaint: egui::Context) {
        let state = Arc::clone(&self.state);
        *state.lock().expect("exec state lock poisoned") = ExecState::Running {
            step: "opening MIDI link".into(),
        };
        thread::Builder::new()
            .name("plan-exec".into())
            .spawn(move || {
                let result = run(&plan, &in_port, &state, &repaint);
                *state.lock().expect("exec state lock poisoned") = result;
                repaint.request_repaint();
            })
            .expect("failed to spawn execution thread");
    }
}

/// `Crash.wav`, or `sample slot 03` when the name is not resolved.
fn sample_label(model: Option<&ProjectModel>, slot: u8) -> String {
    match names::sample_display_name(model.and_then(|m| m.sample_name(slot))) {
        ResolvedName::Native(name) => name,
        _ => format!("sample slot {slot:02}"),
    }
}

fn run(
    plan: &Plan,
    in_port: &str,
    state: &Arc<Mutex<ExecState>>,
    repaint: &egui::Context,
) -> ExecState {
    let Some(out_port) = ports::snapshot()
        .outputs
        .into_iter()
        .find(|n| is_rytm_port(n))
    else {
        return ExecState::Failed("no Rytm MIDI output port found".into());
    };
    let link = match MidiLink::open(in_port, &out_port) {
        Ok(link) => link,
        Err(e) => return ExecState::Failed(e.to_string()),
    };
    let mut backend = match LiveBackend::new(link) {
        Ok(backend) => backend,
        Err(e) => return ExecState::Failed(e.to_string()),
    };
    let total = plan.steps.len();
    let report = execute(plan, &mut backend, |index, step| {
        *state.lock().expect("exec state lock poisoned") = ExecState::Running {
            step: format!("step {}/{}: {}", index + 1, total, step.describe()),
        };
        repaint.request_repaint();
    });
    ExecState::Done(report)
}
