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

use crate::model::edit::{FieldValue, KitEdit, KitField};
use crate::model::SAMPLE_SLOT_COUNT;

/// One step of a transaction.
#[derive(Debug, Clone, PartialEq)]
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
    /// Send one pre-encoded object write (kit/pattern/pool sound sysex).
    SendObject { label: String, sysex: Vec<u8> },
    /// Read a kit back and check every drum track's sample slot.
    VerifyKitSlots {
        kit_index: usize,
        expected: Vec<Option<u8>>,
    },
    /// Read a pattern back and check its kit link.
    VerifyPatternKitLink {
        pattern_index: usize,
        expected_kit: usize,
    },
    /// Apply a set of field edits to a freshly read kit and write it back.
    EditKit {
        kit_index: usize,
        edits: Vec<KitEdit>,
    },
    /// Read a kit back and check every listed field against its expected
    /// value (the read-back half of [`OpStep::EditKit`]).
    VerifyKitFields {
        kit_index: usize,
        checks: Vec<KitEdit>,
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
            Self::SendObject { label, .. } => label.clone(),
            Self::VerifyKitSlots { kit_index, .. } => {
                format!(
                    "read kit {:02} back and verify its sample slots",
                    kit_index + 1
                )
            }
            Self::VerifyPatternKitLink { pattern_index, .. } => format!(
                "read pattern {} back and verify its kit link",
                crate::model::pattern_display_name(*pattern_index)
            ),
            Self::EditKit { kit_index, edits } => format!(
                "kit {:02}: apply {} change{} and write the kit",
                kit_index + 1,
                edits.len(),
                if edits.len() == 1 { "" } else { "s" }
            ),
            Self::VerifyKitFields { kit_index, checks } => format!(
                "read kit {:02} back and verify {} edited field{}",
                kit_index + 1,
                checks.len(),
                if checks.len() == 1 { "" } else { "s" }
            ),
        }
    }
}

/// An ordered, previewable transaction.
#[derive(Debug, Clone, PartialEq)]
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

    /// A plan that applies a draft of kit field edits, prepending the session
    /// backup when one has not been taken yet. Every edit is validated here,
    /// before anything touches the device.
    pub fn edit_kit(
        draft: &crate::model::edit::KitEditDraft,
        backup_taken: bool,
    ) -> Result<Self, OpError> {
        if draft.edits.is_empty() {
            return Err(OpError::InvalidRequest("no edits to apply".into()));
        }
        for edit in &draft.edits {
            edit.validate().map_err(OpError::InvalidRequest)?;
        }
        let mut steps = Vec::new();
        if !backup_taken {
            steps.push(OpStep::BackupProject);
        }
        steps.push(OpStep::EditKit {
            kit_index: draft.kit_index,
            edits: draft.edits.clone(),
        });
        steps.push(OpStep::VerifyKitFields {
            kit_index: draft.kit_index,
            checks: draft.edits.clone(),
        });
        Ok(Self {
            title: format!("Save changes to kit {:02}", draft.kit_index + 1),
            steps,
        })
    }

    /// A plan that writes a transferred kit into the active project.
    /// Always backs up first (the active project changed since any earlier
    /// session backup — that is the point of a cross-project paste).
    pub fn paste_kit(paste: &crate::transfer::MaterializedKitPaste) -> Self {
        Self {
            title: format!("Copy kit {:02} into this project", paste.kit_index + 1),
            steps: vec![
                OpStep::BackupProject,
                OpStep::SendObject {
                    label: paste.write.label.clone(),
                    sysex: paste.write.sysex.clone(),
                },
                OpStep::VerifyKitSlots {
                    kit_index: paste.kit_index,
                    expected: paste.expected_kit_slots.clone(),
                },
            ],
        }
    }

    /// A plan that writes a transferred pattern (+ kit + pool sounds) into
    /// the active project. Always backs up first — the active project has
    /// changed since any earlier session backup (that is the whole point of
    /// a cross-project paste).
    pub fn paste_pattern(paste: &crate::transfer::MaterializedPaste) -> Self {
        let mut steps = vec![OpStep::BackupProject];
        for write in &paste.writes {
            steps.push(OpStep::SendObject {
                label: write.label.clone(),
                sysex: write.sysex.clone(),
            });
        }
        steps.push(OpStep::VerifyKitSlots {
            kit_index: paste.kit_index,
            expected: paste.expected_kit_slots.clone(),
        });
        steps.push(OpStep::VerifyPatternKitLink {
            pattern_index: paste.pattern_index,
            expected_kit: paste.kit_index,
        });
        Self {
            title: format!(
                "Copy pattern {} into this project",
                crate::model::pattern_display_name(paste.pattern_index)
            ),
            steps,
        }
    }
}

