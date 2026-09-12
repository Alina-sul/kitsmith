//! Transaction framework tests against a mock device backend.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use kitsmith::analysis::{DriveRefScan, ProjectHit};
use kitsmith::device::files::{DriveEntry, RamSlot};
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
    /// In-memory +Drive: directory path → its entries.
    drive: BTreeMap<String, Vec<DriveEntry>>,
    /// Active-project RAM slot table served by `ram_sample_slots`.
    ram: Vec<RamSlot>,
    /// When true, +Drive writes are acknowledged but not applied — the
    /// verify listing then disagrees.
    ignore_drive_writes: bool,
}

impl MockBackend {
    /// Applies one +Drive mutation to the in-memory tree.
    fn apply_drive(&mut self, action: DriveAction) {
        if self.ignore_drive_writes {
            return;
        }
        match action {
            DriveAction::Rename { src, dst } => {
                let (src_dir, src_name) = split(&src);
                let (dst_dir, dst_name) = split(&dst);
                let Some(mut entry) = self.drive.get_mut(&src_dir).and_then(|list| {
                    let i = list.iter().position(|e| e.name == src_name)?;
                    Some(list.remove(i))
                }) else {
                    return;
                };
                entry.name = dst_name;
                self.drive.entry(dst_dir).or_default().push(entry);
            }
            DriveAction::CreateDir { path } => {
                let (parent, name) = split(&path);
                self.drive.entry(parent).or_default().push(dir_entry(&name));
                self.drive.entry(path).or_default();
            }
            DriveAction::Delete { path } => {
                let (parent, name) = split(&path);
                if let Some(list) = self.drive.get_mut(&parent) {
                    list.retain(|e| e.name != name);
                }
                self.drive.remove(&path);
            }
        }
    }
}

enum DriveAction {
    Rename { src: String, dst: String },
    CreateDir { path: String },
    Delete { path: String },
}

fn split(path: &str) -> (String, String) {
    match path.rsplit_once('/') {
        Some(("", name)) => ("/".to_owned(), name.to_owned()),
        Some((dir, name)) => (dir.to_owned(), name.to_owned()),
        None => ("/".to_owned(), path.to_owned()),
    }
}

fn file_entry(name: &str, hash: u32, size: u32) -> DriveEntry {
    DriveEntry {
        name: name.to_owned(),
        size,
        hash,
        is_dir: false,
        write_protected: false,
    }
}

