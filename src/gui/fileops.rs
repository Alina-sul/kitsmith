//! Worker for file-protocol write operations: native project rename,
//! project copy to an empty slot, and loading a +Drive sample into a project
//! RAM slot. One operation at a time; every op preflights with fresh reads,
//! backs up what it touches, and verifies by reading back.

use std::sync::{Arc, Mutex};
use std::thread;

use crate::cache;
use crate::device::detect::is_rytm_port;
use crate::device::files::{crc32, FileClient, PROJECTS_PATH};
use crate::device::projfile;
use crate::midi::link::MidiLink;
use crate::midi::ports;
use crate::model::SampleSlotInfo;

/// A device-mutating file operation.
#[derive(Debug, Clone)]
pub enum FileOp {
    /// Rewrite a project slot's native name (download → patch → upload →
    /// verify byte-for-byte).
    RenameProject { slot: u32, new_name: String },
    /// Copy a project slot to an **empty** slot (device-side copy).
    CopyProject { src: u32, dst: u32 },
    /// Move a project slot to an **empty** slot (backup first, byte-compare
    /// the destination against the pre-move payload).
    MoveProject { src: u32, dst: u32 },
    /// Swap two **occupied** project slots (both backed up first, verified
    /// crosswise).
    SwapProjects { a: u32, b: u32 },
    /// Clear a project slot (mandatory local backup before the clear).
    ClearProject { slot: u32 },
    /// Re-upload a locally backed-up project into its (empty) slot.
    RestoreProject { slot: u32 },
    /// Load a +Drive sample file into a free project RAM slot.
    LoadDriveSample { drive_path: String, slot: u8 },
}

impl FileOp {
    pub fn describe(&self) -> String {
        match self {
            Self::RenameProject { slot, new_name } => {
                format!("Rename project {slot:03} to “{new_name}”")
            }
            Self::CopyProject { src, dst } => format!("Copy project {src:03} to slot {dst:03}"),
            Self::MoveProject { src, dst } => format!("Move project {src:03} to slot {dst:03}"),
            Self::SwapProjects { a, b } => format!("Swap projects {a:03} and {b:03}"),
            Self::ClearProject { slot } => format!("Clear project slot {slot:03}"),
            Self::RestoreProject { slot } => {
                format!("Restore project slot {slot:03} from the local backup")
            }
            Self::LoadDriveSample { drive_path, slot } => {
                format!("Load {drive_path} into sample slot {slot:03}")
            }
        }
    }
}

/// What a finished operation changed (so the app can update its model).
#[derive(Debug, Clone)]
pub enum FileOpOutcome {
    Renamed { slot: u32, name: String },
    Copied { dst: u32, name: String },
    Moved { src: u32, dst: u32, name: String },
    Swapped { a: u32, b: u32 },
    Cleared { slot: u32 },
    Restored { slot: u32, name: String },
    SampleLoaded(SampleSlotInfo),
}

#[derive(Default)]
enum OpStatus {
    #[default]
    Idle,
    Running(String),
    Finished(Box<Result<(FileOpOutcome, String), String>>),
}

/// Handle to the (at most one) running file operation.
#[derive(Default)]
pub struct FileOpController {
    status: Arc<Mutex<OpStatus>>,
}

impl FileOpController {
    pub fn is_running(&self) -> bool {
        matches!(
            *self.status.lock().expect("fileop status lock poisoned"),
            OpStatus::Running(_)
        )
    }

    /// Human description of the running op, if any.
    pub fn running_description(&self) -> Option<String> {
        match &*self.status.lock().expect("fileop status lock poisoned") {
            OpStatus::Running(desc) => Some(desc.clone()),
            _ => None,
        }
    }

    /// If an op finished, returns its result (resetting to idle).
    pub fn take_finished(&self) -> Option<Result<(FileOpOutcome, String), String>> {
        let mut status = self.status.lock().expect("fileop status lock poisoned");
        if matches!(*status, OpStatus::Finished(_)) {
            if let OpStatus::Finished(result) = std::mem::take(&mut *status) {
                return Some(*result);
            }
        }
        None
    }

    /// Starts executing `op` over `in_port`.
    pub fn start(&self, in_port: String, op: FileOp, on_change: impl Fn() + Send + 'static) {
        if self.is_running() {
            return;
        }
        *self.status.lock().expect("fileop status lock poisoned") =
            OpStatus::Running(op.describe());
        let status = Arc::clone(&self.status);
        thread::Builder::new()
            .name("file-op".into())
            .spawn(move || {
                let result = run_op(&in_port, &op);
                *status.lock().expect("fileop status lock poisoned") =
                    OpStatus::Finished(Box::new(result));
                on_change();
            })
            .expect("failed to spawn file-op thread");
    }
}

