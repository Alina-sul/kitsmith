//! Clean-room client for the Elektron file/data sysex protocol — the wire
//! protocol Elektron Transfer speaks (undocumented; formats learned from the
//! MIDI stream structure documented by the elektroid project, GPLv3, used as
//! a spec only — no code ported).
//!
//! Read-only subset for now: ping, +Drive sample directory listing, data
//! directory listing (projects), RAM sample-slot table, and the
//! hash+size → +Drive path lookup that turns RAM slots into sample *names*.
//!
//! ## Wire format
//!
//! Sysex frame: `F0 00 20 3C 10 00 <7-bit-packed payload> F7`. The payload
//! packs each 7 data bytes into 8 wire bytes (leading byte carries the seven
//! MSBs, bit 6 first). A decoded request is `[seq_hi, seq_lo, 0, 0, opcode,
//! args…]`; the reply echoes the sequence at bytes 2–3 and answers with
//! `opcode | 0x80` at byte 4, with a status byte at 5.
//!
//! The sequence sits at a *different offset* in each direction (0–1 outbound,
//! 2–3 inbound). That asymmetry looks like a bug and is not: a live probe
//! against an AR MKII (OS 1.72) drove ping → dir listings → RAM table → hash
//! lookups → download through one client, so every call after the first ran
//! with a non-zero sequence and still matched its reply. Do not "fix" it
//! without re-probing hardware.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::config::FILE_REPLY_TIMEOUT;
use crate::midi::link::MidiLink;

/// Sysex header for the file protocol.
const HEADER: [u8; 6] = [0xF0, 0x00, 0x20, 0x3C, 0x10, 0x00];

const OP_PING: u8 = 0x01;
const OP_SAMPLE_READ_DIR: u8 = 0x10;
const OP_RAM_READ: u8 = 0x17;
const OP_SAMPLE_INFO_FROM_HASH_SIZE: u8 = 0x23;
const OP_DATA_LIST: u8 = 0x53;

/// The data filesystem path that holds the 128 project slots.
pub const PROJECTS_PATH: &str = "/projects";

/// One entry of a +Drive sample directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriveEntry {
    pub name: String,
    pub size: u32,
    pub hash: u32,
    pub is_dir: bool,
    pub write_protected: bool,
}

/// One entry of a data directory (e.g. a project slot).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataEntry {
    pub name: String,
    /// Slot id for file entries; `None` for directories.
    pub id: Option<u32>,
    pub size: u32,
    pub has_valid_data: bool,
    pub is_dir: bool,
}

/// One RAM sample slot of the active project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RamSlot {
    pub id: u16,
    pub hash: u32,
    pub size: u32,
    pub used: bool,
}

/// Stateful client: owns the request sequence counter.
#[derive(Default)]
pub struct FileClient {
    seq: u16,
}

impl FileClient {
    pub fn new() -> Self {
        Self::default()
    }

    /// True when the device answers the protocol ping.
    pub fn ping(&mut self, link: &mut MidiLink) -> bool {
        self.transact(link, &[OP_PING]).is_some()
    }

    /// Lists one +Drive sample directory (`/`, `/incoming`, …).
    pub fn list_sample_dir(&mut self, link: &mut MidiLink, path: &str) -> Option<Vec<DriveEntry>> {
        let mut body = vec![OP_SAMPLE_READ_DIR];
        push_path(&mut body, path);
        let reply = self.transact(link, &body)?;
        Some(parse_sample_dir(&reply))
    }

    /// Lists one data directory — [`PROJECTS_PATH`] yields the project slots.
    pub fn list_data_dir(&mut self, link: &mut MidiLink, path: &str) -> Option<Vec<DataEntry>> {
        let mut body = vec![OP_DATA_LIST];
        push_path(&mut body, path);
        body.extend_from_slice(&0u32.to_be_bytes()); // start index
        body.extend_from_slice(&0u32.to_be_bytes()); // end index
        body.push(1); // list everything
        let reply = self.transact(link, &body)?;
        Some(parse_data_dir(&reply))
    }

    /// Reads the active project's RAM sample-slot table.
    pub fn ram_slots(&mut self, link: &mut MidiLink) -> Option<Vec<RamSlot>> {
        let reply = self.transact(link, &[OP_RAM_READ, 0xFF, 0xFF, 0xFF, 0xFF])?;
        parse_ram_slots(&reply)
    }

    /// Resolves a RAM slot's content hash+size to its +Drive file path.
    pub fn sample_path_from_hash_size(
        &mut self,
        link: &mut MidiLink,
        hash: u32,
        size: u32,
    ) -> Option<String> {
        let mut body = vec![OP_SAMPLE_INFO_FROM_HASH_SIZE];
        body.extend_from_slice(&hash.to_be_bytes());
        body.extend_from_slice(&size.to_be_bytes());
        let reply = self.transact(link, &body)?;
        parse_hash_path(&reply)
    }

