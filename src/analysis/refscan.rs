//! Cross-project reference scan for one +Drive sample, by content identity.
//!
//! The join key is **hash + size** — the device tracks sample content by
//! CRC-like hash, and both the active project's RAM slot table and every
//! backed-up project's slot table carry it. A scan can only see what exists
//! locally: device projects that were never backed up are a blind spot the
//! UI must state, never paper over.

use crate::cache::ProjectManifest;
use crate::device::projfile;
use crate::model::{ProjectInfo, ProjectModel};

/// Everything known to reference one +Drive sample file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DriveRefScan {
    /// Active-project sample slots whose content matches.
    pub active_slots: Vec<u8>,
    /// Matches inside locally backed-up projects.
    pub backed_up: Vec<ProjectHit>,
    /// Device projects with no scannable local backup — the blind spot.
    pub unscanned: Vec<ProjectInfo>,
}

/// One backed-up project that references the sample.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectHit {
    pub slot_id: u32,
    pub name: String,
    /// The project's sample slots pointing at this content.
    pub sample_slots: Vec<u8>,
}

impl DriveRefScan {
    /// True when any known project (active or backed up) uses the sample.
    pub fn has_references(&self) -> bool {
        !self.active_slots.is_empty() || !self.backed_up.is_empty()
    }

    /// Total count of referencing slots, for `N uses` labels.
    pub fn reference_count(&self) -> usize {
        self.active_slots.len()
            + self
                .backed_up
                .iter()
                .map(|hit| hit.sample_slots.len())
                .sum::<usize>()
    }
}

/// Scans everything scannable for `hash`+`size`: the active model's slots and
/// every raw backed-up project payload served by `load`. `model.projects`
/// (the device slot list) determines what counts as unscanned; offline it is
/// empty and the blind spot is simply unknown.
pub fn scan(
    model: &ProjectModel,
    manifest: &ProjectManifest,
    load: impl Fn(u32) -> Option<Vec<u8>>,
    hash: u32,
    size: u32,
) -> DriveRefScan {
    let active_slots = model
        .samples
        .iter()
        .filter(|s| s.hash == hash && s.size == size)
        .map(|s| s.slot)
        .collect();

    let mut backed_up = Vec::new();
    let mut scanned = std::collections::BTreeSet::new();
    for (&slot_id, backup) in manifest {
        // Only the raw (uncompressed) payload format is parseable; compressed
        // backups from before the raw pipeline stay unscanned.
        if !backup.raw {
            continue;
        }
        let Some(snapshot) = load(slot_id).and_then(|bytes| projfile::parse_raw_project(&bytes))
        else {
            continue;
        };
        scanned.insert(slot_id);
        let sample_slots: Vec<u8> = snapshot
            .samples
            .iter()
            .filter(|s| s.hash == hash && s.size == size)
            .map(|s| s.slot)
            .collect();
        if !sample_slots.is_empty() {
            backed_up.push(ProjectHit {
                slot_id,
                name: backup.name.clone(),
                sample_slots,
            });
        }
    }

    let unscanned = model
        .projects
        .iter()
        .filter(|p| !scanned.contains(&p.id))
        .cloned()
        .collect();

    DriveRefScan {
        active_slots,
        backed_up,
        unscanned,
    }
}
