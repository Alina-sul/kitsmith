//! The dashboard's own view of a Rytm project.
//!
//! rytm-rs structs mirror the wire format; this module distills them into the
//! small graph the UI needs — kits, sounds, patterns, and crucially the
//! **sample-slot usage index** (slot → which kit/track uses it), which is the
//! information the device itself never shows. Serializable, so it doubles as
//! the on-disk cache format.

pub mod edit;
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KitModel {
    pub index: usize,
    pub name: String,
    pub sounds: Vec<SoundModel>,
}

/// One Sound, reduced to what the dashboard cares about.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SoundModel {
    pub name: String,
    pub machine: String,
    /// Project sample slot (1–127) the SMPL page points at, if any.
    pub sample_slot: Option<u8>,
    /// Current values of the editable parameter pages. `None` in caches
    /// written before the kit editor existed — the editor is gated until a
    /// re-sync reads real values (never show fabricated zeros as state).
    #[serde(default)]
    pub params: Option<TrackParams>,
}

/// The machine-independent parameter values the kit editor can change,
/// captured at sync time so editors open on the device's current state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct TrackParams {
    pub level: i32,
    pub smpl_tune: i32,
    pub smpl_fine_tune: i32,
    pub smpl_bit_reduction: i32,
    pub smpl_start: f32,
    pub smpl_end: f32,
    pub smpl_loop: bool,
    pub smpl_level: i32,
    pub amp_attack: i32,
    pub amp_hold: i32,
    pub amp_decay: i32,
    pub amp_overdrive: i32,
    pub amp_delay_send: i32,
    pub amp_reverb_send: i32,
    pub amp_pan: i32,
    pub amp_volume: i32,
    pub flt_attack: i32,
    pub flt_sustain: i32,
    pub flt_decay: i32,
    pub flt_release: i32,
    pub flt_cutoff: i32,
    pub flt_resonance: i32,
    pub flt_env_amount: i32,
}

impl TrackParams {
    /// Reads the editable values off one kit-track Sound. `level` is the
    /// kit-side track level (pool sounds have none — pass 0).
    fn from_sound(sound: &rytm_rs::object::Sound, level: usize) -> Self {
        let sample = sound.sample();
        let amp = sound.amplitude();
        let filter = sound.filter();
        Self {
            level: level as i32,
            smpl_tune: sample.tune() as i32,
            smpl_fine_tune: sample.fine_tune() as i32,
            smpl_bit_reduction: sample.bit_reduction() as i32,
            smpl_start: sample.start(),
            smpl_end: sample.end(),
            smpl_loop: sample.loop_flag(),
            smpl_level: sample.volume() as i32,
            amp_attack: amp.attack() as i32,
            amp_hold: amp.hold() as i32,
            amp_decay: amp.decay() as i32,
            amp_overdrive: amp.overdrive() as i32,
            amp_delay_send: amp.delay_send() as i32,
            amp_reverb_send: amp.reverb_send() as i32,
            // rytm-rs's pan() getter sign-extends the i8 field into usize;
            // truncate back to one byte to recover the signed value.
            amp_pan: i32::from((amp.pan() as u8) as i8),
            amp_volume: amp.volume() as i32,
            flt_attack: filter.attack() as i32,
            flt_sustain: filter.sustain() as i32,
            flt_decay: filter.decay() as i32,
            flt_release: filter.release() as i32,
            flt_cutoff: filter.cutoff() as i32,
            flt_resonance: filter.resonance() as i32,
            flt_env_amount: filter.envelope_amount() as i32,
        }
    }