/// What happened when a plan ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionReport {
    /// Descriptions of completed steps, in order.
    pub completed: Vec<String>,
    /// The step that failed and why, if any (nothing after it ran).
    pub failed: Option<(String, String)>,
    /// Whether a [`OpStep::BackupProject`] step ran to completion, so the
    /// caller can skip re-backing-up without parsing `completed` prose.
    pub backed_up: bool,
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
        backed_up: false,
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
            OpStep::SendObject { sysex, .. } => backend.send_object(sysex),
            OpStep::VerifyKitSlots {
                kit_index,
                expected,
            } => verify_kit_slots(backend, *kit_index, expected),
            OpStep::VerifyPatternKitLink {
                pattern_index,
                expected_kit,
            } => verify_pattern_kit_link(backend, *pattern_index, *expected_kit),
            OpStep::EditKit { kit_index, edits } => backend.apply_kit_edits(*kit_index, edits),
            OpStep::VerifyKitFields { kit_index, checks } => {
                verify_kit_fields(backend, *kit_index, checks)
            }
        };
        match result {
            Ok(()) => {
                if matches!(step, OpStep::BackupProject) {
                    report.backed_up = true;
                }
                report.completed.push(step.describe());
            }
            Err(e) => {
                report.failed = Some((step.describe(), e.to_string()));
                break;
            }
        }
    }
    report
}

/// Read-back verification of edited kit fields: every check's field is
/// re-read from the device and compared against the value that was written.
fn verify_kit_fields(
    backend: &mut dyn DeviceBackend,
    kit_index: usize,
    checks: &[KitEdit],
) -> Result<(), OpError> {
    let fields: Vec<KitField> = checks.iter().map(|c| c.field).collect();
    let read_back: Vec<FieldValue> = backend.read_kit_fields(kit_index, &fields)?;
    if read_back.len() != checks.len() {
        return Err(OpError::VerificationFailed(format!(
            "asked for {} fields, device read returned {}",
            checks.len(),
            read_back.len()
        )));
    }
    for (check, read) in checks.iter().zip(&read_back) {
        if !check.value_matches(read) {
            return Err(OpError::VerificationFailed(format!(
                "kit {:02} {}: wrote {}, device reports {read}",
                kit_index + 1,
                check.field.label(),
                check.value
            )));
        }
    }
    Ok(())
}

/// Read-back verification of a written kit's sample slots.
fn verify_kit_slots(
    backend: &mut dyn DeviceBackend,
    kit_index: usize,
    expected: &[Option<u8>],
) -> Result<(), OpError> {
    let read_back = backend.read_kit_sample_slots(kit_index)?;
    if read_back != expected {
        return Err(OpError::VerificationFailed(format!(
            "kit {:02} sample slots read back as {read_back:?}, expected {expected:?}",
            kit_index + 1
        )));
    }
    Ok(())
}

/// Read-back verification of a written pattern's kit link.
fn verify_pattern_kit_link(
    backend: &mut dyn DeviceBackend,
    pattern_index: usize,
    expected_kit: usize,
) -> Result<(), OpError> {
    let read_back = backend.read_pattern_kit_link(pattern_index)?;
    if read_back != expected_kit {
        return Err(OpError::VerificationFailed(format!(
            "pattern {} links to kit {read_back}, expected {expected_kit}",
            crate::model::pattern_display_name(pattern_index)
        )));
    }
    Ok(())
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