fn open_client(in_port: &str) -> Result<(MidiLink, FileClient), String> {
    let out_port = ports::snapshot()
        .outputs
        .into_iter()
        .find(|n| is_rytm_port(n))
        .ok_or("no Rytm MIDI output port found")?;
    let mut link = MidiLink::open(in_port, &out_port).map_err(|e| e.to_string())?;
    let mut client = FileClient::new();
    if !client.ping(&mut link) {
        return Err("device did not answer the file-protocol ping".into());
    }
    Ok((link, client))
}

fn run_op(in_port: &str, op: &FileOp) -> Result<(FileOpOutcome, String), String> {
    let (mut link, mut client) = open_client(in_port)?;
    match op {
        FileOp::RenameProject { slot, new_name } => {
            rename_project(&mut link, &mut client, *slot, new_name)
        }
        FileOp::CopyProject { src, dst } => copy_project(&mut link, &mut client, *src, *dst),
        FileOp::MoveProject { src, dst } => move_project(&mut link, &mut client, *src, *dst),
        FileOp::SwapProjects { a, b } => swap_projects(&mut link, &mut client, *a, *b),
        FileOp::ClearProject { slot } => clear_project(&mut link, &mut client, *slot),
        FileOp::RestoreProject { slot } => restore_project(&mut link, &mut client, *slot),
        FileOp::LoadDriveSample { drive_path, slot } => {
            load_drive_sample(&mut link, &mut client, drive_path, *slot)
        }
    }
}

/// Downloads one project slot and stores it as a local backup; the payload
/// doubles as the verify oracle for slot moves/swaps. Aborts (before any
/// write) when the backup cannot be saved.
fn backup_slot(
    link: &mut MidiLink,
    client: &mut FileClient,
    slot: u32,
    label: &str,
) -> Result<Vec<u8>, String> {
    let path = format!("{PROJECTS_PATH}/{slot}");
    let download = client
        .download_data(link, &path, false, |_, _| {})
        .ok_or_else(|| format!("could not download project {slot:03} (backup-first)"))?;
    cache::save_project_backup(
        &cache::default_dir(),
        slot,
        label,
        &download.bytes,
        crc32(0, &download.bytes),
        true,
    )
    .map_err(|e| format!("could not store the safety backup for {slot:03}: {e} — aborted"))?;
    Ok(download.bytes)
}

/// One project slot's identity in a listing snapshot.
type SlotSnapshot = std::collections::BTreeMap<u32, (String, bool)>;

/// Fresh slot listing as id → (name, has_valid_data).
fn snapshot_slots(link: &mut MidiLink, client: &mut FileClient) -> Result<SlotSnapshot, String> {
    Ok(client
        .list_data_dir(link, PROJECTS_PATH)
        .ok_or("could not list project slots")?
        .into_iter()
        .filter_map(|e| e.id.map(|id| (id, (e.name, e.has_valid_data))))
        .collect())
}

fn slot_is_valid(slots: &SlotSnapshot, id: u32) -> bool {
    slots.get(&id).is_some_and(|(_, valid)| *valid)
}

/// With the slot-op framing unverified, the realistic failure mode is a
/// misrouted path hitting a slot we never named. Every slot except the
/// intended ones must be identical between the pre- and post-op listings.
fn verify_untouched_slots(
    before: &SlotSnapshot,
    after: &SlotSnapshot,
    touched: &[u32],
) -> Result<(), String> {
    let changed: Vec<u32> = before
        .keys()
        .chain(after.keys())
        .filter(|id| !touched.contains(id))
        .filter(|id| before.get(id) != after.get(id))
        .copied()
        .collect();
    if changed.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "slots {} changed even though the operation never named them — \
             STOP and re-sync; do not run further slot operations before \
             checking the device",
            changed
                .iter()
                .map(|id| format!("{id:03}"))
                .collect::<Vec<_>>()
                .join(", ")
        ))
    }
}

/// Absolute path of one slot's local backup payload, for error messages.
fn backup_file_path(slot: u32) -> String {
    cache::default_dir()
        .join("projects")
        .join(format!("{slot:03}.bin"))
        .display()
        .to_string()
}

