//! The dashboard's own view of a Rytm project.
//!
//! rytm-rs structs mirror the wire format; this module distills them into the
//! small graph the UI needs — kits, sounds, patterns, and crucially the
//! **sample-slot usage index** (slot → which kit/track uses it), which is the
//! information the device itself never shows. Serializable, so it doubles as
//! the on-disk cache format.

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

/// A distilled Rytm project.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ProjectModel {
    pub kits: Vec<KitModel>,
    pub patterns: Vec<PatternModel>,
    pub pool_sounds: Vec<SoundModel>,
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

/// One pattern: its index and the kit it links to.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PatternModel {
    pub index: usize,
    pub kit_index: usize,
}

/// One use of a sample slot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SlotUse {
    pub kit_index: usize,
    pub kit_name: String,
    pub track: usize,
    pub sound_name: String,
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
            .map(|p| PatternModel {
                index: p.index(),
                kit_index: p.kit_number(),
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
        let raw_slot = sound.sample().slice_number();
        Self {
            name: sound.name().to_owned(),
            machine: format!("{:?}", sound.machine_type()),
            sample_slot: (!SMP_OFF_VALUES.contains(&raw_slot))
                .then(|| u8::try_from(raw_slot).unwrap_or(u8::MAX))
                .filter(|&s| s <= SAMPLE_SLOT_COUNT),
        }
    }
}

/// Formats a pattern index (0–127) the way the device shows it: bank A–H + 01–16.
pub fn pattern_display_name(index: usize) -> String {
    let bank = (b'A' + (index / 16) as u8) as char;
    format!("{bank}{:02}", index % 16 + 1)
}