fn dir_entry(name: &str) -> DriveEntry {
    DriveEntry {
        name: name.to_owned(),
        size: 0,
        hash: 0,
        is_dir: true,
        write_protected: false,
    }
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

    fn list_sample_dir(&mut self, path: &str) -> Result<Vec<DriveEntry>, OpError> {
        self.calls.push(format!("list {path}"));
        Ok(self.drive.get(path).cloned().unwrap_or_default())
    }

    fn rename_sample_path(&mut self, src: &str, dst: &str) -> Result<(), OpError> {
        self.calls.push(format!("rename {src} -> {dst}"));
        self.apply_drive(DriveAction::Rename {
            src: src.to_owned(),
            dst: dst.to_owned(),
        });
        Ok(())
    }

    fn create_sample_dir(&mut self, path: &str) -> Result<(), OpError> {
        self.calls.push(format!("mkdir {path}"));
        self.apply_drive(DriveAction::CreateDir {
            path: path.to_owned(),
        });
        Ok(())
    }

    fn delete_sample_dir(&mut self, path: &str) -> Result<(), OpError> {
        self.calls.push(format!("rmdir {path}"));
        self.apply_drive(DriveAction::Delete {
            path: path.to_owned(),
        });
        Ok(())
    }

    fn delete_sample_file(&mut self, path: &str) -> Result<(), OpError> {
        self.calls.push(format!("delete {path}"));
        self.apply_drive(DriveAction::Delete {
            path: path.to_owned(),
        });
        Ok(())
    }

    fn ram_sample_slots(&mut self) -> Result<Vec<RamSlot>, OpError> {
        self.calls.push("ram-slots".into());
        Ok(self.ram.clone())
    }

    fn download_sample_to(
        &mut self,
        path: &str,
        dest: &Path,
        _hash: u32,
        _size: u32,
    ) -> Result<PathBuf, OpError> {
        self.calls.push(format!("download {path}"));
        Ok(dest.to_path_buf())
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

// ---- +Drive file management -----------------------------------------------

/// A small +Drive: /loops (K1, K2), /oneshots (empty), /factory (protected).
fn drive_backend() -> MockBackend {
    let mut backend = MockBackend::default();
    backend.drive.insert(
        "/".to_owned(),
        vec![
            dir_entry("loops"),
            dir_entry("oneshots"),
            DriveEntry {
                write_protected: true,
                ..dir_entry("factory")
            },
        ],
    );
    backend.drive.insert(
        "/loops".to_owned(),
        vec![file_entry("K1", 0xAA, 100), file_entry("K2", 0xBB, 200)],
    );
    backend.drive.insert("/oneshots".to_owned(), Vec::new());
    backend
}

fn no_refs() -> DriveRefScan {
    DriveRefScan::default()
}

#[test]
fn rename_plan_renames_and_verifies_both_names() {
    let entry = file_entry("K1", 0xAA, 100);
    let plan = Plan::rename_drive_entry("/loops", &entry, "KICK", true).expect("plan");
    let mut backend = drive_backend();
    let report = execute(&plan, &mut backend, |_, _| {});
    assert!(report.succeeded(), "failed: {:?}", report.failed);
    let names: Vec<&str> = backend.drive["/loops"]
        .iter()
        .map(|e| e.name.as_str())
        .collect();
    assert!(names.contains(&"KICK") && !names.contains(&"K1"));
    // Renamed entry keeps its content identity.
    let renamed = backend.drive["/loops"]
        .iter()
        .find(|e| e.name == "KICK")
        .unwrap();
    assert_eq!((renamed.hash, renamed.size), (0xAA, 100));
}

#[test]
fn move_plan_relocates_across_directories() {
    let entry = file_entry("K1", 0xAA, 100);
    let plan = Plan::move_drive_entry("/loops", &entry, "/oneshots", true).expect("plan");
    let mut backend = drive_backend();
    let report = execute(&plan, &mut backend, |_, _| {});
    assert!(report.succeeded(), "failed: {:?}", report.failed);
    assert!(backend.drive["/loops"].iter().all(|e| e.name != "K1"));
    assert!(backend.drive["/oneshots"].iter().any(|e| e.name == "K1"));
}

#[test]
fn drive_write_that_does_not_land_fails_verification() {
    let entry = file_entry("K1", 0xAA, 100);
    let plan = Plan::rename_drive_entry("/loops", &entry, "KICK", true).expect("plan");
    let mut backend = drive_backend();
    backend.ignore_drive_writes = true;
    let report = execute(&plan, &mut backend, |_, _| {});
    assert!(!report.succeeded());
    let (_, err) = report.failed.unwrap();
    assert!(err.contains("verification failed"), "got: {err}");
}

#[test]
fn stale_listing_stops_the_rename_before_writing() {
    // The plan was built from a stale listing (it believes K1 has hash
    // 0xDEAD); the device reports different content under that name.
    let stale = file_entry("K1", 0xDEAD, 1);
    let plan = Plan::rename_drive_entry("/loops", &stale, "KICK", true).expect("plan");
    let mut backend = drive_backend();
    let report = execute(&plan, &mut backend, |_, _| {});
    assert!(!report.succeeded());
    assert!(
        !backend.calls.iter().any(|c| c.starts_with("rename")),
        "no write after a failed preflight: {:?}",
        backend.calls
    );
}

#[test]
fn rename_to_an_existing_name_is_refused_at_execute_time() {
    let entry = file_entry("K1", 0xAA, 100);
    let plan = Plan::rename_drive_entry("/loops", &entry, "K2", true).expect("plan");
    let mut backend = drive_backend();
    let report = execute(&plan, &mut backend, |_, _| {});
    assert!(!report.succeeded());
    assert!(!backend.calls.iter().any(|c| c.starts_with("rename")));
}

#[test]
fn rename_onto_a_case_variant_is_refused() {
    // Device FS case semantics are unresolved — a case-variant of another
    // entry is conservatively a collision…
    let entry = file_entry("K1", 0xAA, 100);
    let plan = Plan::rename_drive_entry("/loops", &entry, "k2", true).expect("plan");
    let mut backend = drive_backend();
    let report = execute(&plan, &mut backend, |_, _| {});
    assert!(!report.succeeded());
    assert!(!backend.calls.iter().any(|c| c.starts_with("rename")));
    // …but a pure case-change of the entry's own name is not self-colliding.
    let plan = Plan::rename_drive_entry("/loops", &entry, "k1", true).expect("plan");
    let mut backend = drive_backend();
    let report = execute(&plan, &mut backend, |_, _| {});
    assert!(report.succeeded(), "failed: {:?}", report.failed);
}

#[test]
fn move_destination_must_be_a_well_formed_absolute_path() {
    let entry = file_entry("K1", 0xAA, 100);
    for bad in ["oneshots", "/oneshots/", "//oneshots", ""] {
        assert!(
            Plan::move_drive_entry("/loops", &entry, bad, true).is_err(),
            "{bad:?} must be refused"
        );
    }
}

#[test]
fn factory_and_bad_names_are_refused_at_plan_time() {
    let entry = file_entry("K1", 0xAA, 100);
    assert!(Plan::rename_drive_entry("/factory", &entry, "X", true).is_err());
    assert!(Plan::move_drive_entry("/loops", &entry, "/factory", true).is_err());
    assert!(Plan::create_drive_dir("/factory", "sub", true).is_err());
    let protected = DriveEntry {
        write_protected: true,
        ..file_entry("K1", 0xAA, 100)
    };
    assert!(Plan::rename_drive_entry("/loops", &protected, "X", true).is_err());
    // Name validation.
    assert!(Plan::rename_drive_entry("/loops", &entry, "", true).is_err());
    assert!(Plan::rename_drive_entry("/loops", &entry, "a/b", true).is_err());
    assert!(Plan::rename_drive_entry("/loops", &entry, "K1", true).is_err());
    assert!(Plan::create_drive_dir("/", &"x".repeat(33), true).is_err());
    // Moving a folder into itself.
    let folder = dir_entry("loops");
    assert!(Plan::move_drive_entry("/", &folder, "/loops", true).is_err());
    assert!(Plan::move_drive_entry("/", &folder, "/loops/sub", true).is_err());
}

#[test]
fn delete_file_plan_backs_up_downloads_then_deletes() {
    let entry = file_entry("K1", 0xAA, 100);
    let plan = Plan::delete_drive_file("/loops", &entry, &no_refs(), false).expect("plan");
    // Session backup + download + delete + verify.
    assert_eq!(plan.steps.len(), 4);
    assert_eq!(plan.steps[0], OpStep::BackupProject);
    let mut backend = drive_backend();
    let report = execute(&plan, &mut backend, |_, _| {});
    assert!(report.succeeded(), "failed: {:?}", report.failed);
    let download_at = backend
        .calls
        .iter()
        .position(|c| c.starts_with("download"))
        .expect("a local copy is downloaded");
    let delete_at = backend
        .calls
        .iter()
        .position(|c| c.starts_with("delete"))
        .expect("then deleted");
    assert!(download_at < delete_at, "backup strictly before delete");
    assert!(backend.drive["/loops"].iter().all(|e| e.name != "K1"));
}

#[test]
fn delete_is_refused_while_references_exist() {
    let entry = file_entry("K1", 0xAA, 100);
    let referenced = DriveRefScan {
        active_slots: vec![3],
        ..DriveRefScan::default()
    };
    assert!(Plan::delete_drive_file("/loops", &entry, &referenced, true).is_err());
    let in_backup = DriveRefScan {
        backed_up: vec![ProjectHit {
            slot_id: 7,
            name: "LIVE".into(),
            sample_slots: vec![1, 2],
        }],
        ..DriveRefScan::default()
    };
    assert!(Plan::delete_drive_file("/loops", &entry, &in_backup, true).is_err());
}

#[test]
fn delete_is_refused_when_the_active_slot_table_still_uses_it() {
    // The plan-time scan saw nothing, but by execute time the content is
    // loaded in a RAM slot — the fresh check must stop the delete.
    let entry = file_entry("K1", 0xAA, 100);
    let plan = Plan::delete_drive_file("/loops", &entry, &no_refs(), true).expect("plan");
    let mut backend = drive_backend();
    backend.ram = vec![RamSlot {
        id: 3,
        hash: 0xAA,
        size: 100,
        used: true,
    }];
    let report = execute(&plan, &mut backend, |_, _| {});
    assert!(!report.succeeded());
    assert!(!backend.calls.iter().any(|c| c.starts_with("delete")));
}

#[test]
fn delete_dir_refuses_non_empty_at_execute_time() {
    let entry = dir_entry("loops");
    let plan = Plan::delete_drive_dir("/", &entry, true).expect("plan");
    let mut backend = drive_backend();
    let report = execute(&plan, &mut backend, |_, _| {});
    assert!(!report.succeeded());
    assert!(!backend.calls.iter().any(|c| c.starts_with("rmdir")));
    // The empty one deletes fine.
    let entry = dir_entry("oneshots");
    let plan = Plan::delete_drive_dir("/", &entry, true).expect("plan");
    let mut backend = drive_backend();
    let report = execute(&plan, &mut backend, |_, _| {});
    assert!(report.succeeded(), "failed: {:?}", report.failed);
}

#[test]
fn create_dir_plan_creates_and_verifies() {
    let plan = Plan::create_drive_dir("/", "new-kit", true).expect("plan");
    let mut backend = drive_backend();
    let report = execute(&plan, &mut backend, |_, _| {});
    assert!(report.succeeded(), "failed: {:?}", report.failed);
    assert!(backend.drive["/"]
        .iter()
        .any(|e| e.name == "new-kit" && e.is_dir));
    // Creating an existing name is refused at execute time.
    let plan = Plan::create_drive_dir("/", "loops", true).expect("plan");
    let mut backend = drive_backend();
    let report = execute(&plan, &mut backend, |_, _| {});
    assert!(!report.succeeded());
    assert!(!backend.calls.iter().any(|c| c.starts_with("mkdir")));
}