fn move_project(
    link: &mut MidiLink,
    client: &mut FileClient,
    src: u32,
    dst: u32,
) -> Result<(FileOpOutcome, String), String> {
    let before = snapshot_slots(link, client)?;
    let name = before
        .get(&src)
        .filter(|(_, valid)| *valid)
        .map(|(name, _)| name.clone())
        .ok_or(format!("source project {src:03} not found"))?;
    if slot_is_valid(&before, dst) {
        return Err(format!("slot {dst:03} is not empty — move refused"));
    }
    // The pre-move download is both the safety backup and the verify oracle.
    let original = backup_slot(link, client, src, "(pre-move)")?;

    client.move_data(
        link,
        &format!("{PROJECTS_PATH}/{src}"),
        &format!("{PROJECTS_PATH}/{dst}"),
    )?;

    let after = snapshot_slots(link, client)?;
    verify_untouched_slots(&before, &after, &[src, dst])?;
    if slot_is_valid(&after, src) || !slot_is_valid(&after, dst) {
        return Err(format!(
            "the move command ran but the device listing does not match \
             afterwards — nothing was changed locally. Re-sync and check \
             slots {src:03} and {dst:03} before retrying; the pre-move \
             backup is at {}",
            backup_file_path(src)
        ));
    }
    let moved = client
        .download_data(link, &format!("{PROJECTS_PATH}/{dst}"), false, |_, _| {})
        .ok_or(
            "the move looks complete, but the verification download failed — \
             re-sync and open the project before trusting it",
        )?;
    if moved.bytes != original {
        // The source is empty and the destination is wrong: put the original
        // back where it came from (same recovery rename already performs).
        let restore =
            client.upload_data(link, &format!("{PROJECTS_PATH}/{src}"), &original, |_| {});
        return Err(match restore {
            Ok(()) => format!(
                "the project arrived at slot {dst:03} damaged — the original \
                 was restored to slot {src:03}. Re-sync before retrying"
            ),
            Err(e) => format!(
                "the project arrived at slot {dst:03} damaged AND restoring \
                 to {src:03} failed ({e}) — restore manually from {}",
                backup_file_path(src)
            ),
        });
    }
    // The verified bytes now live at dst: index them there and drop the
    // stale src entry (identical content — no recovery is lost).
    let _ = cache::save_project_backup(
        &cache::default_dir(),
        dst,
        &name,
        &moved.bytes,
        crc32(0, &moved.bytes),
        true,
    );
    let _ = cache::remove_project_backup(&cache::default_dir(), src);
    Ok((
        FileOpOutcome::Moved {
            src,
            dst,
            name: name.clone(),
        },
        format!("Project {src:03} “{name}” moved to slot {dst:03} and verified byte-for-byte"),
    ))
}

fn swap_projects(
    link: &mut MidiLink,
    client: &mut FileClient,
    a: u32,
    b: u32,
) -> Result<(FileOpOutcome, String), String> {
    let before = snapshot_slots(link, client)?;
    for slot in [a, b] {
        if !slot_is_valid(&before, slot) {
            return Err(format!(
                "slot {slot:03} is empty — use Move instead of Swap"
            ));
        }
    }
    let name_a = before.get(&a).map(|(n, _)| n.clone()).unwrap_or_default();
    let name_b = before.get(&b).map(|(n, _)| n.clone()).unwrap_or_default();
    let payload_a = backup_slot(link, client, a, "(pre-swap)")?;
    let payload_b = backup_slot(link, client, b, "(pre-swap)")?;

    client.swap_data(
        link,
        &format!("{PROJECTS_PATH}/{a}"),
        &format!("{PROJECTS_PATH}/{b}"),
    )?;

    let after = snapshot_slots(link, client)?;
    verify_untouched_slots(&before, &after, &[a, b])?;
    // Verify crosswise: each slot must now hold the other's exact bytes.
    for (slot, expected) in [(a, &payload_b), (b, &payload_a)] {
        let read = client
            .download_data(link, &format!("{PROJECTS_PATH}/{slot}"), false, |_, _| {})
            .ok_or("swap sent but a verification download failed")?;
        if &read.bytes != expected {
            return Err(format!(
                "after the swap, slot {slot:03} does not hold the expected \
                 project — re-sync and check both slots before touching them. \
                 The pre-swap backups are at {} and {}",
                backup_file_path(a),
                backup_file_path(b)
            ));
        }
    }
    // Local mirrors must match the device: re-save both backups crosswise
    // under their new slots (with real names, not the pre-swap labels), or
    // every snapshot-driven view serves the wrong project.
    let cache_dir = cache::default_dir();
    let _ = cache::save_project_backup(
        &cache_dir,
        a,
        &name_b,
        &payload_b,
        crc32(0, &payload_b),
        true,
    );
    let _ = cache::save_project_backup(
        &cache_dir,
        b,
        &name_a,
        &payload_a,
        crc32(0, &payload_a),
        true,
    );
    Ok((
        FileOpOutcome::Swapped { a, b },
        format!("Projects {a:03} and {b:03} swapped and verified byte-for-byte"),
    ))
}

