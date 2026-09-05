//! Cross-project pattern transfer (PRD Part IX, first slice).
//!
//! The device can only expose the *active* project to object sysex, so the
//! flow is a **clipboard**: capture a pattern + its kit + Sound-locked pool
//! sounds (fresh reads, raw sysex) while project A is active; the user loads
//! project B on the hardware and syncs; paste rewrites sample-slot references
//! to project B's slots (matched by +Drive content hash/path) and writes the
//! objects back — same slot numbers, explicit overwrite warnings, through the
//! backup-first Plan pipeline.
//!
//! v1 copies to the *same* pattern/kit/pool slot numbers in the target;
//! retargeting to different slots is a later milestone.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use rytm_rs::prelude::*;
use rytm_rs::RytmProject;
use serde::{Deserialize, Serialize};

use crate::device::objects::accept_reply;
use crate::midi::link::MidiLink;
use crate::model::{slot_from_wire, ProjectModel, SampleSlotInfo};

/// File name of the persisted pattern clipboard inside the cache directory.
const CLIPBOARD_FILE: &str = "pattern_clipboard.json";

/// File name of the persisted kit clipboard inside the cache directory.
const KIT_CLIPBOARD_FILE: &str = "kit_clipboard.json";

/// A sample slot the captured objects reference, with its +Drive identity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SampleDep {
    pub source_slot: u8,
    pub name: String,
    pub path: String,
    pub size: u32,
    pub hash: u32,
}

/// A captured pattern and everything it depends on, portable across project
/// loads. Object payloads are the raw sysex responses from the capture reads.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PatternClipboard {
    pub captured_at_unix: u64,
    /// Human hint for the UI, e.g. `B03 · Kit 02 DEUIL B`.
    pub source_hint: String,
    pub pattern_index: usize,
    pub kit_index: usize,
    pub kit_name: String,
    pub pattern_sysex: Vec<u8>,
    pub kit_sysex: Vec<u8>,
    /// Pool sounds selected by the pattern's Sound Locks: index → raw sysex.
    pub pool_sounds: BTreeMap<u8, Vec<u8>>,
    /// Every sample slot referenced by the kit, plocks or pool sounds.
    pub samples: Vec<SampleDep>,
}

/// Preflight result for pasting into the currently synced (target) project.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PastePreview {
    /// Source slot → matching target slot (same +Drive content).
    pub slot_remap: BTreeMap<u8, u8>,
    /// Dependencies with no matching sample in the target project. Their
    /// references are cleared (SMP off) rather than left silently wrong.
    pub missing: Vec<SampleDep>,
    /// `B03` — the target pattern that will be replaced.
    pub overwrite_pattern: String,
    /// Kit slot + its current name in the target project.
    pub overwrite_kit: String,
    /// Target patterns currently linked to the kit slot being replaced —
    /// replacing the kit affects them too (PRD §3.3).
    pub kit_also_used_by: Vec<String>,
    /// Pool slots that will be replaced in the target project.
    pub overwrite_pool: Vec<u8>,
}

/// One encoded object write plus the label shown in the plan preview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectWrite {
    pub label: String,
    pub sysex: Vec<u8>,
}

/// Everything the Plan needs to execute and verify a paste.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterializedPaste {
    pub writes: Vec<ObjectWrite>,
    pub kit_index: usize,
    /// Expected per-track sample slots after the write (read-back check).
    pub expected_kit_slots: Vec<Option<u8>>,
    pub pattern_index: usize,
}

/// A captured kit and its sample dependencies, portable across project loads.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KitClipboard {
    pub captured_at_unix: u64,
    /// Human hint for the UI, e.g. `Kit 02 DEUIL B`.
    pub source_hint: String,
    pub kit_index: usize,
    pub kit_name: String,
    pub kit_sysex: Vec<u8>,
    /// Every sample slot referenced by the kit's 12 track Sounds.
    pub samples: Vec<SampleDep>,
}

