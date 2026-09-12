//! Kit edits as pure data.
//!
//! The GUI builds [`KitEdit`]s against the model; `ops/` compiles them into a
//! transaction; only `device/kit_edit.rs` translates them to rytm-rs calls.
//! Every edit names a [`KitField`] (also the read-back verification key) and
//! carries a [`FieldValue`], so "apply" and "verify" share one vocabulary.

use serde::{Deserialize, Serialize};

use super::TRACK_COUNT;

/// Longest kit/Sound name the device accepts (ASCII).
pub const NAME_MAX_LEN: usize = 15;

/// Sample start/end travel as u16 (0..=30720) for an f32 range of 0.0..=120.0,
/// so a read-back can differ from the written value by up to one wire step
/// (120/30720 ≈ 0.0039). Values closer than this are the same position.
pub const SAMPLE_POS_TOLERANCE: f32 = 0.005;

/// One editable field of a kit. `usize` payloads are the drum track (0–11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KitField {
    KitName,
    SoundName(usize),
    TrackLevel(usize),
    SampleTune(usize),
    SampleFineTune(usize),
    SampleBitReduction(usize),
    SampleStart(usize),
    SampleEnd(usize),
    SampleLoop(usize),
    SampleLevel(usize),
    AmpAttack(usize),
    AmpHold(usize),
    AmpDecay(usize),
    AmpOverdrive(usize),
    AmpDelaySend(usize),
    AmpReverbSend(usize),
    AmpPan(usize),
    AmpVolume(usize),
    FltAttack(usize),
    FltSustain(usize),
    FltDecay(usize),
    FltRelease(usize),
    FltCutoff(usize),
    FltResonance(usize),
    FltEnvAmount(usize),
}

/// A value a [`KitField`] can hold.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FieldValue {
    Text(String),
    Int(i32),
    Float(f32),
    Toggle(bool),
}

impl std::fmt::Display for FieldValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Text(s) => write!(f, "\"{}\"", s.trim()),
            Self::Int(v) => write!(f, "{v}"),
            Self::Float(v) => write!(f, "{v:.2}"),
            Self::Toggle(true) => write!(f, "on"),
            Self::Toggle(false) => write!(f, "off"),
        }
    }
}

impl KitField {
    /// The drum track this field belongs to (`None` = whole-kit field).
    pub fn track(self) -> Option<usize> {
        match self {
            Self::KitName => None,
            Self::SoundName(t)
            | Self::TrackLevel(t)
            | Self::SampleTune(t)
            | Self::SampleFineTune(t)
            | Self::SampleBitReduction(t)
            | Self::SampleStart(t)
            | Self::SampleEnd(t)
            | Self::SampleLoop(t)
            | Self::SampleLevel(t)
            | Self::AmpAttack(t)
            | Self::AmpHold(t)
            | Self::AmpDecay(t)
            | Self::AmpOverdrive(t)
            | Self::AmpDelaySend(t)
            | Self::AmpReverbSend(t)
            | Self::AmpPan(t)
            | Self::AmpVolume(t)
            | Self::FltAttack(t)
            | Self::FltSustain(t)
            | Self::FltDecay(t)
            | Self::FltRelease(t)
            | Self::FltCutoff(t)
            | Self::FltResonance(t)
            | Self::FltEnvAmount(t) => Some(t),
        }
    }

    /// The parameter's name as the device labels it.
    pub fn label(self) -> &'static str {
        match self {
            Self::KitName => "kit name",
            Self::SoundName(_) => "Sound name",
            Self::TrackLevel(_) => "track level",
            Self::SampleTune(_) => "SMPL tune",
            Self::SampleFineTune(_) => "SMPL fine tune",
            Self::SampleBitReduction(_) => "SMPL bit reduction",
            Self::SampleStart(_) => "SMPL start",
            Self::SampleEnd(_) => "SMPL end",
            Self::SampleLoop(_) => "SMPL loop",
            Self::SampleLevel(_) => "SMPL level",
            Self::AmpAttack(_) => "AMP attack",
            Self::AmpHold(_) => "AMP hold",
            Self::AmpDecay(_) => "AMP decay",
            Self::AmpOverdrive(_) => "AMP overdrive",
            Self::AmpDelaySend(_) => "AMP delay send",
            Self::AmpReverbSend(_) => "AMP reverb send",
            Self::AmpPan(_) => "AMP pan",
            Self::AmpVolume(_) => "AMP volume",
            Self::FltAttack(_) => "FLTR attack",
            Self::FltSustain(_) => "FLTR sustain",
            Self::FltDecay(_) => "FLTR decay",
            Self::FltRelease(_) => "FLTR release",
            Self::FltCutoff(_) => "FLTR cutoff",
            Self::FltResonance(_) => "FLTR resonance",
            Self::FltEnvAmount(_) => "FLTR env amount",
        }
    }

    /// Inclusive value range for integer fields (`None` = not an integer).
    pub fn int_range(self) -> Option<(i32, i32)> {
        match self {
            Self::KitName | Self::SoundName(_) | Self::SampleLoop(_) => None,
            Self::SampleStart(_) | Self::SampleEnd(_) => None,
            Self::SampleTune(_) => Some((-24, 24)),
            Self::SampleFineTune(_) | Self::AmpPan(_) | Self::FltEnvAmount(_) => Some((-64, 63)),
            Self::TrackLevel(_)
            | Self::SampleBitReduction(_)
            | Self::SampleLevel(_)
            | Self::AmpAttack(_)
            | Self::AmpHold(_)
            | Self::AmpDecay(_)
            | Self::AmpOverdrive(_)
            | Self::AmpDelaySend(_)
            | Self::AmpReverbSend(_)
            | Self::AmpVolume(_)
            | Self::FltAttack(_)
            | Self::FltSustain(_)
            | Self::FltDecay(_)
            | Self::FltRelease(_)
            | Self::FltCutoff(_)
            | Self::FltResonance(_) => Some((0, 127)),
        }
    }

    /// Inclusive range for float fields (`None` = not a float).
    pub fn float_range(self) -> Option<(f32, f32)> {
        matches!(self, Self::SampleStart(_) | Self::SampleEnd(_)).then_some((0.0, 120.0))
    }
}