fn clear_project(
    link: &mut MidiLink,
    client: &mut FileClient,
    slot: u32,
) -> Result<(FileOpOutcome, String), String> {
    let before = snapshot_slots(link, client)?;
    if !slot_is_valid(&before, slot) {
        return Err(format!("slot {slot:03} is already empty"));
    }
    // Mandatory backup; a failed save aborts before anything is sent. The
    // manifest entry stays keyed to this slot on purpose — it is what
    // "Restore from backup" re-uploads.
    backup_slot(link, client, slot, "(cleared — restorable)")?;

    client.clear_data(link, &format!("{PROJECTS_PATH}/{slot}"))?;

    let after = snapshot_slots(link, client)?;
    verify_untouched_slots(&before, &after, &[slot])?;
    if slot_is_valid(&after, slot) {
        return Err(format!(
            "the clear command ran but slot {slot:03} still reads as \
             occupied — re-sync and retry"
        ));
    }
    Ok((
        FileOpOutcome::Cleared { slot },
        format!(
            "Project slot {slot:03} cleared — restore it any time via \
             “Restore from backup” on the empty slot"
        ),
    ))
}

/// Re-uploads a locally backed-up project into its (empty) slot — the
/// recovery path that makes the pre-clear backup a real promise.
fn restore_project(
    link: &mut MidiLink,
    client: &mut FileClient,
    slot: u32,
) -> Result<(FileOpOutcome, String), String> {
    let cache_dir = cache::default_dir();
    let manifest = cache::load_project_manifest(&cache_dir);
    let backup = manifest
        .get(&slot)
        .ok_or(format!("no local backup exists for slot {slot:03}"))?;
    if !backup.raw {
        return Err(format!(
            "the backup for slot {slot:03} is in the old compressed format \
             and cannot be re-uploaded"
        ));
    }
    let payload = cache::load_project_backup(&cache_dir, slot)
        .ok_or(format!("the backup file for slot {slot:03} is missing"))?;
    if crc32(0, &payload) != backup.crc {
        return Err(format!(
            "the backup file for slot {slot:03} does not match its recorded \
             checksum — refusing to upload a damaged payload"
        ));
    }
    let before = snapshot_slots(link, client)?;
    if slot_is_valid(&before, slot) {
        return Err(format!(
            "slot {slot:03} is occupied on the device — restore refused \
             (clear it first if you really want to overwrite)"
        ));
    }

    client.upload_data(link, &format!("{PROJECTS_PATH}/{slot}"), &payload, |_| {})?;

    let after = snapshot_slots(link, client)?;
    verify_untouched_slots(&before, &after, &[slot])?;
    let read = client
        .download_data(link, &format!("{PROJECTS_PATH}/{slot}"), false, |_, _| {})
        .ok_or("upload sent but the verification download failed — re-sync")?;
    if read.bytes != payload {
        return Err(format!(
            "the restored project reads back differently than the backup — \
             do not trust slot {slot:03}; the backup at {} is untouched",
            backup_file_path(slot)
        ));
    }
    // The slot is live again: parse the real name for the model/manifest.
    let name = projfile::parse_raw_project(&payload)
        .map(|s| s.name)
        .unwrap_or_default();
    let _ = cache::save_project_backup(&cache_dir, slot, &name, &payload, crc32(0, &payload), true);
    Ok((
        FileOpOutcome::Restored {
            slot,
            name: name.clone(),
        },
        format!("Project “{name}” restored to slot {slot:03} and verified byte-for-byte"),
    ))
}

