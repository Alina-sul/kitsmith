//! Transaction framework tests against a mock device backend.

use std::collections::HashMap;
use std::path::PathBuf;

use kitsmith::model::edit::{FieldValue, KitEdit, KitEditDraft, KitField};
use kitsmith::ops::{execute, DeviceBackend, OpError, OpStep, Plan};

/// In-memory device: records calls, serves configurable slot state.
#[derive(Default)]
struct MockBackend {
    calls: Vec<String>,
    slots: HashMap<(usize, usize), u8>,
    fail_backup: bool,
    /// When set, reads report this instead of what was written.
    lie_on_read: Option<u8>,
    /// Slots reported by `read_kit_sample_slots` (paste verification).
    kit_slots: Vec<Option<u8>>,
    /// Kit link reported by `read_pattern_kit_link`.
    pattern_kit_link: usize,
    /// Field edits applied via `apply_kit_edits`, echoed by `read_kit_fields`.
    applied_edits: Vec<KitEdit>,
    /// When set, every field read reports this instead of what was written.
    lie_on_field_read: Option<FieldValue>,
}

impl DeviceBackend for MockBackend {
    fn backup_project(&mut self) -> Result<PathBuf, OpError> {
        self.calls.push("backup".into());
        if self.fail_backup {
            return Err(OpError::Device("device silent".into()));
        }
        Ok(PathBuf::from("/tmp/mock-backup.syx"))
    }

    fn write_kit_sample_slot(
        &mut self,
        kit_index: usize,
        track: usize,
        slot: u8,
    ) -> Result<(), OpError> {
        self.calls.push(format!("write {kit_index}/{track}={slot}"));
        self.slots.insert((kit_index, track), slot);
        Ok(())
    }

    fn read_kit_sample_slot(
        &mut self,
        kit_index: usize,
        track: usize,
    ) -> Result<Option<u8>, OpError> {
        self.calls.push(format!("read {kit_index}/{track}"));
        if let Some(lie) = self.lie_on_read {
            return Ok(Some(lie));
        }
        Ok(self.slots.get(&(kit_index, track)).copied())
    }

    fn send_object(&mut self, sysex: &[u8]) -> Result<(), OpError> {
        self.calls.push(format!("send {} bytes", sysex.len()));
        Ok(())
    }

    fn read_kit_sample_slots(&mut self, kit_index: usize) -> Result<Vec<Option<u8>>, OpError> {
        self.calls.push(format!("read-kit {kit_index}"));
        Ok(self.kit_slots.clone())
    }

    fn read_pattern_kit_link(&mut self, pattern_index: usize) -> Result<usize, OpError> {
        self.calls.push(format!("read-pattern {pattern_index}"));
        Ok(self.pattern_kit_link)
    }

    fn apply_kit_edits(&mut self, kit_index: usize, edits: &[KitEdit]) -> Result<(), OpError> {
        self.calls
            .push(format!("edit-kit {kit_index} ({} edits)", edits.len()));
        self.applied_edits.extend_from_slice(edits);
        Ok(())
    }

    fn read_kit_fields(
        &mut self,
        kit_index: usize,
        fields: &[KitField],
    ) -> Result<Vec<FieldValue>, OpError> {
        self.calls.push(format!("read-fields {kit_index}"));
        fields
            .iter()
            .map(|field| {
                if let Some(lie) = &self.lie_on_field_read {
                    return Ok(lie.clone());
                }
                self.applied_edits
                    .iter()
                    .rev()
                    .find(|e| e.field == *field)
                    .map(|e| e.value.clone())
                    .ok_or_else(|| OpError::Device("field never written".into()))
            })
            .collect()
    }
}

#[test]
fn first_write_plan_includes_backup_then_assignment() {
    let plan = Plan::assign_sample_slot(4, 2, 17, false).unwrap();
    assert_eq!(plan.steps.len(), 2);
    assert_eq!(plan.steps[0], OpStep::BackupProject);
    // With a session backup already taken the plan skips it.
    let plan2 = Plan::assign_sample_slot(4, 2, 17, true).unwrap();
    assert_eq!(plan2.steps.len(), 1);
}

