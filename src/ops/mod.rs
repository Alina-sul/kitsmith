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

use std::path::PathBuf;

use crate::analysis::DriveRefScan;
use crate::config::DRIVE_NAME_MAX_LEN;
use crate::device::files::{is_protected_drive_path, DriveEntry};
use crate::model::edit::{FieldValue, KitEdit, KitField};
use crate::model::SAMPLE_SLOT_COUNT;

/// What a +Drive listing must show for one entry name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriveEntryExpect {
    /// No entry of this name.
    Absent,
    /// A directory of this name.
    Dir,
    /// A file of this name with exactly this content identity.
    File { hash: u32, size: u32 },
}

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
    /// Rename/move one +Drive entry (`dst` in another directory = move).
    /// The executor preflights with fresh listings: the source must still
    /// match `expect` and the destination name must be free.
    RenameDrivePath {
        src: String,
        dst: String,
        expect: DriveEntryExpect,
    },
    /// Create a +Drive directory (preflight: the name must be free).
    CreateDriveDir { path: String },
    /// Delete a +Drive directory (preflight: it must list as empty —
    /// regardless of what the device itself would refuse).
    DeleteDriveDir { path: String },
    /// Download a +Drive sample to a local WAV before deleting it — the only
    /// honest backup, since nothing can re-upload it yet.
    BackupDriveFile {
        path: String,
        hash: u32,
        size: u32,
        dest: PathBuf,
    },
    /// Delete one +Drive sample file (preflight: content identity unchanged
    /// and not loaded in the active project's slot table).
    DeleteDriveFile { path: String, hash: u32, size: u32 },
    /// Re-list a +Drive directory and check one entry against `expect`
    /// (the read-back half of every +Drive write).
    VerifyDriveEntry {
        dir: String,
        name: String,
        expect: DriveEntryExpect,
    },
    /// Rename one Sound Pool slot's Sound (fresh read + drift guard, same
    /// policy as kit edits).
    RenamePoolSound { pool_index: u8, name: String },
    /// Read the pool Sound back and check its name (NUL padding ignored).
    VerifyPoolSoundName { pool_index: u8, expected: String },
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
            Self::RenameDrivePath { src, dst, .. } => format!("rename {src} to {dst}"),
            Self::CreateDriveDir { path } => format!("create folder {path}"),
            Self::DeleteDriveDir { path } => format!("delete empty folder {path}"),
            Self::BackupDriveFile { path, dest, .. } => {
                format!("download {path} to {} first", dest.display())
            }
            Self::DeleteDriveFile { path, .. } => format!("delete {path} from the +Drive"),
            Self::VerifyDriveEntry { dir, name, expect } => match expect {
                DriveEntryExpect::Absent => format!("re-list {dir} and verify {name} is gone"),
                DriveEntryExpect::Dir => format!("re-list {dir} and verify folder {name} exists"),
                DriveEntryExpect::File { .. } => {
                    format!("re-list {dir} and verify {name} is there with unchanged content")
                }
            },
            Self::RenamePoolSound { pool_index, name } => format!(
                "pool sound {:03}: set the name to “{name}” and write it back",
                pool_index + 1
            ),
            Self::VerifyPoolSoundName { pool_index, .. } => format!(
                "read pool sound {:03} back and verify its name",
                pool_index + 1
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

    /// A plan that renames one +Drive entry inside its directory.
    pub fn rename_drive_entry(
        dir: &str,
        entry: &DriveEntry,
        new_name: &str,
        backup_taken: bool,
    ) -> Result<Self, OpError> {
        check_drive_writable(dir, entry)?;
        validate_drive_name(new_name)?;
        if new_name == entry.name {
            return Err(OpError::InvalidRequest("the name is unchanged".into()));
        }
        let src = drive_join(dir, &entry.name);
        let dst = drive_join(dir, new_name);
        let expect = entry_expect(entry);
        let mut steps = session_backup(backup_taken);
        steps.push(OpStep::RenameDrivePath {
            src,
            dst,
            expect: expect.clone(),
        });
        steps.push(OpStep::VerifyDriveEntry {
            dir: dir.to_owned(),
            name: entry.name.clone(),
            expect: DriveEntryExpect::Absent,
        });
        steps.push(OpStep::VerifyDriveEntry {
            dir: dir.to_owned(),
            name: new_name.to_owned(),
            expect,
        });
        Ok(Self {
            title: format!("Rename {} to {new_name}", entry.name),
            steps,
        })
    }

    /// A plan that moves one +Drive entry to another directory (same rename
    /// opcode, destination path in `dst_dir`).
    pub fn move_drive_entry(
        src_dir: &str,
        entry: &DriveEntry,
        dst_dir: &str,
        backup_taken: bool,
    ) -> Result<Self, OpError> {
        check_drive_writable(src_dir, entry)?;
        if is_protected_drive_path(dst_dir) {
            return Err(OpError::InvalidRequest(
                "the factory tree is write-protected".into(),
            ));
        }
        // Free-text destinations must be well-formed absolute folder paths;
        // "list of a nonexistent path" is only known to time out, so refuse
        // the malformed ones before they get near the device.
        if !dst_dir.starts_with('/')
            || (dst_dir != "/" && dst_dir.ends_with('/'))
            || dst_dir.contains("//")
        {
            return Err(OpError::InvalidRequest(
                "the destination must be an absolute folder path like /SAMPLES/KICKS".into(),
            ));
        }
        if src_dir == dst_dir {
            return Err(OpError::InvalidRequest("already in this folder".into()));
        }
        let src = drive_join(src_dir, &entry.name);
        if entry.is_dir && (dst_dir == src || dst_dir.starts_with(&format!("{src}/"))) {
            return Err(OpError::InvalidRequest(
                "cannot move a folder into itself".into(),
            ));
        }
        let dst = drive_join(dst_dir, &entry.name);
        let expect = entry_expect(entry);
        let mut steps = session_backup(backup_taken);
        steps.push(OpStep::RenameDrivePath {
            src,
            dst,
            expect: expect.clone(),
        });
        steps.push(OpStep::VerifyDriveEntry {
            dir: src_dir.to_owned(),
            name: entry.name.clone(),
            expect: DriveEntryExpect::Absent,
        });
        steps.push(OpStep::VerifyDriveEntry {
            dir: dst_dir.to_owned(),
            name: entry.name.clone(),
            expect,
        });
        Ok(Self {
            title: format!("Move {} to {dst_dir}", entry.name),
            steps,
        })
    }

    /// A plan that creates a new +Drive directory under `parent`.
    pub fn create_drive_dir(parent: &str, name: &str, backup_taken: bool) -> Result<Self, OpError> {
        if is_protected_drive_path(parent) {
            return Err(OpError::InvalidRequest(
                "the factory tree is write-protected".into(),
            ));
        }
        validate_drive_name(name)?;
        let mut steps = session_backup(backup_taken);
        steps.push(OpStep::CreateDriveDir {
            path: drive_join(parent, name),
        });
        steps.push(OpStep::VerifyDriveEntry {
            dir: parent.to_owned(),
            name: name.to_owned(),
            expect: DriveEntryExpect::Dir,
        });
        Ok(Self {
            title: format!("Create folder {name}"),
            steps,
        })
    }

    /// A plan that deletes an empty +Drive directory (the executor refuses
    /// a non-empty one at execute time, on a fresh listing).
    pub fn delete_drive_dir(
        parent: &str,
        entry: &DriveEntry,
        backup_taken: bool,
    ) -> Result<Self, OpError> {
        check_drive_writable(parent, entry)?;
        if !entry.is_dir {
            return Err(OpError::InvalidRequest(format!(
                "{} is a file, not a folder",
                entry.name
            )));
        }
        let mut steps = session_backup(backup_taken);
        steps.push(OpStep::DeleteDriveDir {
            path: drive_join(parent, &entry.name),
        });
        steps.push(OpStep::VerifyDriveEntry {
            dir: parent.to_owned(),
            name: entry.name.clone(),
            expect: DriveEntryExpect::Absent,
        });
        Ok(Self {
            title: format!("Delete folder {}", entry.name),
            steps,
        })
    }

    /// A plan that deletes one +Drive sample file: refuses while any known
    /// project references the content, and downloads a local WAV copy first
    /// (nothing can re-upload a deleted sample yet — that copy is the only
    /// recovery path).
    pub fn delete_drive_file(
        parent: &str,
        entry: &DriveEntry,
        scan: &DriveRefScan,
        backup_taken: bool,
    ) -> Result<Self, OpError> {
        check_drive_writable(parent, entry)?;
        if entry.is_dir {
            return Err(OpError::InvalidRequest(format!(
                "{} is a folder — delete it as one",
                entry.name
            )));
        }
        if scan.has_references() {
            return Err(OpError::InvalidRequest(format!(
                "{} is still used by {} sample slot{} in known projects — \
                 Kitsmith won't delete it while projects reference it; remove \
                 it from those projects on the device first",
                entry.name,
                scan.reference_count(),
                if scan.reference_count() == 1 { "" } else { "s" }
            )));
        }
        let path = drive_join(parent, &entry.name);
        let dest = crate::cache::default_dir()
            .join("deleted-samples")
            .join(format!("{:08x}-{}.wav", entry.hash, entry.name));
        let mut steps = session_backup(backup_taken);
        steps.push(OpStep::BackupDriveFile {
            path: path.clone(),
            hash: entry.hash,
            size: entry.size,
            dest,
        });
        steps.push(OpStep::DeleteDriveFile {
            path,
            hash: entry.hash,
            size: entry.size,
        });
        steps.push(OpStep::VerifyDriveEntry {
            dir: parent.to_owned(),
            name: entry.name.clone(),
            expect: DriveEntryExpect::Absent,
        });
        Ok(Self {
            title: format!("Delete {}", entry.name),
            steps,
        })
    }

    /// A plan that renames one Sound Pool slot's Sound, prepending the
    /// session backup when one has not been taken yet.
    pub fn rename_pool_sound(
        pool_index: u8,
        name: &str,
        backup_taken: bool,
    ) -> Result<Self, OpError> {
        if pool_index >= 128 {
            return Err(OpError::InvalidRequest(format!(
                "pool slot must be 0–127, got {pool_index}"
            )));
        }
        crate::model::edit::validate_name(name).map_err(OpError::InvalidRequest)?;
        let mut steps = session_backup(backup_taken);
        steps.push(OpStep::RenamePoolSound {
            pool_index,
            name: name.to_owned(),
        });
        steps.push(OpStep::VerifyPoolSoundName {
            pool_index,
            expected: name.to_owned(),
        });
        Ok(Self {
            title: format!("Rename pool sound {:03}", pool_index + 1),
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
            OpStep::RenameDrivePath { src, dst, expect } => {
                rename_drive_path(backend, src, dst, expect)
            }
            OpStep::CreateDriveDir { path } => create_drive_dir(backend, path),
            OpStep::DeleteDriveDir { path } => delete_drive_dir(backend, path),
            OpStep::BackupDriveFile {
                path,
                hash,
                size,
                dest,
            } => backup_drive_file(backend, path, *hash, *size, dest),
            OpStep::DeleteDriveFile { path, hash, size } => {
                delete_drive_file(backend, path, *hash, *size)
            }
            OpStep::VerifyDriveEntry { dir, name, expect } => {
                verify_drive_entry(backend, dir, name, expect)
            }
            OpStep::RenamePoolSound { pool_index, name } => {
                backend.rename_pool_sound(*pool_index, name)
            }
            OpStep::VerifyPoolSoundName {
                pool_index,
                expected,
            } => verify_pool_sound_name(backend, *pool_index, expected),
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

/// Read-back verification of a pool Sound's name (device names come back
/// NUL-padded to 15 bytes — trim before comparing).
fn verify_pool_sound_name(
    backend: &mut dyn DeviceBackend,
    pool_index: u8,
    expected: &str,
) -> Result<(), OpError> {
    let read = backend.read_pool_sound_name(pool_index)?;
    if read.trim_end_matches('\0') == expected.trim_end_matches('\0') {
        Ok(())
    } else {
        Err(OpError::VerificationFailed(format!(
            "pool sound {:03}: wrote \"{expected}\", device reports \"{}\"",
            pool_index + 1,
            read.trim_end_matches('\0')
        )))
    }
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

// ---- +Drive step executors ------------------------------------------------

/// Splits a full +Drive path into (directory, entry name).
fn split_drive_path(path: &str) -> (String, String) {
    match path.rsplit_once('/') {
        Some(("", name)) => ("/".to_owned(), name.to_owned()),
        Some((dir, name)) => (dir.to_owned(), name.to_owned()),
        None => ("/".to_owned(), path.to_owned()),
    }
}

/// Finds one entry by name in a fresh listing.
fn find_entry<'a>(entries: &'a [DriveEntry], name: &str) -> Option<&'a DriveEntry> {
    entries.iter().find(|e| e.name == name)
}

/// Case-insensitive lookup for collision checks: the device filesystem's
/// case behavior is unresolved (see the /incoming vs INCOMING finding in
/// docs/hardware-verification.md), so a name that differs only by case is
/// conservatively treated as taken — a wrong refusal beats an overwrite.
fn find_entry_ci<'a>(entries: &'a [DriveEntry], name: &str) -> Option<&'a DriveEntry> {
    entries.iter().find(|e| e.name.eq_ignore_ascii_case(name))
}

/// Checks a fresh source entry against what the plan was built from. The
/// listing the user acted on may be stale — any mismatch stops the write.
fn check_source_entry(entry: &DriveEntry, expect: &DriveEntryExpect) -> Result<(), OpError> {
    if entry.write_protected {
        return Err(OpError::InvalidRequest(format!(
            "{} is write-protected",
            entry.name
        )));
    }
    let ok = match expect {
        DriveEntryExpect::Absent => false,
        DriveEntryExpect::Dir => entry.is_dir,
        DriveEntryExpect::File { hash, size } => {
            !entry.is_dir && entry.hash == *hash && entry.size == *size
        }
    };
    if ok {
        Ok(())
    } else {
        Err(OpError::InvalidRequest(format!(
            "{} changed on the device since the browser read it — re-open the \
             folder and retry",
            entry.name
        )))
    }
}

/// Preflight + send for a rename/move.
fn rename_drive_path(
    backend: &mut dyn DeviceBackend,
    src: &str,
    dst: &str,
    expect: &DriveEntryExpect,
) -> Result<(), OpError> {
    let (src_dir, src_name) = split_drive_path(src);
    let (dst_dir, dst_name) = split_drive_path(dst);
    let entries = backend.list_sample_dir(&src_dir)?;
    let entry = find_entry(&entries, &src_name)
        .ok_or_else(|| OpError::InvalidRequest(format!("{src} no longer exists on the device")))?;
    check_source_entry(entry, expect)?;
    let dst_entries = if dst_dir == src_dir {
        entries
    } else {
        // A mistyped destination surfaces here as a transport-level failure;
        // say what it means, not which opcode timed out.
        backend.list_sample_dir(&dst_dir).map_err(|_| {
            OpError::InvalidRequest(format!(
                "the folder {dst_dir} was not found on the +Drive — check the \
                 spelling or create it first; nothing was moved"
            ))
        })?
    };
    // Skip the source itself so a pure case-change rename stays possible
    // once the probe settles case semantics; any OTHER case-variant is a
    // collision.
    if dst_entries.iter().any(|e| {
        e.name.eq_ignore_ascii_case(&dst_name) && !(dst_dir == src_dir && e.name == src_name)
    }) {
        return Err(OpError::InvalidRequest(format!(
            "{dst} already exists (or differs only by case) — pick another name"
        )));
    }
    backend.rename_sample_path(src, dst)
}

/// Preflight + send for a directory creation.
fn create_drive_dir(backend: &mut dyn DeviceBackend, path: &str) -> Result<(), OpError> {
    let (parent, name) = split_drive_path(path);
    let entries = backend.list_sample_dir(&parent)?;
    if find_entry_ci(&entries, &name).is_some() {
        return Err(OpError::InvalidRequest(format!(
            "{path} already exists (or differs only by case) — pick another name"
        )));
    }
    backend.create_sample_dir(path)
}

/// Preflight + send for a directory deletion: only ever an empty one.
fn delete_drive_dir(backend: &mut dyn DeviceBackend, path: &str) -> Result<(), OpError> {
    let contents = backend.list_sample_dir(path)?;
    if !contents.is_empty() {
        return Err(OpError::InvalidRequest(format!(
            "{path} is not empty ({} item{}) — delete refused",
            contents.len(),
            if contents.len() == 1 { "" } else { "s" }
        )));
    }
    backend.delete_sample_dir(path)
}

/// Downloads the file to `dest` (WAV + JSON sidecar) before deletion.
fn backup_drive_file(
    backend: &mut dyn DeviceBackend,
    path: &str,
    hash: u32,
    size: u32,
    dest: &std::path::Path,
) -> Result<(), OpError> {
    backend
        .download_sample_to(path, dest, hash, size)
        .map(|_| ())
}

/// Preflight + send for a file deletion: identity unchanged and nothing in
/// the active project's slot table still points at the content.
fn delete_drive_file(
    backend: &mut dyn DeviceBackend,
    path: &str,
    hash: u32,
    size: u32,
) -> Result<(), OpError> {
    let (dir, name) = split_drive_path(path);
    let entries = backend.list_sample_dir(&dir)?;
    let entry = find_entry(&entries, &name)
        .ok_or_else(|| OpError::InvalidRequest(format!("{path} no longer exists")))?;
    check_source_entry(entry, &DriveEntryExpect::File { hash, size })?;
    let slots = backend.ram_sample_slots()?;
    if slots
        .iter()
        .any(|s| s.used && s.hash == hash && s.size == size)
    {
        return Err(OpError::InvalidRequest(format!(
            "{name} is loaded in the active project's sample slots — unload it first"
        )));
    }
    backend.delete_sample_file(path)
}

/// Read-back verification of one +Drive listing entry.
fn verify_drive_entry(
    backend: &mut dyn DeviceBackend,
    dir: &str,
    name: &str,
    expect: &DriveEntryExpect,
) -> Result<(), OpError> {
    let entries = backend.list_sample_dir(dir)?;
    let entry = find_entry(&entries, name);
    let ok = match expect {
        DriveEntryExpect::Absent => entry.is_none(),
        DriveEntryExpect::Dir => entry.is_some_and(|e| e.is_dir),
        DriveEntryExpect::File { hash, size } => {
            entry.is_some_and(|e| !e.is_dir && e.hash == *hash && e.size == *size)
        }
    };
    if ok {
        Ok(())
    } else {
        Err(OpError::VerificationFailed(format!(
            "{dir}/{name}: the device listing does not show the expected result"
        )))
    }
}

// ---- +Drive plan helpers ----------------------------------------------------

/// The session-backup prefix every mutating plan starts with.
fn session_backup(backup_taken: bool) -> Vec<OpStep> {
    if backup_taken {
        Vec::new()
    } else {
        vec![OpStep::BackupProject]
    }
}

/// What a fresh listing must show for `entry` after a successful move/rename.
fn entry_expect(entry: &DriveEntry) -> DriveEntryExpect {
    if entry.is_dir {
        DriveEntryExpect::Dir
    } else {
        DriveEntryExpect::File {
            hash: entry.hash,
            size: entry.size,
        }
    }
}

/// Refuses writes into the factory tree and on write-protected entries.
fn check_drive_writable(dir: &str, entry: &DriveEntry) -> Result<(), OpError> {
    if is_protected_drive_path(dir) {
        return Err(OpError::InvalidRequest(
            "the factory tree is write-protected".into(),
        ));
    }
    if entry.write_protected {
        return Err(OpError::InvalidRequest(format!(
            "{} is write-protected",
            entry.name
        )));
    }
    Ok(())
}

/// Validates a new +Drive file/folder name before it goes anywhere near the
/// device: printable ASCII, no path separators, bounded length.
fn validate_drive_name(name: &str) -> Result<(), OpError> {
    if name.is_empty() {
        return Err(OpError::InvalidRequest("the name is empty".into()));
    }
    if name.len() > DRIVE_NAME_MAX_LEN {
        return Err(OpError::InvalidRequest(format!(
            "the name is longer than {DRIVE_NAME_MAX_LEN} characters"
        )));
    }
    if !name.is_ascii() || name.chars().any(|c| c.is_ascii_control()) {
        return Err(OpError::InvalidRequest(
            "the name must be printable ASCII".into(),
        ));
    }
    if name.contains('/') || name == "." || name == ".." {
        return Err(OpError::InvalidRequest(
            "the name cannot contain '/' or be a dot path".into(),
        ));
    }
    Ok(())
}

/// Joins a directory and an entry name into a full +Drive path.
fn drive_join(dir: &str, name: &str) -> String {
    if dir == "/" {
        format!("/{name}")
    } else {
        format!("{dir}/{name}")
    }
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