    /// Sends `body` (opcode + args) and waits for its matching reply,
    /// returning the decoded reply message.
    fn transact(&mut self, link: &mut MidiLink, body: &[u8]) -> Option<Vec<u8>> {
        let seq = self.seq;
        self.seq = self.seq.wrapping_add(1);
        let mut msg = vec![(seq >> 8) as u8, (seq & 0xFF) as u8, 0, 0];
        msg.extend_from_slice(body);
        link.send(&frame(&msg)).ok()?;

        let expected_type = body[0] | 0x80;
        let deadline = std::time::Instant::now() + FILE_REPLY_TIMEOUT;
        loop {
            let remaining = deadline.checked_duration_since(std::time::Instant::now())?;
            let sysex = link.recv_timeout(remaining)?;
            let Some(reply) = unframe(&sysex) else {
                continue; // not a file-protocol message
            };
            if reply.len() < 6 {
                continue;
            }
            let reply_seq = (u16::from(reply[2]) << 8) | u16::from(reply[3]);
            if reply_seq != seq || reply[4] != expected_type {
                continue;
            }
            return Some(reply);
        }
    }
}

/// Fills the model's sample-slot names and project list via the file
/// protocol. Returns `false` when the device does not answer the protocol
/// ping (the object-sysex model is still valid then).
///
/// Every lookup blocks up to [`FILE_REPLY_TIMEOUT`], so `cancel` is checked
/// between them — a device that pings but ignores the lookups would otherwise
/// hold the sync for minutes with no way out.
pub fn enrich_model(
    link: &mut MidiLink,
    model: &mut crate::model::ProjectModel,
    cancel: &Arc<AtomicBool>,
    mut progress: impl FnMut(&'static str, usize, usize),
) -> bool {
    let mut client = FileClient::new();
    if !client.ping(link) {
        return false;
    }

    if let Some(slots) = client.ram_slots(link) {
        let used: Vec<RamSlot> = slots.into_iter().filter(|s| s.used).collect();
        model.samples.clear();
        for (i, slot) in used.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                return true;
            }
            progress("sample names", i + 1, used.len());
            let Ok(slot_number) = u8::try_from(slot.id) else {
                continue;
            };
            if let Some(path) = client.sample_path_from_hash_size(link, slot.hash, slot.size) {
                let name = path.rsplit('/').next().unwrap_or(&path).to_owned();
                model.samples.push(crate::model::SampleSlotInfo {
                    slot: slot_number,
                    name,
                    path,
                    size: slot.size,
                });
            }
        }
    }

    if cancel.load(Ordering::Relaxed) {
        return true;
    }
    progress("project list", 1, 1);
    if let Some(entries) = client.list_data_dir(link, PROJECTS_PATH) {
        model.projects = entries
            .into_iter()
            .filter(|e| !e.is_dir && !e.name.is_empty())
            .map(|e| crate::model::ProjectInfo {
                id: e.id.unwrap_or(0),
                name: e.name,
            })
            .collect();
    }
    true
}

/// Wraps a decoded message into a complete sysex frame.
pub fn frame(msg: &[u8]) -> Vec<u8> {
    let mut sysex = HEADER.to_vec();
    sysex.extend(pack7(msg));
    sysex.push(0xF7);
    sysex
}

/// Unwraps a sysex frame into the decoded message, if it is file-protocol.
pub fn unframe(sysex: &[u8]) -> Option<Vec<u8>> {
    let payload = sysex.strip_prefix(&HEADER[..])?.strip_suffix(&[0xF7][..])?;
    Some(unpack7(payload))
}

/// Packs 8-bit data into 7-bit wire bytes: each group of up to 7 data bytes
/// is preceded by one byte carrying their MSBs (first byte's MSB in bit 6).
pub fn pack7(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + data.len().div_ceil(7));
    for group in data.chunks(7) {
        let mut msbs = 0u8;
        for (k, &byte) in group.iter().enumerate() {
            if byte & 0x80 != 0 {
                msbs |= 0x40 >> k;
            }
        }
        out.push(msbs);
        out.extend(group.iter().map(|&b| b & 0x7F));
    }
    out
}

/// Inverse of [`pack7`].
pub fn unpack7(wire: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(wire.len().saturating_sub(wire.len() / 8));
    for group in wire.chunks(8) {
        let Some((&msbs, rest)) = group.split_first() else {
            continue;
        };
        for (k, &byte) in rest.iter().enumerate() {
            let high = if msbs & (0x40 >> k) != 0 { 0x80 } else { 0 };
            out.push(byte | high);
        }
    }
    out
}

