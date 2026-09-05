//! Plan preview + execution flow: the modal that stands between a drag
//! gesture and the device.

use std::sync::{Arc, Mutex};
use std::thread;

use eframe::egui::{self, Color32, RichText};

use crate::device::detect::is_rytm_port;
use crate::midi::link::MidiLink;
use crate::midi::ports;
use crate::model::SAMPLE_SLOT_COUNT;
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
    /// True once a session backup ran successfully (plans skip re-backup).
    backup_taken: bool,
}

impl ExecutePanel {
    /// Renders the preview modal + execution state. Call every frame.
    pub fn show(&mut self, ctx: &egui::Context, device_in_port: Option<&str>) {
        let state = self.state.lock().expect("exec state lock poisoned").clone();
        match state {
            ExecState::Running { step } => {
                egui::Window::new("Executing")
                    .collapsible(false)
                    .resizable(false)
                    .show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(step);
                        });
                    });
                return;
            }
            ExecState::Done(report) => {
                // The session backup counts only once it actually completed.
                if report.completed.iter().any(|line| line.contains("backup")) {
                    self.backup_taken = true;
                }
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
                        ui.label(format!("✓ {line}"));
                    }
                    if let Some((step, err)) = &report.failed {
                        ui.label(
                            RichText::new(format!("✗ {step}: {err}")).color(Color32::LIGHT_RED),
                        );
                        ui.label(
                            RichText::new(
                                "Nothing after the failed step ran. The session backup \
                                 (if listed above) is on disk.",
                            )
                            .weak(),
                        );
                    }
                    if ui.button("close").clicked() {
                        close = true;
                    }
                });
                if close {
                    *self.state.lock().expect("exec state lock poisoned") = ExecState::Idle;
                }
                return;
            }
            ExecState::Failed(err) => {
                let mut close = false;
                egui::Window::new("Execution failed")
                    .collapsible(false)
                    .show(ctx, |ui| {
                        ui.label(RichText::new(&err).color(Color32::LIGHT_RED));
                        if ui.button("close").clicked() {
                            close = true;
                        }
                    });
                if close {
                    *self.state.lock().expect("exec state lock poisoned") = ExecState::Idle;
                }
                return;
            }
            ExecState::Idle => {}
        }

        let Some(pending) = &mut self.pending else {
            return;
        };
        let mut close = false;
        let mut launch: Option<Plan> = None;
        egui::Window::new("Confirm change")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(format!(
                    "Assign a sample slot to kit {} · track {}",
                    pending.kit_index + 1,
                    pending.track + 1
                ));
                ui.horizontal(|ui| {
                    ui.label("sample slot:");
                    ui.add(egui::DragValue::new(&mut pending.slot).range(1..=SAMPLE_SLOT_COUNT));
                });
                match Plan::assign_sample_slot(
                    pending.kit_index,
                    pending.track,
                    pending.slot,
                    self.backup_taken,
                ) {
                    Ok(plan) => {
                        ui.add_space(4.0);
                        ui.label(RichText::new("This will:").weak());
                        for step in &plan.steps {
                            ui.label(format!("  • {}", step.describe()));
                        }
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            let connected = device_in_port.is_some();
                            if ui
                                .add_enabled(connected, egui::Button::new("▶ Execute on device"))
                                .on_disabled_hover_text("connect the Rytm first")
                                .clicked()
                            {
                                launch = Some(plan);
                            }
                            if ui.button("cancel").clicked() {
                                close = true;
                            }
                        });
                    }
                    Err(e) => {
                        ui.label(RichText::new(e.to_string()).color(Color32::LIGHT_RED));
                    }
                }
            });

        if let (Some(plan), Some(port)) = (launch, device_in_port) {
            self.pending = None;
            self.spawn(plan, port.to_owned(), ctx.clone());
        } else if close {
            self.pending = None;
        }
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