#[test]
fn rejects_out_of_range_slots() {
    assert!(Plan::assign_sample_slot(0, 0, 0, true).is_err());
    assert!(Plan::assign_sample_slot(0, 0, 128, true).is_err());
    assert!(Plan::assign_sample_slot(0, 0, 127, true).is_ok());
}

#[test]
fn executes_backup_write_verify_in_order() {
    let plan = Plan::assign_sample_slot(4, 2, 17, false).unwrap();
    let mut backend = MockBackend::default();
    let mut seen_steps = Vec::new();
    let report = execute(&plan, &mut backend, |i, step| {
        seen_steps.push((i, step.describe()));
    });
    assert!(report.succeeded());
    assert_eq!(report.completed.len(), 2);
    assert_eq!(
        backend.calls,
        vec!["backup", "write 4/2=17", "read 4/2"],
        "steps must run backup → write → read-back"
    );
    assert_eq!(seen_steps.len(), 2);
    assert_eq!(backend.slots[&(4, 2)], 17);
}

#[test]
fn stops_at_failed_backup_without_writing() {
    let plan = Plan::assign_sample_slot(4, 2, 17, false).unwrap();
    let mut backend = MockBackend {
        fail_backup: true,
        ..Default::default()
    };
    let report = execute(&plan, &mut backend, |_, _| {});
    assert!(!report.succeeded());
    assert!(report.completed.is_empty());
    assert_eq!(
        backend.calls,
        vec!["backup"],
        "no write after failed backup"
    );
    let (step, err) = report.failed.unwrap();
    assert!(step.contains("backup"));
    assert!(err.contains("device silent"));
}

#[test]
fn verification_mismatch_fails_the_step() {
    let plan = Plan::assign_sample_slot(1, 1, 10, true).unwrap();
    let mut backend = MockBackend {
        lie_on_read: Some(99),
        ..Default::default()
    };
    let report = execute(&plan, &mut backend, |_, _| {});
    assert!(!report.succeeded());
    let (_, err) = report.failed.unwrap();
    assert!(err.contains("verification failed"), "got: {err}");
    assert!(err.contains("99"));
}

#[test]
fn report_flags_the_backup_structurally() {
    // The GUI skips a re-backup on this flag; it must come from the step that
    // ran, not from prose in `completed` that a reword would silently break.
    let mut backend = MockBackend::default();
    let plan = Plan::assign_sample_slot(0, 1, 4, false).expect("valid plan");
    let report = execute(&plan, &mut backend, |_, _| {});
    assert!(report.succeeded());
    assert!(report.backed_up);

    let mut backend = MockBackend::default();
    let plan = Plan::assign_sample_slot(0, 1, 4, true).expect("valid plan");
    let report = execute(&plan, &mut backend, |_, _| {});
    assert!(report.succeeded());
    assert!(!report.backed_up, "no backup step ran");
}

fn sample_draft() -> KitEditDraft {
    let mut draft = KitEditDraft::new(3);
    draft.push(KitField::KitName, FieldValue::Text("MY KIT".into()));
    draft.push(KitField::AmpDecay(0), FieldValue::Int(64));
    draft.push(KitField::SampleTune(5), FieldValue::Int(-12));
    draft
}

#[test]
fn edit_kit_plan_backs_up_edits_then_verifies() {
    let plan = Plan::edit_kit(&sample_draft(), false).unwrap();
    assert_eq!(plan.steps.len(), 3);
    assert_eq!(plan.steps[0], OpStep::BackupProject);
    assert!(matches!(
        plan.steps[1],
        OpStep::EditKit { kit_index: 3, .. }
    ));
    assert!(matches!(
        plan.steps[2],
        OpStep::VerifyKitFields { kit_index: 3, .. }
    ));
    // With a session backup taken the plan skips the backup step only.
    let plan2 = Plan::edit_kit(&sample_draft(), true).unwrap();
    assert_eq!(plan2.steps.len(), 2);
}

