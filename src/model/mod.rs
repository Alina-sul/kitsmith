//! The dashboard's own view of a Rytm project.
//!
//! rytm-rs structs mirror the wire format; this module distills them into the
//! small graph the UI needs — kits, sounds, patterns, and crucially the
//! **sample-slot usage index** (slot → which kit/track uses it), which is the
//! information the device itself never shows. Serializable, so it doubles as
//! the on-disk cache format.

pub mod names;

use std::collections::BTreeMap;

use rytm_rs::RytmProject;
use serde::{Deserialize, Serialize};

/// Number of sample slots in a project (1–127; the device shows no slot 0).
pub const SAMPLE_SLOT_COUNT: u8 = 127;

/// Drum track count (pads); a kit carries one Sound per track.
pub const TRACK_COUNT: usize = 12;

/// Sample-page wire values that mean "no sample assigned" (SMP OFF).
///
/// 0xFF is what rytm-rs writes for SMP OFF; 0 is the factory-default value.
/// Verify on hardware whether 0 can ever be a real slot (tracked in the M2 PR).
const SMP_OFF_VALUES: [usize; 2] = [0, 0xFF];

/// Interprets a raw SMPL-page slice number as a project sample slot.
pub fn slot_from_wire(raw: usize) -> Option<u8> {
    (!SMP_OFF_VALUES.contains(&raw))
        .then(|| u8::try_from(raw).unwrap_or(u8::MAX))
        .filter(|&s| s <= SAMPLE_SLOT_COUNT)
}

/// A distilled Rytm project.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ProjectModel {
    pub kits: Vec<KitModel>,
    pub patterns: Vec<PatternModel>,
    pub pool_sounds: Vec<SoundModel>,
    /// Names of the filled RAM sample slots (file protocol; empty until a
    /// sync with file-protocol support ran).
    #[serde(default)]
    pub samples: Vec<SampleSlotInfo>,
    /// The +Drive project slots (file protocol).
    #[serde(default)]
    pub projects: Vec<ProjectInfo>,
    /// True when patterns were scanned for Sound Locks and sample plocks.
    /// Old caches deserialize as `false` — the dependency data is then
    /// incomplete and destructive conclusions must not be drawn from it.
    #[serde(default)]
    pub dependency_scan: bool,
}

/// One filled sample slot with its +Drive identity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SampleSlotInfo {
    pub slot: u8,
    /// File name without directories (display name).
    pub name: String,
    /// Full +Drive path.
    pub path: String,
    pub size: u32,
    /// Device content hash (from the RAM slot table) — the stable sample
    /// identity used to match slots across projects. 0 in old caches.
    #[serde(default)]
    pub hash: u32,
}

/// One +Drive project slot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectInfo {
    pub id: u32,
    pub name: String,
}

/// One kit: 12 track Sounds + a name.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KitModel {
    pub index: usize,
    pub name: String,
    pub sounds: Vec<SoundModel>,
}

/// One Sound, reduced to what the dashboard cares about.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SoundModel {
    pub name: String,
    pub machine: String,
    /// Project sample slot (1–127) the SMPL page points at, if any.
    pub sample_slot: Option<u8>,
}

/// One pattern: its index, the kit it links to, and its sample dependencies.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PatternModel {
    pub index: usize,
    pub kit_index: usize,
    /// Sound Pool slots selected by trig Sound Locks (deduped, sorted).
    #[serde(default)]
    pub sound_locks: Vec<u8>,
    /// Sample slots selected directly by trig sample plocks (deduped, sorted).
    #[serde(default)]
    pub plocked_sample_slots: Vec<u8>,
}

/// One use of a sample slot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SlotUse {
    pub kit_index: usize,
    pub kit_name: String,
    pub track: usize,
    pub sound_name: String,
}

/// A Sound Pool entry that references a sample slot, plus the patterns that
/// select it via trig Sound Locks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolUse {
    pub pool_index: u8,
    pub sound_name: String,
    /// Pattern indices whose Sound Locks select this pool sound.
    pub patterns: Vec<usize>,
}

/// Everything known to reference one sample slot (PRD §3.6: the full graph is
/// kit tracks + Sound Pool sounds reachable via Sound Locks + sample plocks).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SlotReferences {
    pub kit_uses: Vec<SlotUse>,
    pub pool_uses: Vec<PoolUse>,
    /// Pattern indices with a trig sample plock pointing at this slot.
    pub plock_patterns: Vec<usize>,
}

impl SlotReferences {
    pub fn is_empty(&self) -> bool {
        self.kit_uses.is_empty() && self.pool_uses.is_empty() && self.plock_patterns.is_empty()
    }

    /// Total count of distinct references, for `N uses` labels.
    pub fn count(&self) -> usize {
        self.kit_uses.len() + self.pool_uses.len() + self.plock_patterns.len()
    }
}

