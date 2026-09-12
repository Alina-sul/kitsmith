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

/// Fresh slot listing, as (id → has_valid_data) pairs for preflights.
fn list_slots(link: &mut MidiLink, client: &mut FileClient) -> Result<Vec<(u32, bool)>, String> {
    Ok(client
        .list_data_dir(link, PROJECTS_PATH)
        .ok_or("could not list project slots")?
        .into_iter()
        .filter_map(|e| e.id.map(|id| (id, e.has_valid_data)))
        .collect())
}

fn slot_is_valid(slots: &[(u32, bool)], id: u32) -> bool {
    slots.iter().any(|&(slot, valid)| slot == id && valid)
}

fn move_project(
    link: &mut MidiLink,
    client: &mut FileClient,
    src: u32,
    dst: u32,
) -> Result<(FileOpOutcome, String), String> {
    let entries = client
        .list_data_dir(link, PROJECTS_PATH)
        .ok_or("could not list project slots")?;
    let name = entries
        .iter()
        .find(|e| e.id == Some(src) && e.has_valid_data)
        .map(|e| e.name.clone())
        .ok_or(format!("source project {src:03} not found"))?;
    if entries
        .iter()
        .any(|e| e.id == Some(dst) && e.has_valid_data)
    {
        return Err(format!("slot {dst:03} is not empty — move refused"));
    }
    // The pre-move download is both the safety backup and the verify oracle.
    let original = backup_slot(link, client, src, "(pre-move)")?;

    client.move_data(
        link,
        &format!("{PROJECTS_PATH}/{src}"),
        &format!("{PROJECTS_PATH}/{dst}"),
    )?;

    let slots = list_slots(link, client)?;
    if slot_is_valid(&slots, src) || !slot_is_valid(&slots, dst) {
        return Err(format!(
            "move sent but the slots read back wrong (src {src:03} \
             occupied: {}, dst {dst:03} occupied: {}) — restore from the \
             local backup if needed",
            slot_is_valid(&slots, src),
            slot_is_valid(&slots, dst)
        ));
    }
    let moved = client
        .download_data(link, &format!("{PROJECTS_PATH}/{dst}"), false, |_, _| {})
        .ok_or("move verified by listing, but the verification download failed")?;
    if moved.bytes != original {
        return Err(format!(
            "project arrived at slot {dst:03} but differs from the original — \
             do not trust it; the pre-move backup of {src:03} is on disk"
        ));
    }
    // Index the payload under its new slot so it stays browsable.
    let _ = cache::save_project_backup(
        &cache::default_dir(),
        dst,
        &name,
        &moved.bytes,
        crc32(0, &moved.bytes),
        true,
    );
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
    let slots = list_slots(link, client)?;
    for slot in [a, b] {
        if !slot_is_valid(&slots, slot) {
            return Err(format!(
                "slot {slot:03} is empty — use Move instead of Swap"
            ));
        }
    }
    let payload_a = backup_slot(link, client, a, "(pre-swap)")?;
    let payload_b = backup_slot(link, client, b, "(pre-swap)")?;

    client.swap_data(
        link,
        &format!("{PROJECTS_PATH}/{a}"),
        &format!("{PROJECTS_PATH}/{b}"),
    )?;

    // Verify crosswise: each slot must now hold the other's exact bytes.
    for (slot, expected) in [(a, &payload_b), (b, &payload_a)] {
        let read = client
            .download_data(link, &format!("{PROJECTS_PATH}/{slot}"), false, |_, _| {})
            .ok_or("swap sent but a verification download failed")?;
        if &read.bytes != expected {
            return Err(format!(
                "swap sent but slot {slot:03} does not hold the expected \
                 project — both pre-swap backups are on disk"
            ));
        }
    }
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
    let slots = list_slots(link, client)?;
    if !slot_is_valid(&slots, slot) {
        return Err(format!("slot {slot:03} is already empty"));
    }
    // Mandatory backup; a failed save aborts before anything is sent.
    backup_slot(link, client, slot, "(pre-clear)")?;

    client.clear_data(link, &format!("{PROJECTS_PATH}/{slot}"))?;

    let slots = list_slots(link, client)?;
    if slot_is_valid(&slots, slot) {
        return Err(format!(
            "clear sent but slot {slot:03} still reads as occupied"
        ));
    }
    Ok((
        FileOpOutcome::Cleared { slot },
        format!("Project slot {slot:03} cleared — the pre-clear backup is on disk"),
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