#[test]
fn edit_kit_plan_rejects_empty_and_invalid_drafts() {
    assert!(Plan::edit_kit(&KitEditDraft::new(0), true).is_err());

    let mut long_name = KitEditDraft::new(0);
    long_name.push(
        KitField::KitName,
        FieldValue::Text("SIXTEEN CHARSXXX".into()),
    );
    assert!(Plan::edit_kit(&long_name, true).is_err());

    let mut out_of_range = KitEditDraft::new(0);
    out_of_range.push(KitField::AmpDecay(0), FieldValue::Int(200));
    assert!(Plan::edit_kit(&out_of_range, true).is_err());

    let mut wrong_type = KitEditDraft::new(0);
    wrong_type.push(KitField::AmpDecay(0), FieldValue::Toggle(true));
    assert!(Plan::edit_kit(&wrong_type, true).is_err());
}

#[test]
fn edit_kit_executes_and_verifies_every_field() {
    let plan = Plan::edit_kit(&sample_draft(), true).unwrap();
    let mut backend = MockBackend::default();
    let report = execute(&plan, &mut backend, |_, _| {});
    assert!(report.succeeded(), "failed: {:?}", report.failed);
    assert_eq!(backend.calls, vec!["edit-kit 3 (3 edits)", "read-fields 3"]);
    assert_eq!(backend.applied_edits.len(), 3);
}

#[test]
fn edit_kit_verification_mismatch_fails_with_field_name() {
    let plan = Plan::edit_kit(&sample_draft(), true).unwrap();
    let mut backend = MockBackend {
        lie_on_field_read: Some(FieldValue::Text("MY KIT".into())),
        ..MockBackend::default()
    };
    let report = execute(&plan, &mut backend, |_, _| {});
    assert!(!report.succeeded());
    let (_, err) = report.failed.unwrap();
    assert!(err.contains("verification failed"), "got: {err}");
    // The kit name happens to match the lie; the first mismatching field
    // (AMP decay) must be named.
    assert!(err.contains("AMP decay"), "got: {err}");
}

#[test]
fn draft_dedupes_by_field_and_clears() {
    let mut draft = KitEditDraft::new(0);
    draft.push(KitField::AmpDecay(2), FieldValue::Int(10));
    draft.push(KitField::AmpDecay(2), FieldValue::Int(90));
    assert_eq!(draft.edits.len(), 1, "same field must replace, not append");
    assert_eq!(
        draft.value(KitField::AmpDecay(2)),
        Some(&FieldValue::Int(90))
    );
    draft.push(KitField::AmpDecay(3), FieldValue::Int(90));
    assert_eq!(draft.edits.len(), 2, "same param on another track is new");
    draft.clear_field(KitField::AmpDecay(2));
    assert_eq!(draft.value(KitField::AmpDecay(2)), None);
    assert_eq!(draft.edits.len(), 1);
}

#[test]
fn edit_preview_lines_are_human_readable() {
    let draft = sample_draft();
    let lines: Vec<String> = draft.edits.iter().map(KitEdit::describe).collect();
    assert_eq!(lines[0], "kit name → \"MY KIT\"");
    assert_eq!(lines[1], "BD · AMP decay → 64", "got: {}", lines[1]);
    assert_eq!(lines[2], "LT · SMPL tune → -12", "got: {}", lines[2]);
}

#[test]
fn float_verification_tolerates_wire_quantization() {
    let edit = KitEdit {
        field: KitField::SampleStart(0),
        value: FieldValue::Float(30.0),
    };
    // One u16 wire step is 120/30720 ≈ 0.0039.
    assert!(edit.value_matches(&FieldValue::Float(30.0039)));
    assert!(!edit.value_matches(&FieldValue::Float(30.02)));
}

#[test]
fn failed_backup_is_not_reported_as_taken() {
    let mut backend = MockBackend {
        fail_backup: true,
        ..MockBackend::default()
    };
    let plan = Plan::assign_sample_slot(0, 1, 4, false).expect("valid plan");
    let report = execute(&plan, &mut backend, |_, _| {});
    assert!(!report.succeeded());
    assert!(!report.backed_up);
}
