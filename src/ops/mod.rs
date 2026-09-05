//! Device-mutating operations as transactions.
//!
//! Nothing in the GUI writes to the device directly. A user gesture compiles
//! into a [`Plan`] (ordered steps), the plan is *previewed*, and only then
//! executed against a [`DeviceBackend`] — with a session backup enforced
//! before the first mutating step, and read-back verification after writes.
//! The backend is a trait so the whole pipeline is testable with a mock.

mod backend;
mod live;

pub use backend::{DeviceBackend, OpError};
pub use live::LiveBackend;

use crate::model::SAMPLE_SLOT_COUNT;

/// One step of a transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpStep {
    /// Pull a full object backup to disk (enforced before the first write).
    BackupProject,
    /// Point `kit_index`/`track`'s Sound at project sample slot `slot`
    /// (verified by read-back).
    AssignSampleSlot {
        kit_index: usize,
        track: usize,
        slot: u8,
    },
}

impl OpStep {
    /// One human-readable line for the preview dialog.
    pub fn describe(&self) -> String {
        match self {
            Self::BackupProject => "pull a full project object backup to disk".to_owned(),
            Self::AssignSampleSlot {
                kit_index,
                track,
                slot,
            } => format!(
                "kit {} track {}: set sample slot to {slot}, write the kit, read it back",
                kit_index + 1,
                track + 1
            ),
        }
    }
}

/// An ordered, previewable transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub title: String,
    pub steps: Vec<OpStep>,
}

impl Plan {
    /// A plan that re-points one kit track at a sample slot, prepending the
    /// session backup when one has not been taken yet.
    pub fn assign_sample_slot(
        kit_index: usize,
        track: usize,
        slot: u8,
        backup_taken: bool,
    ) -> Result<Self, OpError> {
        if slot == 0 || slot > SAMPLE_SLOT_COUNT {
            return Err(OpError::InvalidRequest(format!(
                "sample slot must be 1–{SAMPLE_SLOT_COUNT}, got {slot}"
            )));
        }
        let mut steps = Vec::new();
        if !backup_taken {
            steps.push(OpStep::BackupProject);
        }
        steps.push(OpStep::AssignSampleSlot {
            kit_index,
            track,
            slot,
        });
        Ok(Self {
            title: format!(
                "Assign sample slot {slot} to kit {} track {}",
                kit_index + 1,
                track + 1
            ),
            steps,
        })
    }
}

/// What happened when a plan ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionReport {
    /// Descriptions of completed steps, in order.
    pub completed: Vec<String>,
    /// The step that failed and why, if any (nothing after it ran).
    pub failed: Option<(String, String)>,
}

impl ExecutionReport {
    pub fn succeeded(&self) -> bool {
        self.failed.is_none()
    }
}

/// Runs the plan against the backend, stopping at the first failure.
/// `on_step` fires before each step (for progress display).
pub fn execute(
    plan: &Plan,
    backend: &mut dyn DeviceBackend,
    mut on_step: impl FnMut(usize, &OpStep),
) -> ExecutionReport {
    let mut report = ExecutionReport {
        completed: Vec::new(),
        failed: None,
    };
    for (index, step) in plan.steps.iter().enumerate() {
        on_step(index, step);
        let result = match step {
            OpStep::BackupProject => backend.backup_project().map(|_path| ()),
            OpStep::AssignSampleSlot {
                kit_index,
                track,
                slot,
            } => assign_and_verify(backend, *kit_index, *track, *slot),
        };
        match result {
            Ok(()) => report.completed.push(step.describe()),
            Err(e) => {
                report.failed = Some((step.describe(), e.to_string()));
                break;
            }
        }
    }
    report
}

/// Write + read-back verification for a slot assignment.
fn assign_and_verify(
    backend: &mut dyn DeviceBackend,
    kit_index: usize,
    track: usize,
    slot: u8,
) -> Result<(), OpError> {
    backend.write_kit_sample_slot(kit_index, track, slot)?;
    let read_back = backend.read_kit_sample_slot(kit_index, track)?;
    if read_back != Some(slot) {
        return Err(OpError::VerificationFailed(format!(
            "wrote slot {slot} but device reports {read_back:?}"
        )));
    }
    Ok(())
}