/// One requested change to a kit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KitEdit {
    pub field: KitField,
    pub value: FieldValue,
}

impl KitEdit {
    /// Checks the value against the field's type and range, without a device.
    pub fn validate(&self) -> Result<(), String> {
        let err = |what: &str| {
            Err(format!(
                "{}: {what} (got {})",
                self.field.label(),
                self.value
            ))
        };
        match (&self.value, self.field) {
            (FieldValue::Text(name), KitField::KitName | KitField::SoundName(_)) => {
                if !name.is_ascii() {
                    return err("name must be ASCII");
                }
                if name.len() > NAME_MAX_LEN {
                    return err("name is longer than 15 characters");
                }
                Ok(())
            }
            (FieldValue::Toggle(_), KitField::SampleLoop(_)) => Ok(()),
            (FieldValue::Float(v), field) => match field.float_range() {
                Some((lo, hi)) if (lo..=hi).contains(v) => Ok(()),
                Some((lo, hi)) => err(&format!("must be within {lo}–{hi}")),
                None => err("field does not take a float"),
            },
            (FieldValue::Int(v), field) => match field.int_range() {
                Some((lo, hi)) if (lo..=hi).contains(v) => Ok(()),
                Some((lo, hi)) => err(&format!("must be within {lo}–{hi}")),
                None => err("field does not take an integer"),
            },
            _ => err("wrong value type for this field"),
        }
    }

    /// One human-readable preview line, e.g. `A1 · AMP decay → 64`.
    pub fn describe(&self) -> String {
        match self.field.track() {
            Some(track) => format!(
                "{} · {} → {}",
                super::names::track_label(track),
                self.field.label(),
                self.value
            ),
            None => format!("{} → {}", self.field.label(), self.value),
        }
    }

    /// Whether `read` (a device read-back) matches this edit's value.
    /// Floats compare within [`SAMPLE_POS_TOLERANCE`] (wire quantization).
    pub fn value_matches(&self, read: &FieldValue) -> bool {
        match (&self.value, read) {
            (FieldValue::Float(a), FieldValue::Float(b)) => (a - b).abs() < SAMPLE_POS_TOLERANCE,
            // Device names are NUL-padded to 15 bytes; compare the text.
            (FieldValue::Text(a), FieldValue::Text(b)) => {
                a.trim_end_matches('\0') == b.trim_end_matches('\0')
            }
            (a, b) => a == b,
        }
    }
}

/// An in-progress set of edits to one kit, deduplicated by field.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct KitEditDraft {
    pub kit_index: usize,
    pub edits: Vec<KitEdit>,
}

impl KitEditDraft {
    pub fn new(kit_index: usize) -> Self {
        Self {
            kit_index,
            edits: Vec::new(),
        }
    }

    /// Records an edit; a later edit to the same field replaces the earlier.
    pub fn push(&mut self, field: KitField, value: FieldValue) {
        debug_assert!(field.track().is_none_or(|t| t < TRACK_COUNT));
        match self.edits.iter_mut().find(|e| e.field == field) {
            Some(existing) => existing.value = value,
            None => self.edits.push(KitEdit { field, value }),
        }
    }

    /// Drops any pending edit of `field` (the value returned to current).
    pub fn clear_field(&mut self, field: KitField) {
        self.edits.retain(|e| e.field != field);
    }

    /// The draft's pending value for `field`, if any.
    pub fn value(&self, field: KitField) -> Option<&FieldValue> {
        self.edits
            .iter()
            .find(|e| e.field == field)
            .map(|e| &e.value)
    }

    pub fn is_empty(&self) -> bool {
        self.edits.is_empty()
    }
}
