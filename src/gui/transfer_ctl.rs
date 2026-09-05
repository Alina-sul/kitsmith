//! Worker that captures a pattern for cross-project transfer (fresh device
//! reads on a background thread).

use std::sync::{Arc, Mutex};
use std::thread;

use crate::device::detect::is_rytm_port;
use crate::midi::link::MidiLink;
use crate::midi::ports;
use crate::model::ProjectModel;
use crate::transfer::{self, PatternClipboard};

#[derive(Default)]
enum CaptureStatus {
    #[default]
    Idle,
    Running,
    Finished(Box<Result<PatternClipboard, String>>),
}

/// Handle to the (at most one) running capture worker.
#[derive(Default)]
pub struct CaptureController {
    status: Arc<Mutex<CaptureStatus>>,
}

impl CaptureController {
    pub fn is_running(&self) -> bool {
        matches!(
            *self.status.lock().expect("capture status lock poisoned"),
            CaptureStatus::Running
        )
    }

    /// If a capture finished, returns its result (resetting to idle).
    pub fn take_finished(&self) -> Option<Result<PatternClipboard, String>> {
        let mut status = self.status.lock().expect("capture status lock poisoned");
        if matches!(*status, CaptureStatus::Finished(_)) {
            if let CaptureStatus::Finished(result) = std::mem::take(&mut *status) {
                return Some(*result);
            }
        }
        None
    }

    /// Starts capturing `pattern_index` over `in_port`.
    pub fn start(
        &self,
        in_port: String,
        model: ProjectModel,
        pattern_index: usize,
        on_change: impl Fn() + Send + 'static,
    ) {
        if self.is_running() {
            return;
        }
        *self.status.lock().expect("capture status lock poisoned") = CaptureStatus::Running;
        let status = Arc::clone(&self.status);
        thread::Builder::new()
            .name("pattern-capture".into())
            .spawn(move || {
                // rytm-rs can panic on malformed/unsupported objects; keep
                // that failure inside this worker instead of poisoning the app.
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run_capture(&in_port, &model, pattern_index)
                }))
                .unwrap_or_else(|_| Err("internal error while decoding device objects".to_owned()));
                if let Ok(clip) = &result {
                    let _ = transfer::save(clip);
                }
                *status.lock().expect("capture status lock poisoned") =
                    CaptureStatus::Finished(Box::new(result));
                on_change();
            })
            .expect("failed to spawn capture thread");
    }
}

fn run_capture(
    in_port: &str,
    model: &ProjectModel,
    pattern_index: usize,
) -> Result<PatternClipboard, String> {
    let out_port = ports::snapshot()
        .outputs
        .into_iter()
        .find(|n| is_rytm_port(n))
        .ok_or("no Rytm MIDI output port found")?;
    let mut link = MidiLink::open(in_port, &out_port).map_err(|e| e.to_string())?;
    transfer::capture(&mut link, model, pattern_index)
}