/// Preflight result for pasting a kit into the synced target project.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KitPastePreview {
    pub slot_remap: BTreeMap<u8, u8>,
    pub missing: Vec<SampleDep>,
    /// Kit slot + its current name in the target project.
    pub overwrite_kit: String,
    /// Target patterns linked to the kit slot being replaced.
    pub kit_used_by: Vec<String>,
}

/// Everything the Plan needs to execute and verify a kit paste.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterializedKitPaste {
    pub write: ObjectWrite,
    pub kit_index: usize,
    pub expected_kit_slots: Vec<Option<u8>>,
}

/// Captures one kit with a **fresh** device read; `model` supplies the
/// +Drive identities of the sample slots its sounds reference.
pub fn capture_kit(
    link: &mut MidiLink,
    model: &ProjectModel,
    kit_index: usize,
) -> Result<KitClipboard, String> {
    let mut rytm = RytmProject::try_default().map_err(|e| e.to_string())?;
    let kit_sysex = query_object(
        link,
        &mut rytm,
        KitQuery::new(kit_index)
            .and_then(|q| q.as_sysex())
            .map_err(|e| e.to_string())?,
        "kit",
    )?;
    let kit = rytm
        .kits()
        .get(kit_index)
        .ok_or("kit missing after decode")?;
    let kit_name = kit.name().to_owned();
    let mut slots: BTreeSet<u8> = BTreeSet::new();
    for sound in kit.sounds().iter().take(12) {
        if let Some(slot) = slot_from_wire(sound.sample().slice_number()) {
            slots.insert(slot);
        }
    }
    Ok(KitClipboard {
        captured_at_unix: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        source_hint: format!("Kit {:02} {}", kit_index + 1, kit_name.trim()),
        kit_index,
        kit_name,
        kit_sysex,
        samples: resolve_sample_deps(model, slots),
    })
}

/// Builds the kit-paste preview against the synced target project (pure).
pub fn plan_paste_kit(clip: &KitClipboard, target: &ProjectModel) -> KitPastePreview {
    let mut slot_remap = BTreeMap::new();
    let mut missing = Vec::new();
    for dep in &clip.samples {
        match match_sample(dep, &target.samples) {
            Some(slot) => {
                slot_remap.insert(dep.source_slot, slot);
            }
            None => missing.push(dep.clone()),
        }
    }
    let overwrite_kit = target
        .kits
        .get(clip.kit_index)
        .map(|k| {
            format!(
                "Kit {:02} {}",
                clip.kit_index + 1,
                crate::model::names::kit_display_name(&k.name, k.index).text()
            )
        })
        .unwrap_or_else(|| format!("Kit {:02}", clip.kit_index + 1));
    let kit_used_by = target
        .patterns
        .iter()
        .filter(|p| p.kit_index == clip.kit_index)
        .map(|p| crate::model::pattern_display_name(p.index))
        .collect();
    KitPastePreview {
        slot_remap,
        missing,
        overwrite_kit,
        kit_used_by,
    }
}

/// Decodes the captured kit, rewrites its sample references to the target
/// project's slots, and re-encodes it for writing (pure CPU).
pub fn materialize_kit(
    clip: &KitClipboard,
    preview: &KitPastePreview,
) -> Result<MaterializedKitPaste, String> {
    let mut rytm = RytmProject::try_default().map_err(|e| e.to_string())?;
    rytm.update_from_sysex_response(&clip.kit_sysex)
        .map_err(|e| format!("captured kit no longer decodes: {e}"))?;
    let kit = rytm
        .kits_mut()
        .get_mut(clip.kit_index)
        .ok_or("captured kit missing after decode")?;
    let mut expected_kit_slots = Vec::with_capacity(12);
    for sound in kit.sounds_mut().iter_mut().take(12) {
        let raw = sound.sample().slice_number();
        match slot_from_wire(raw).and_then(|src| preview.slot_remap.get(&src).copied()) {
            Some(slot) => {
                sound
                    .sample_mut()
                    .set_slice_number(usize::from(slot))
                    .map_err(|e| e.to_string())?;
                expected_kit_slots.push(Some(slot));
            }
            None => {
                if slot_from_wire(raw).is_some() {
                    sound.sample_mut().unset_slice();
                }
                expected_kit_slots.push(None);
            }
        }
    }
    Ok(MaterializedKitPaste {
        write: ObjectWrite {
            label: format!("write kit {:02}", clip.kit_index + 1),
            sysex: kit.as_sysex().map_err(|e| e.to_string())?,
        },
        kit_index: clip.kit_index,
        expected_kit_slots,
    })
}

