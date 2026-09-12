//! Runs a project sync on a worker thread and publishes its status to the UI.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::device::objects::{sync_active_project, SyncOutcome, SyncProgress};
use crate::midi::link::MidiLink;
use crate::model::ProjectModel;
use crate::{cache, device::detect::is_rytm_port, midi::ports};

/// What the sync worker last reported.
#[derive(Debug, Clone, Default)]
pub enum SyncStatus {
    #[default]
    Idle,
    Running(SyncProgress),
    /// Sync produced a model (already cached); the app takes it via
    /// [`SyncController::take_finished`].
    Finished {
        model: ProjectModel,
        missed: usize,
    },
    Failed(String),
}

/// Handle to the (at most one) running sync worker.
#[derive(Default)]
pub struct SyncController {
    status: Arc<Mutex<SyncStatus>>,
    cancel: Arc<AtomicBool>,
}

impl SyncController {
    pub fn status(&self) -> SyncStatus {
        self.status
            .lock()
            .expect("sync status lock poisoned")
            .clone()
    }

    pub fn is_running(&self) -> bool {
        matches!(self.status(), SyncStatus::Running(_))
    }

    /// Asks a running sync to stop after the current object.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// If a sync finished, returns its model (resetting to idle).
    pub fn take_finished(&self) -> Option<(ProjectModel, usize)> {
        let mut status = self.status.lock().expect("sync status lock poisoned");
        if let SyncStatus::Finished { model, missed } = &*status {
            let result = (model.clone(), *missed);
            *status = SyncStatus::Idle;
            return Some(result);
        }
        None
    }

    /// Starts a sync against `in_port`, unless one is already running.
    /// `on_change` fires on every status update (GUI repaint trigger).
    pub fn start(&self, in_port: String, on_change: impl Fn() + Send + 'static) {
        if self.is_running() {
            return;
        }
        self.cancel.store(false, Ordering::Relaxed);
        *self.status.lock().expect("sync status lock poisoned") =
            SyncStatus::Running(SyncProgress::default());

        let status = Arc::clone(&self.status);
        let cancel = Arc::clone(&self.cancel);
        thread::Builder::new()
            .name("project-sync".into())
            .spawn(move || {
                let result = run_sync(&in_port, &cancel, &status, &on_change);
                *status.lock().expect("sync status lock poisoned") = result;
                on_change();
            })
            .expect("failed to spawn sync thread");
    }
}

fn run_sync(
    in_port: &str,
    cancel: &Arc<AtomicBool>,
    status: &Arc<Mutex<SyncStatus>>,
    on_change: &impl Fn(),
) -> SyncStatus {
    // The Rytm's output port may not share the input port's exact name; pick
    // the first Rytm-looking output.
    let Some(out_port) = ports::snapshot()
        .outputs
        .into_iter()
        .find(|n| is_rytm_port(n))
    else {
        return SyncStatus::Failed("no Rytm MIDI output port found".into());
    };
    let mut link = match MidiLink::open(in_port, &out_port) {
        Ok(link) => link,
        Err(e) => return SyncStatus::Failed(e.to_string()),
    };

    let outcome = sync_active_project(&mut link, cancel, |progress| {
        *status.lock().expect("sync status lock poisoned") = SyncStatus::Running(progress);
        on_change();
    });

    match outcome {
        Ok(SyncOutcome::Complete { mut model, missed }) => {
            // Second phase: sample-slot names + project list over the file
            // protocol. Optional — a device that ignores it leaves the model
            // valid, just without names.
            let _ = crate::device::files::enrich_model(
                &mut link,
                &mut model,
                cancel,
                |stage, done, total| {
                    *status.lock().expect("sync status lock poisoned") =
                        SyncStatus::Running(crate::device::objects::SyncProgress {
                            stage,
                            done,
                            total,
                            missed,
                        });
                    on_change();
                },
            );
            if let Err(e) = cache::save(&model) {
                return SyncStatus::Failed(format!("synced, but cache write failed: {e}"));
            }
            SyncStatus::Finished { model, missed }
        }
        Ok(SyncOutcome::Cancelled) => SyncStatus::Idle,
        Ok(SyncOutcome::DeviceSilent) => {
            SyncStatus::Failed("device stopped answering object queries".into())
        }
        Err(e) => SyncStatus::Failed(e.to_string()),
    }
}
