//! Parser for raw (uncompressed) project payloads downloaded over the data
//! protocol — the WP11 indexer's read side.
//!
//! Layout mapped empirically against live AR MKII OS 1.72 downloads and the
//! libanalogrytm FW1.70 struct sizes (see `docs/hardware-verification.md`):
//!
//! ```text
//! 0x0000    header, magic AC 11 D3 03; project name (cstr) at 0x27
//! 0x0037    work-buffer pattern copy (0x332D bytes)
//! 0x3364    128 × pattern records, 0x332D bytes each
//!           (kit link at +0x3325: 0..127 saved kit, 0xFF unsaved)
//! 0x19C9E4  work-buffer kit copy (0xA36 bytes)
//! 0x19D41A  128 × kit records, 0xA32 bytes each
//!           (name at +0x04; 12 track sounds from +0x2E, 162 bytes each,
//!            sound name at +0x0C)
//! ```
//!
//! Everything else (settings, songs, pool sounds, sample table) is not
//! parsed yet; unknown regions are left untouched.

use serde::{Deserialize, Serialize};

/// Magic prefix of a project payload (compressed and raw share it).
const MAGIC: [u8; 4] = [0xAC, 0x11, 0xD3, 0x03];

const NAME_OFFSET: usize = 0x27;
const NAME_LEN: usize = 15;

const PATTERNS_OFFSET: usize = 0x3364;
const PATTERN_SIZE: usize = 0x332D;
const PATTERN_KIT_OFFSET: usize = 0x3325;

// After the pattern array comes a work-buffer kit copy (0xA36), then the
// saved-kit array.
const KITS_OFFSET: usize = PATTERNS_OFFSET + 128 * PATTERN_SIZE + 0xA36; // 0x19D41A
const KIT_SIZE: usize = 0xA32;
const KIT_NAME_OFFSET: usize = 0x04;
const KIT_SOUNDS_OFFSET: usize = 0x2E;
const SOUND_SIZE: usize = 162;
const SOUND_NAME_OFFSET: usize = 0x0C;

// Sample-slot table (found empirically; a 16-byte header entry carrying the
// 0x7F count precedes slot 1). Entry i (1..=127): hash u32be, size u32be,
// 8 reserved bytes.
const SAMPLES_OFFSET: usize = 0x1F94C9;
const SAMPLE_ENTRY_SIZE: usize = 16;

/// Minimum payload length that contains all parsed regions.
const MIN_RAW_LEN: usize = SAMPLES_OFFSET + 128 * SAMPLE_ENTRY_SIZE;

/// One kit as read from a raw project payload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RawKit {
    pub index: usize,
    pub name: String,
    /// The 12 track Sound names (may be empty strings).
    pub sound_names: Vec<String>,
}

/// One pattern as read from a raw project payload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RawPattern {
    pub index: usize,
    /// Linked saved kit slot; `None` when the pattern's kit is unsaved/empty.
    pub kit_link: Option<u8>,
}

/// One used sample slot of a project, by content identity.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct RawSampleSlot {
    pub slot: u8,
    pub hash: u32,
    pub size: u32,
}

/// Kits + patterns + sample slots of one project, read from its backup.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RawProjectSnapshot {
    pub name: String,
    pub kits: Vec<RawKit>,
    pub patterns: Vec<RawPattern>,
    /// Used sample slots (hash+size identity; names live on the +Drive).
    #[serde(default)]
    pub samples: Vec<RawSampleSlot>,
}

impl RawProjectSnapshot {
    /// Kits that at least one pattern links to.
    pub fn linked_kits(&self) -> Vec<usize> {
        let mut linked: Vec<usize> = self
            .patterns
            .iter()
            .filter_map(|p| p.kit_link.map(usize::from))
            .collect();
        linked.sort_unstable();
        linked.dedup();
        linked
    }

    /// True when a kit slot looks used (named, or linked by a pattern).
    pub fn kit_in_use(&self, index: usize) -> bool {
        self.kits
            .get(index)
            .is_some_and(|k| !k.name.trim().is_empty())
            || self
                .patterns
                .iter()
                .any(|p| p.kit_link == Some(index as u8))
    }
}

/// True when `bytes` looks like a raw (uncompressed) project payload this
/// parser understands. Compressed payloads share the magic but are far
/// shorter than the fixed raw layout.
pub fn is_raw_project(bytes: &[u8]) -> bool {
    bytes.len() >= MIN_RAW_LEN && bytes.starts_with(&MAGIC)
}

/// Parses a raw project payload. Returns `None` when the payload is not the
/// raw format (e.g. a compressed backup) — never guesses.
pub fn parse_raw_project(bytes: &[u8]) -> Option<RawProjectSnapshot> {
    if !is_raw_project(bytes) {
        return None;
    }
    let name = read_name(bytes, NAME_OFFSET, NAME_LEN);

    let patterns = (0..128)
        .map(|i| {
            let link = bytes[PATTERNS_OFFSET + i * PATTERN_SIZE + PATTERN_KIT_OFFSET];
            RawPattern {
                index: i,
                kit_link: (link <= 127).then_some(link),
            }
        })
        .collect();

    let kits = (0..128)
        .map(|i| {
            let base = KITS_OFFSET + i * KIT_SIZE;
            let sound_names = (0..12)
                .map(|t| {
                    read_name(
                        bytes,
                        base + KIT_SOUNDS_OFFSET + t * SOUND_SIZE + SOUND_NAME_OFFSET,
                        NAME_LEN,
                    )
                })
                .collect();
            RawKit {
                index: i,
                name: read_name(bytes, base + KIT_NAME_OFFSET, NAME_LEN),
                sound_names,
            }
        })
        .collect();

    let samples = (1..=127u8)
        .filter_map(|slot| {
            let base = SAMPLES_OFFSET + usize::from(slot) * SAMPLE_ENTRY_SIZE;
            let hash = u32::from_be_bytes(bytes[base..base + 4].try_into().ok()?);
            let size = u32::from_be_bytes(bytes[base + 4..base + 8].try_into().ok()?);
            (hash != 0 && size != 0).then_some(RawSampleSlot { slot, hash, size })
        })
        .collect();

    Some(RawProjectSnapshot {
        name,
        kits,
        patterns,
        samples,
    })
}

/// The raw kit record bytes for `kit_index`, for re-encoding into sysex.
pub fn kit_raw_record(bytes: &[u8], kit_index: usize) -> Option<&[u8]> {
    if !is_raw_project(bytes) || kit_index > 127 {
        return None;
    }
    let base = KITS_OFFSET + kit_index * KIT_SIZE;
    bytes.get(base..base + KIT_SIZE)
}

/// Reads a fixed-capacity, NUL-terminated device name; non-ASCII bytes are
/// rendered as their low-ASCII equivalent rather than dropped.
fn read_name(bytes: &[u8], offset: usize, max_len: usize) -> String {
    bytes
        .get(offset..offset + max_len)
        .map(|field| {
            field
                .iter()
                .take_while(|&&b| b != 0)
                .map(|&b| {
                    if (0x20..0x7F).contains(&b) {
                        b as char
                    } else {
                        '?'
                    }
                })
                .collect::<String>()
                .trim()
                .to_owned()
        })
        .unwrap_or_default()
}