/// Resolves a set of source slots to dependencies with +Drive identity.
fn resolve_sample_deps(model: &ProjectModel, slots: BTreeSet<u8>) -> Vec<SampleDep> {
    slots
        .into_iter()
        .map(|slot| {
            model.samples.iter().find(|s| s.slot == slot).map_or_else(
                || SampleDep {
                    source_slot: slot,
                    name: format!("slot {slot:03} (name unavailable)"),
                    path: String::new(),
                    size: 0,
                    hash: 0,
                },
                |s| SampleDep {
                    source_slot: slot,
                    name: s.name.clone(),
                    path: s.path.clone(),
                    size: s.size,
                    hash: s.hash,
                },
            )
        })
        .collect()
}

/// Persists the kit clipboard.
pub fn save_kit(clip: &KitClipboard) -> std::io::Result<()> {
    let dir = crate::cache::default_dir();
    std::fs::create_dir_all(&dir)?;
    let json = serde_json::to_vec_pretty(clip).map_err(std::io::Error::other)?;
    std::fs::write(dir.join(KIT_CLIPBOARD_FILE), json)
}

/// Loads the persisted kit clipboard, if any.
pub fn load_kit() -> Option<KitClipboard> {
    let bytes = std::fs::read(crate::cache::default_dir().join(KIT_CLIPBOARD_FILE)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Removes the persisted kit clipboard.
pub fn clear_saved_kit() {
    let _ = std::fs::remove_file(crate::cache::default_dir().join(KIT_CLIPBOARD_FILE));
}

/// Captures a pattern + its kit + Sound-locked pool sounds with **fresh**
/// device reads (PRD §30: never trust a stale sync for a transfer source).
/// `model` supplies the +Drive identities of referenced sample slots.
pub fn capture(
    link: &mut MidiLink,
    model: &ProjectModel,
    pattern_index: usize,
) -> Result<PatternClipboard, String> {
    let mut rytm = RytmProject::try_default().map_err(|e| e.to_string())?;

    let pattern_sysex = query_object(
        link,
        &mut rytm,
        PatternQuery::new(pattern_index)
            .and_then(|q| q.as_sysex())
            .map_err(|e| e.to_string())?,
        "pattern",
    )?;
    let pattern = rytm
        .patterns()
        .get(pattern_index)
        .ok_or("pattern missing after decode")?;
    let kit_index = pattern.kit_number();
    if kit_index > 127 {
        return Err(format!(
            "pattern links to kit {kit_index} — not a saved kit slot"
        ));
    }

    // Which pool sounds do the pattern's Sound Locks select, and which
    // sample slots do its plocks touch?
    let mut pool_indices: BTreeSet<u8> = BTreeSet::new();
    let mut slots: BTreeSet<u8> = BTreeSet::new();
    for track in pattern.tracks() {
        for trig in track.trigs() {
            if let Ok(lock) = u8::try_from(trig.sound_lock()) {
                if lock <= 127 {
                    pool_indices.insert(lock);
                }
            }
            if let Ok(Some(raw)) = trig.plock_get_sample_number() {
                if let Some(slot) = slot_from_wire(raw) {
                    slots.insert(slot);
                }
            }
        }
    }

    let kit_sysex = query_object(
        link,
        &mut rytm,
        KitQuery::new(kit_index)
            .and_then(|q| q.as_sysex())
            .map_err(|e| e.to_string())?,
        "kit",
    )?;
    let kit = rytm
        .kits()
        .get(kit_index)
        .ok_or("kit missing after decode")?;
    let kit_name = kit.name().to_owned();
    for sound in kit.sounds().iter().take(12) {
        if let Some(slot) = slot_from_wire(sound.sample().slice_number()) {
            slots.insert(slot);
        }
    }

    let mut pool_sounds = BTreeMap::new();
    for &pool_index in &pool_indices {
        let sysex = query_object(
            link,
            &mut rytm,
            SoundQuery::new(usize::from(pool_index))
                .and_then(|q| q.as_sysex())
                .map_err(|e| e.to_string())?,
            "pool sound",
        )?;
        if let Some(sound) = rytm.pool_sounds().get(usize::from(pool_index)) {
            if let Some(slot) = slot_from_wire(sound.sample().slice_number()) {
                slots.insert(slot);
            }
        }
        pool_sounds.insert(pool_index, sysex);
    }

    let samples = resolve_sample_deps(model, slots);

    Ok(PatternClipboard {
        captured_at_unix: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        source_hint: format!(
            "{} · Kit {:02} {}",
            crate::model::pattern_display_name(pattern_index),
            kit_index + 1,
            kit_name.trim()
        ),
        pattern_index,
        kit_index,
        kit_name,
        pattern_sysex,
        kit_sysex,
        pool_sounds,
        samples,
    })
}

/// Sends one object query and returns the accepted raw reply.
fn query_object(
    link: &mut MidiLink,
    rytm: &mut RytmProject,
    query: Vec<u8>,
    what: &str,
) -> Result<Vec<u8>, String> {
    link.send(&query).map_err(|e| e.to_string())?;
    accept_reply(link, rytm).ok_or_else(|| format!("{what} query got no reply"))
}

/// Builds the paste preview against the synced target project (pure).
pub fn plan_paste(clip: &PatternClipboard, target: &ProjectModel) -> PastePreview {
    let mut slot_remap = BTreeMap::new();
    let mut missing = Vec::new();
    for dep in &clip.samples {
        match match_sample(dep, &target.samples) {
            Some(slot) => {
                slot_remap.insert(dep.source_slot, slot);
            }
            None => missing.push(dep.clone()),
        }
    }
    let overwrite_kit = target
        .kits
        .get(clip.kit_index)
        .map(|k| {
            format!(
                "Kit {:02} {}",
                clip.kit_index + 1,
                crate::model::names::kit_display_name(&k.name, k.index).text()
            )
        })
        .unwrap_or_else(|| format!("Kit {:02}", clip.kit_index + 1));
    let kit_also_used_by = target
        .patterns
        .iter()
        .filter(|p| p.kit_index == clip.kit_index && p.index != clip.pattern_index)
        .map(|p| crate::model::pattern_display_name(p.index))
        .collect();
    PastePreview {
        slot_remap,
        missing,
        overwrite_pattern: crate::model::pattern_display_name(clip.pattern_index),
        overwrite_kit,
        kit_also_used_by,
        overwrite_pool: clip.pool_sounds.keys().copied().collect(),
    }
}

/// Matches a dependency to a target slot: by content hash+size when the
/// capture knew the hash, by +Drive path otherwise.
fn match_sample(dep: &SampleDep, target_samples: &[SampleSlotInfo]) -> Option<u8> {
    if dep.hash != 0 {
        if let Some(hit) = target_samples
            .iter()
            .find(|s| s.hash == dep.hash && s.size == dep.size)
        {
            return Some(hit.slot);
        }
    }
    target_samples
        .iter()
        .find(|s| !dep.path.is_empty() && s.path == dep.path)
        .map(|s| s.slot)
}

/// Decodes the captured objects, rewrites their sample references to the
/// target project's slots, and re-encodes them for writing (pure CPU).
pub fn materialize(
    clip: &PatternClipboard,
    preview: &PastePreview,
) -> Result<MaterializedPaste, String> {
    let mut rytm = RytmProject::try_default().map_err(|e| e.to_string())?;
    for sysex in std::iter::once(&clip.pattern_sysex)
        .chain(std::iter::once(&clip.kit_sysex))
        .chain(clip.pool_sounds.values())
    {
        rytm.update_from_sysex_response(sysex)
            .map_err(|e| format!("captured object no longer decodes: {e}"))?;
    }

    let remap = |raw: usize| -> Option<u8> {
        slot_from_wire(raw).and_then(|src| preview.slot_remap.get(&src).copied())
    };

    let mut writes = Vec::new();

    // Pool sounds first — the pattern's Sound Locks point at them.
    for &pool_index in clip.pool_sounds.keys() {
        let sound = rytm
            .pool_sounds_mut()
            .get_mut(usize::from(pool_index))
            .ok_or_else(|| format!("pool sound {pool_index} out of range"))?;
        let raw = sound.sample().slice_number();
        match remap(raw) {
            Some(slot) => sound
                .sample_mut()
                .set_slice_number(usize::from(slot))
                .map_err(|e| e.to_string())?,
            None => {
                if slot_from_wire(raw).is_some() {
                    sound.sample_mut().unset_slice();
                }
            }
        }
        writes.push(ObjectWrite {
            label: format!("write pool sound {:03}", pool_index + 1),
            sysex: sound.as_sysex().map_err(|e| e.to_string())?,
        });
    }

    // The kit: remap every track sound's sample slot.
    let mut expected_kit_slots = Vec::with_capacity(12);
    {
        let kit = rytm
            .kits_mut()
            .get_mut(clip.kit_index)
            .ok_or("captured kit missing after decode")?;
        for sound in kit.sounds_mut().iter_mut().take(12) {
            let raw = sound.sample().slice_number();
            match remap(raw) {
                Some(slot) => {
                    sound
                        .sample_mut()
                        .set_slice_number(usize::from(slot))
                        .map_err(|e| e.to_string())?;
                    expected_kit_slots.push(Some(slot));
                }
                None => {
                    if slot_from_wire(raw).is_some() {
                        sound.sample_mut().unset_slice();
                    }
                    expected_kit_slots.push(None);
                }
            }
        }
        writes.push(ObjectWrite {
            label: format!("write kit {:02}", clip.kit_index + 1),
            sysex: kit.as_sysex().map_err(|e| e.to_string())?,
        });
    }

    // The pattern: remap per-trig sample plocks; Sound Locks keep their pool
    // indices (same-slot copy). Cleared when the sample has no target match.
    {
        let pattern = rytm
            .patterns_mut()
            .get_mut(clip.pattern_index)
            .ok_or("captured pattern missing after decode")?;
        for track in pattern.tracks_mut() {
            for trig in track.trigs_mut() {
                let Ok(Some(raw)) = trig.plock_get_sample_number() else {
                    continue;
                };
                match remap(raw) {
                    Some(slot) => {
                        let _ = trig.plock_set_sample_number(usize::from(slot));
                    }
                    None => {
                        let _ = trig.plock_clear_sample_number();
                    }
                }
            }
        }
        writes.push(ObjectWrite {
            label: format!(
                "write pattern {}",
                crate::model::pattern_display_name(clip.pattern_index)
            ),
            sysex: pattern.as_sysex().map_err(|e| e.to_string())?,
        });
    }

    Ok(MaterializedPaste {
        writes,
        kit_index: clip.kit_index,
        expected_kit_slots,
        pattern_index: clip.pattern_index,
    })
}

/// Persists the clipboard beside the cache (survives app restarts and the
/// project switch on the device).
pub fn save(clip: &PatternClipboard) -> std::io::Result<()> {
    let dir = crate::cache::default_dir();
    std::fs::create_dir_all(&dir)?;
    let json = serde_json::to_vec_pretty(clip).map_err(std::io::Error::other)?;
    std::fs::write(dir.join(CLIPBOARD_FILE), json)
}

/// Loads the persisted clipboard, if any.
pub fn load() -> Option<PatternClipboard> {
    let bytes = std::fs::read(crate::cache::default_dir().join(CLIPBOARD_FILE)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Removes the persisted clipboard.
pub fn clear_saved() {
    let _ = std::fs::remove_file(crate::cache::default_dir().join(CLIPBOARD_FILE));
}