impl ProjectModel {
    /// Distills a synced `RytmProject` into the dashboard model.
    pub fn from_rytm(rytm: &RytmProject) -> Self {
        let kits = rytm
            .kits()
            .iter()
            .map(|kit| KitModel {
                index: kit.index(),
                name: kit.name().to_owned(),
                sounds: kit
                    .sounds()
                    .iter()
                    .take(TRACK_COUNT)
                    .map(SoundModel::from_rytm)
                    .collect(),
            })
            .collect();
        let patterns = rytm
            .patterns()
            .iter()
            .map(|p| {
                let mut sound_locks = Vec::new();
                let mut plocked_sample_slots = Vec::new();
                for track in p.tracks() {
                    for trig in track.trigs() {
                        // 0xFF = no Sound Lock on this step.
                        if let Ok(lock) = u8::try_from(trig.sound_lock()) {
                            if lock <= 127 {
                                sound_locks.push(lock);
                            }
                        }
                        // A sample plock re-points the step at another slot.
                        if let Ok(Some(raw)) = trig.plock_get_sample_number() {
                            if let Some(slot) = slot_from_wire(raw) {
                                plocked_sample_slots.push(slot);
                            }
                        }
                    }
                }
                sound_locks.sort_unstable();
                sound_locks.dedup();
                plocked_sample_slots.sort_unstable();
                plocked_sample_slots.dedup();
                PatternModel {
                    index: p.index(),
                    kit_index: p.kit_number(),
                    sound_locks,
                    plocked_sample_slots,
                }
            })
            .collect();
        let pool_sounds = rytm
            .pool_sounds()
            .iter()
            .map(SoundModel::from_rytm)
            .collect();
        Self {
            kits,
            patterns,
            pool_sounds,
            // Filled by the file-protocol enrichment pass after a sync.
            samples: Vec::new(),
            projects: Vec::new(),
            dependency_scan: true,
        }
    }

    /// Slot → every kit/track Sound that references it, ordered by slot.
    pub fn slot_usage(&self) -> BTreeMap<u8, Vec<SlotUse>> {
        let mut usage: BTreeMap<u8, Vec<SlotUse>> = BTreeMap::new();
        for kit in &self.kits {
            for (track, sound) in kit.sounds.iter().enumerate() {
                if let Some(slot) = sound.sample_slot {
                    usage.entry(slot).or_default().push(SlotUse {
                        kit_index: kit.index,
                        kit_name: kit.name.clone(),
                        track,
                        sound_name: sound.name.clone(),
                    });
                }
            }
        }
        usage
    }

    /// Display name of a sample slot, when the file protocol resolved one.
    pub fn sample_name(&self, slot: u8) -> Option<&str> {
        self.samples
            .iter()
            .find(|s| s.slot == slot)
            .map(|s| s.name.as_str())
    }

    /// The whole sample dependency graph, indexed by slot: kit tracks, Sound
    /// Pool sounds (with the patterns Sound-locking them), and trig sample
    /// plocks (PRD §3.6). The single definition of what "references a slot"
    /// means — only trustworthy for destructive decisions when
    /// [`Self::dependency_scan`] is true, since old caches lack the scan.
    pub fn slot_reference_map(&self) -> BTreeMap<u8, SlotReferences> {
        let mut map: BTreeMap<u8, SlotReferences> = BTreeMap::new();
        for (slot, kit_uses) in self.slot_usage() {
            map.entry(slot).or_default().kit_uses = kit_uses;
        }
        for (index, sound) in self.pool_sounds.iter().enumerate() {
            let Some(slot) = sound.sample_slot else {
                continue;
            };
            let pool_index = index as u8;
            map.entry(slot).or_default().pool_uses.push(PoolUse {
                pool_index,
                sound_name: sound.name.clone(),
                patterns: self.patterns_sound_locking(pool_index),
            });
        }
        for pattern in &self.patterns {
            for slot in &pattern.plocked_sample_slots {
                map.entry(*slot)
                    .or_default()
                    .plock_patterns
                    .push(pattern.index);
            }
        }
        map
    }

    /// Everything known to reference one `slot` — see [`Self::slot_reference_map`].
    pub fn slot_references(&self, slot: u8) -> SlotReferences {
        self.slot_reference_map().remove(&slot).unwrap_or_default()
    }

    /// Whether `slot` can be unloaded without breaking anything the graph
    /// knows about. `None` until the dependency scan is complete — never
    /// treat incomplete data as "unused" (PRD §22).
    pub fn safe_to_unload(&self, slot: u8) -> Option<bool> {
        self.dependency_scan
            .then(|| self.slot_references(slot).is_empty())
    }

    /// Pattern indices whose Sound Locks select the given pool sound.
    pub fn patterns_sound_locking(&self, pool_index: u8) -> Vec<usize> {
        self.patterns
            .iter()
            .filter(|p| p.sound_locks.contains(&pool_index))
            .map(|p| p.index)
            .collect()
    }

    /// Kits that at least one pattern links to.
    pub fn linked_kit_indices(&self) -> Vec<usize> {
        let mut linked: Vec<usize> = self.patterns.iter().map(|p| p.kit_index).collect();
        linked.sort_unstable();
        linked.dedup();
        linked
    }
}

impl SoundModel {
    fn from_rytm(sound: &rytm_rs::object::Sound) -> Self {
        Self {
            name: sound.name().to_owned(),
            machine: format!("{:?}", sound.machine_type()),
            sample_slot: slot_from_wire(sound.sample().slice_number()),
        }
    }
}

/// Formats a pattern index (0–127) the way the device shows it: bank A–H + 01–16.
pub fn pattern_display_name(index: usize) -> String {
    let bank = (b'A' + (index / 16) as u8) as char;
    format!("{bank}{:02}", index % 16 + 1)
}