fn rename_project(
    link: &mut MidiLink,
    client: &mut FileClient,
    slot: u32,
    new_name: &str,
) -> Result<(FileOpOutcome, String), String> {
    let path = format!("{PROJECTS_PATH}/{slot}");
    // Fresh download doubles as the pre-write backup.
    let original = client
        .download_data(link, &path, false, |_, _| {})
        .ok_or("could not download the project (backup-first)")?;
    let cache_dir = cache::default_dir();
    cache::save_project_backup(
        &cache_dir,
        slot,
        "(pre-rename)",
        &original.bytes,
        crc32(0, &original.bytes),
        true,
    )
    .map_err(|e| format!("could not store the safety backup: {e}"))?;

    let mut patched = original.bytes.clone();
    projfile::patch_project_name(&mut patched, new_name)?;

    client.upload_data(link, &path, &patched, |_| {})?;

    // Verify byte-for-byte; on mismatch, restore the original.
    let read_back = client
        .download_data(link, &path, false, |_, _| {})
        .ok_or("rename written but verification download failed — re-check the slot")?;
    if read_back.bytes != patched {
        let restore = client.upload_data(link, &path, &original.bytes, |_| {});
        return Err(match restore {
            Ok(()) => "verification mismatch — the original project was restored".into(),
            Err(e) => format!(
                "verification mismatch AND restore failed ({e}) — restore manually from the \
                 local backup"
            ),
        });
    }
    let clean = crate::model::names::clean(new_name).to_owned();
    cache::save_project_backup(&cache_dir, slot, &clean, &patched, crc32(0, &patched), true)
        .map_err(|e| format!("renamed OK, but updating the local backup failed: {e}"))?;
    Ok((
        FileOpOutcome::Renamed {
            slot,
            name: clean.clone(),
        },
        format!("Project {slot:03} renamed to “{clean}” and verified"),
    ))
}

fn copy_project(
    link: &mut MidiLink,
    client: &mut FileClient,
    src: u32,
    dst: u32,
) -> Result<(FileOpOutcome, String), String> {
    let entries = client
        .list_data_dir(link, PROJECTS_PATH)
        .ok_or("could not list project slots")?;
    if entries
        .iter()
        .any(|e| e.id == Some(dst) && e.has_valid_data)
    {
        return Err(format!("slot {dst:03} is not empty — copy refused"));
    }
    let src_name = entries
        .iter()
        .find(|e| e.id == Some(src))
        .map(|e| e.name.clone())
        .ok_or(format!("source project {src:03} not found"))?;

    client.copy_data(
        link,
        &format!("{PROJECTS_PATH}/{src}"),
        &format!("{PROJECTS_PATH}/{dst}"),
    )?;

    let entries = client
        .list_data_dir(link, PROJECTS_PATH)
        .ok_or("copy sent but verification listing failed")?;
    if !entries
        .iter()
        .any(|e| e.id == Some(dst) && e.has_valid_data)
    {
        return Err("copy sent but the destination slot did not appear".into());
    }

    // Index the fresh copy right away so it is browsable.
    let path = format!("{PROJECTS_PATH}/{dst}");
    if let Some(download) = client.download_data(link, &path, false, |_, _| {}) {
        let _ = cache::save_project_backup(
            &cache::default_dir(),
            dst,
            &src_name,
            &download.bytes,
            crc32(0, &download.bytes),
            true,
        );
    }
    Ok((
        FileOpOutcome::Copied {
            dst,
            name: src_name.clone(),
        },
        format!("Project {src:03} “{src_name}” copied to slot {dst:03} and verified"),
    ))
}

fn load_drive_sample(
    link: &mut MidiLink,
    client: &mut FileClient,
    drive_path: &str,
    slot: u8,
) -> Result<(FileOpOutcome, String), String> {
    let slots = client
        .ram_slots(link)
        .ok_or("could not read the sample-slot table")?;
    if slots.iter().any(|s| s.id == u16::from(slot) && s.used) {
        return Err(format!("sample slot {slot:03} is already in use"));
    }

    client.ram_load_slot(link, drive_path, slot)?;

    let slots = client
        .ram_slots(link)
        .ok_or("load sent but verification read failed")?;
    let entry = slots
        .iter()
        .find(|s| s.id == u16::from(slot) && s.used)
        .ok_or("load sent but the slot still reads as empty")?;
    let name = drive_path
        .rsplit('/')
        .next()
        .unwrap_or(drive_path)
        .to_owned();
    let info = SampleSlotInfo {
        slot,
        name: name.clone(),
        path: drive_path.to_owned(),
        size: entry.size,
        hash: entry.hash,
    };
    Ok((
        FileOpOutcome::SampleLoaded(info),
        format!("{name} loaded into sample slot {slot:03} and verified"),
    ))
}