    /// The current value of an editable field, in the edit vocabulary.
    /// `None` for fields this struct does not carry (names).
    pub fn value(&self, field: edit::KitField) -> Option<edit::FieldValue> {
        use edit::{FieldValue, KitField};
        Some(match field {
            KitField::KitName | KitField::SoundName(_) => return None,
            KitField::TrackLevel(_) => FieldValue::Int(self.level),
            KitField::SampleTune(_) => FieldValue::Int(self.smpl_tune),
            KitField::SampleFineTune(_) => FieldValue::Int(self.smpl_fine_tune),
            KitField::SampleBitReduction(_) => FieldValue::Int(self.smpl_bit_reduction),
            KitField::SampleStart(_) => FieldValue::Float(self.smpl_start),
            KitField::SampleEnd(_) => FieldValue::Float(self.smpl_end),
            KitField::SampleLoop(_) => FieldValue::Toggle(self.smpl_loop),
            KitField::SampleLevel(_) => FieldValue::Int(self.smpl_level),
            KitField::AmpAttack(_) => FieldValue::Int(self.amp_attack),
            KitField::AmpHold(_) => FieldValue::Int(self.amp_hold),
            KitField::AmpDecay(_) => FieldValue::Int(self.amp_decay),
            KitField::AmpOverdrive(_) => FieldValue::Int(self.amp_overdrive),
            KitField::AmpDelaySend(_) => FieldValue::Int(self.amp_delay_send),
            KitField::AmpReverbSend(_) => FieldValue::Int(self.amp_reverb_send),
            KitField::AmpPan(_) => FieldValue::Int(self.amp_pan),
            KitField::AmpVolume(_) => FieldValue::Int(self.amp_volume),
            KitField::FltAttack(_) => FieldValue::Int(self.flt_attack),
            KitField::FltSustain(_) => FieldValue::Int(self.flt_sustain),
            KitField::FltDecay(_) => FieldValue::Int(self.flt_decay),
            KitField::FltRelease(_) => FieldValue::Int(self.flt_release),
            KitField::FltCutoff(_) => FieldValue::Int(self.flt_cutoff),
            KitField::FltResonance(_) => FieldValue::Int(self.flt_resonance),
            KitField::FltEnvAmount(_) => FieldValue::Int(self.flt_env_amount),
        })
    }
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
            .map(|kit| {
                let levels = kit.track_levels();
                KitModel {
                    index: kit.index(),
                    name: kit.name().to_owned(),
                    sounds: kit
                        .sounds()
                        .iter()
                        .take(TRACK_COUNT)
                        .enumerate()
                        .map(|(track, sound)| {
                            SoundModel::from_rytm(sound, levels.get(track).copied().unwrap_or(0))
                        })
                        .collect(),
                }
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
            .map(|sound| SoundModel::from_rytm(sound, 0))
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

    /// Everything known to reference `slot`: kit tracks, Sound Pool sounds
    /// (with the patterns Sound-locking them), and trig sample plocks.
    /// Only trustworthy for destructive decisions when [`Self::dependency_scan`]
    /// is true — old caches lack the pattern scan.
    pub fn slot_references(&self, slot: u8) -> SlotReferences {
        let kit_uses = self.slot_usage().remove(&slot).unwrap_or_default();
        let mut pool_uses = Vec::new();
        for (index, sound) in self.pool_sounds.iter().enumerate() {
            if sound.sample_slot != Some(slot) {
                continue;
            }
            let pool_index = index as u8;
            let patterns = self
                .patterns
                .iter()
                .filter(|p| p.sound_locks.contains(&pool_index))
                .map(|p| p.index)
                .collect();
            pool_uses.push(PoolUse {
                pool_index,
                sound_name: sound.name.clone(),
                patterns,
            });
        }
        let plock_patterns = self
            .patterns
            .iter()
            .filter(|p| p.plocked_sample_slots.contains(&slot))
            .map(|p| p.index)
            .collect();
        SlotReferences {
            kit_uses,
            pool_uses,
            plock_patterns,
        }
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
    fn from_rytm(sound: &rytm_rs::object::Sound, level: usize) -> Self {
        Self {
            name: sound.name().to_owned(),
            machine: format!("{:?}", sound.machine_type()),
            sample_slot: slot_from_wire(sound.sample().slice_number()),
            params: Some(TrackParams::from_sound(sound, level)),
        }
    }
}

/// Formats a pattern index (0–127) the way the device shows it: bank A–H + 01–16.
pub fn pattern_display_name(index: usize) -> String {
    let bank = (b'A' + (index / 16) as u8) as char;
    format!("{bank}{:02}", index % 16 + 1)
}
