//! Transaction framework tests against a mock device backend.

use std::collections::HashMap;
use std::path::PathBuf;

use rytmdash::ops::{execute, DeviceBackend, OpError, OpStep, Plan};

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
