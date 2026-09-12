//! Display-name resolution rules (PRD v2 §33).
//!
//! Precedence: native device name, then a deterministic semantic fallback.
//! A name that *should* exist but could not be read is "unavailable" — never
//! fabricated. These rules are pure so the UI stays consistent everywhere.

/// The device's track order: pads left→right, bottom row first.
pub const TRACK_LABELS: [&str; 12] = [
    "BD", "SD", "RS", "CP", "BT", "LT", "MT", "HT", "CH", "OH", "CY", "CB",
];

/// How well a display name is known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedName {
    /// A real native device name.
    Native(String),
    /// No native name exists (or it is blank); a deterministic fallback.
    Fallback(String),
    /// A name should exist but the protocol could not resolve it.
    Unavailable,
}

impl ResolvedName {
    /// The text to render as the primary label.
    pub fn text(&self) -> &str {
        match self {
            Self::Native(s) | Self::Fallback(s) => s,
            Self::Unavailable => "Name unavailable",
        }
    }

    /// True when the label is a real native name.
    pub fn is_native(&self) -> bool {
        matches!(self, Self::Native(_))
    }
}

/// Resolves a track Sound name: native if present, else `"BD sound"`.
pub fn sound_display_name(raw: &str, track: usize) -> ResolvedName {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        ResolvedName::Fallback(format!("{} sound", track_label(track)))
    } else {
        ResolvedName::Native(trimmed.to_owned())
    }
}

/// Resolves a kit name: native if present, else `"Kit 014"` (1-based).
pub fn kit_display_name(raw: &str, index: usize) -> ResolvedName {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        ResolvedName::Fallback(format!("Kit {:03}", index + 1))
    } else {
        ResolvedName::Native(trimmed.to_owned())
    }
}

/// Resolves a project name: native if present, else `"Project 042"`.
pub fn project_display_name(raw: &str, slot: u32) -> ResolvedName {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        ResolvedName::Fallback(format!("Project {slot:03}"))
    } else {
        ResolvedName::Native(trimmed.to_owned())
    }
}

/// Resolves a sample-slot name from an optional file-protocol lookup.
/// `None` means the name exists on the device but was not resolved.
pub fn sample_display_name(resolved: Option<&str>) -> ResolvedName {
    match resolved.map(str::trim) {
        Some(name) if !name.is_empty() => ResolvedName::Native(name.to_owned()),
        _ => ResolvedName::Unavailable,
    }
}

/// The hardware track abbreviation for a track index (clamped).
pub fn track_label(track: usize) -> &'static str {
    TRACK_LABELS[track.min(TRACK_LABELS.len() - 1)]
}