/// Parses a sample-directory reply: entries from byte 5, each
/// `{hash: u32, size: u32, write_protected: u8, type: u8, name: cstr}`.
pub fn parse_sample_dir(reply: &[u8]) -> Vec<DriveEntry> {
    let mut entries = Vec::new();
    let mut pos = 5;
    while pos + 10 < reply.len() {
        let hash = be_u32(reply, pos);
        let size = be_u32(reply, pos + 4);
        let write_protected = reply[pos + 8] != 0;
        let kind = reply[pos + 9];
        pos += 10;
        let name = read_cstr(reply, &mut pos);
        entries.push(DriveEntry {
            name,
            size,
            hash,
            is_dir: kind == b'D',
            write_protected,
        });
    }
    entries
}

/// Parses a data-directory reply: entries from byte 18, each
/// `name cstr, has_children u8, type u8` then per type extra fields.
pub fn parse_data_dir(reply: &[u8]) -> Vec<DataEntry> {
    let mut entries = Vec::new();
    let mut pos = 18;
    while pos < reply.len() {
        let name = read_cstr(reply, &mut pos);
        if pos + 2 > reply.len() {
            break;
        }
        let has_children = reply[pos] != 0;
        let kind = reply[pos + 1];
        pos += 2;
        match kind {
            1 => {
                // Plain directory; skip the child-entry count.
                pos += 4;
                entries.push(DataEntry {
                    name,
                    id: None,
                    size: 0,
                    has_valid_data: false,
                    is_dir: true,
                });
            }
            2 => {
                if pos + 12 > reply.len() {
                    break;
                }
                let id = be_u32(reply, pos);
                let size = be_u32(reply, pos + 4);
                let has_valid_data = reply[pos + 10] != 0;
                pos += 12;
                entries.push(DataEntry {
                    name,
                    id: (!has_children).then_some(id),
                    size,
                    has_valid_data,
                    is_dir: has_children,
                });
            }
            _ => break,
        }
    }
    entries
}

/// Bytes one RAM-slot-table entry occupies: `{hash: u32, size: u32, id: u16,
/// used: u8}`.
const RAM_SLOT_ENTRY_LEN: usize = 11;

/// Parses a RAM-slot-table reply: `count: u32` at byte 5, then `count`
/// entries of [`RAM_SLOT_ENTRY_LEN`] bytes each.
pub fn parse_ram_slots(reply: &[u8]) -> Option<Vec<RamSlot>> {
    if reply.len() < 9 {
        return None;
    }
    // The count comes off the wire; trust it only as far as the reply can
    // actually carry, so a garbled count cannot ask for a huge allocation.
    let count = (be_u32(reply, 5) as usize).min((reply.len() - 9) / RAM_SLOT_ENTRY_LEN);
    let mut slots = Vec::with_capacity(count);
    let mut pos = 9;
    for _ in 0..count {
        if pos + RAM_SLOT_ENTRY_LEN > reply.len() {
            break;
        }
        let hash = be_u32(reply, pos);
        let size = be_u32(reply, pos + 4);
        let id = (u16::from(reply[pos + 8]) << 8) | u16::from(reply[pos + 9]);
        let used = reply[pos + 10] != 0;
        pos += RAM_SLOT_ENTRY_LEN;
        slots.push(RamSlot {
            id,
            hash,
            size,
            used: used && size > 0,
        });
    }
    Some(slots)
}

/// Parses a hash→path reply: status at byte 5 (1 = found), path cstr at 14.
pub fn parse_hash_path(reply: &[u8]) -> Option<String> {
    if reply.get(5) != Some(&1) || reply.len() <= 14 {
        return None;
    }
    let mut pos = 14;
    Some(read_cstr(reply, &mut pos))
}

fn push_path(body: &mut Vec<u8>, path: &str) {
    // Device names are CP1252; ASCII is the shared subset, non-ASCII becomes
    // its low byte (good enough for path round-trips of listed names).
    body.extend(path.chars().map(|c| (c as u32).min(0xFF) as u8));
    body.push(0);
}

fn read_cstr(data: &[u8], pos: &mut usize) -> String {
    let start = *pos;
    while *pos < data.len() && data[*pos] != 0 {
        *pos += 1;
    }
    let s = data[start..*pos].iter().map(|&b| b as char).collect();
    *pos = (*pos + 1).min(data.len());
    s
}

fn be_u32(data: &[u8], pos: usize) -> u32 {
    u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]])
}
