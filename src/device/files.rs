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
const OP_SAMPLE_CREATE_DIR: u8 = 0x11;
const OP_SAMPLE_DELETE_DIR: u8 = 0x12;
const OP_RAM_READ: u8 = 0x17;
const OP_RAM_LOAD_SLOT: u8 = 0x19;
const OP_SAMPLE_DELETE_FILE: u8 = 0x20;
const OP_SAMPLE_RENAME: u8 = 0x21;
const OP_SAMPLE_INFO_FROM_HASH_SIZE: u8 = 0x23;
const OP_SAMPLE_READ_OPEN: u8 = 0x30;
const OP_SAMPLE_READ_CLOSE: u8 = 0x31;
const OP_SAMPLE_READ_BLOCK: u8 = 0x32;
const OP_DATA_LIST: u8 = 0x53;
const OP_DATA_READ_OPEN: u8 = 0x54;
const OP_DATA_READ_PARTIAL: u8 = 0x55;
const OP_DATA_READ_CLOSE: u8 = 0x56;
const OP_DATA_WRITE_OPEN: u8 = 0x57;
const OP_DATA_WRITE_PARTIAL: u8 = 0x58;
const OP_DATA_WRITE_CLOSE: u8 = 0x59;
const OP_DATA_MOVE: u8 = 0x5A;
const OP_DATA_COPY: u8 = 0x5B;
const OP_DATA_CLEAR: u8 = 0x5C;
const OP_DATA_SWAP: u8 = 0x5D;

/// Chunk size requested per partial data read (matches Transfer/elektroid).
const DATA_CHUNK_BYTES: u32 = 0x2000;

/// Bytes of sample payload requested per read block (matches Transfer).
const SAMPLE_READ_BLOCK_BYTES: u32 = 0x2000;

/// Offset of the payload bytes inside a sample read-block reply.
const SAMPLE_READ_PAYLOAD_OFFSET: usize = 22;

/// A sample file starts with a 64-byte header (type, stereo flag, byte size,
/// rate, loop points, padding) ahead of the big-endian 16-bit PCM data.
const SAMPLE_FILE_HEADER_LEN: usize = 64;

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

/// One downloaded data item (e.g. a project slot's raw payload).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataDownload {
    pub bytes: Vec<u8>,
    /// The compression flag the device reported for this transfer; the
    /// payload is stored as received either way.
    pub compressed: bool,
}

/// One downloaded +Drive sample: decoded header fields + little-endian PCM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SampleDownload {
    /// 16-bit PCM, byte-swapped to little-endian (the wire is big-endian).
    pub pcm_le: Vec<u8>,
    /// True for stereo-interleaved data (header flag; AR native is mono).
    pub stereo: bool,
    /// Sample rate from the header (48 000 for device-native files).
    pub rate: u32,
}

/// One chunk of a data download (a `DATA_READ_PARTIAL` reply).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataChunk {
    pub last: bool,
    /// CRC the device reports for this chunk (semantics hardware-verified
    /// via the probe; see `docs/hardware-verification.md`).
    pub crc: u32,
    pub data: Vec<u8>,
}

/// Request sequence numbers come from one process-wide counter, so replies
/// can never cross-match between concurrent clients on the same port (e.g.
/// the +Drive browser and a running ops plan): `transact` matches replies by
/// (seq, opcode), and distinct clients now never share a seq.
static NEXT_SEQ: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(0);

/// Stateful file-protocol client.
#[derive(Default)]
pub struct FileClient {}

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

    /// Downloads one data item (e.g. `/projects/2`) chunk by chunk.
    /// Read-only on the device. `compressed` asks the device to compress the
    /// transfer (what Transfer does); `false` requests the raw payload.
    /// `on_chunk` receives each received chunk and the total byte count.
    pub fn download_data(
        &mut self,
        link: &mut MidiLink,
        path: &str,
        compressed: bool,
        mut on_chunk: impl FnMut(&DataChunk, usize),
    ) -> Option<DataDownload> {
        let mut body = vec![OP_DATA_READ_OPEN];
        push_path(&mut body, path);
        body.extend_from_slice(&DATA_CHUNK_BYTES.to_be_bytes());
        body.push(u8::from(compressed));
        let reply = self.transact(link, &body)?;
        let (jid, compressed) = parse_data_open(&reply)?;

        let mut bytes = Vec::new();
        let mut chunk_seq: u32 = 0;
        loop {
            let mut body = vec![OP_DATA_READ_PARTIAL];
            body.extend_from_slice(&jid.to_be_bytes());
            body.extend_from_slice(&chunk_seq.to_be_bytes());
            let reply = self.transact(link, &body)?;
            let chunk = parse_data_chunk(&reply)?;
            // Check the device-reported CRC before trusting the payload —
            // downloads back slot ops as the only recovery copy, so damaged
            // bytes must never be saved as a "backup". Empty chunks carry
            // uninitialized fields (hardware-verified) and are skipped.
            if !chunk.data.is_empty() && crc32(0xFFFF_FFFF, &chunk.data) != chunk.crc {
                return None;
            }
            bytes.extend_from_slice(&chunk.data);
            on_chunk(&chunk, bytes.len());
            chunk_seq = chunk_seq.wrapping_add(1);
            if chunk.last {
                break;
            }
        }

        let mut body = vec![OP_DATA_READ_CLOSE];
        body.extend_from_slice(&jid.to_be_bytes());
        // Close failure doesn't invalidate already-received bytes.
        let _ = self.transact(link, &body);
        Some(DataDownload { bytes, compressed })
    }

    /// Uploads one data item (e.g. a whole project payload to
    /// `/projects/8`). Chunked with per-chunk CRC; the close reply echoes
    /// the byte count, which is verified. `progress` receives bytes sent.
    pub fn upload_data(
        &mut self,
        link: &mut MidiLink,
        path: &str,
        payload: &[u8],
        mut progress: impl FnMut(usize),
    ) -> Result<(), String> {
        let mut body = vec![OP_DATA_WRITE_OPEN];
        body.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        push_path(&mut body, path);
        let reply = self
            .transact(link, &body)
            .ok_or("write-open got no reply")?;
        if reply.get(5) != Some(&1) {
            return Err("device refused the data write".into());
        }
        let jid = be_u32_checked(&reply, 6).ok_or("write-open reply came back short")?;

        let mut sent = 0usize;
        for (chunk_seq, chunk) in payload.chunks(DATA_CHUNK_BYTES as usize).enumerate() {
            let mut body = vec![OP_DATA_WRITE_PARTIAL];
            body.extend_from_slice(&jid.to_be_bytes());
            body.extend_from_slice(&(chunk_seq as u32).to_be_bytes());
            body.extend_from_slice(&crc32(0xFFFF_FFFF, chunk).to_be_bytes());
            body.extend_from_slice(&(chunk.len() as u32).to_be_bytes());
            body.extend_from_slice(chunk);
            let reply = self
                .transact(link, &body)
                .ok_or("write-partial got no reply")?;
            if reply.get(5) != Some(&1) {
                return Err(format!("device rejected chunk {chunk_seq}"));
            }
            sent += chunk.len();
            progress(sent);
        }

        let mut body = vec![OP_DATA_WRITE_CLOSE];
        body.extend_from_slice(&jid.to_be_bytes());
        body.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        let reply = self
            .transact(link, &body)
            .ok_or("write-close got no reply")?;
        if reply.get(5) != Some(&1) {
            return Err("device refused to close the data write".into());
        }
        let received =
            be_u32_checked(&reply, 10).ok_or("write-close reply came back short")? as usize;
        if received != payload.len() {
            return Err(format!(
                "device reports {received} bytes received, sent {}",
                payload.len()
            ));
        }
        Ok(())
    }

    /// Copies one data item to another slot (e.g. `/projects/20` →
    /// `/projects/8`). The device does the copy; nothing is transferred.
    pub fn copy_data(&mut self, link: &mut MidiLink, src: &str, dst: &str) -> Result<(), String> {
        let mut body = vec![OP_DATA_COPY];
        push_path(&mut body, src);
        push_path(&mut body, dst);
        let reply = self.transact(link, &body).ok_or("copy got no reply")?;
        if reply.get(5) != Some(&1) {
            return Err("device refused the copy".into());
        }
        Ok(())
    }

    /// Moves one data item to an empty slot (e.g. `/projects/20` →
    /// `/projects/8`). Device-side; nothing is transferred.
    pub fn move_data(&mut self, link: &mut MidiLink, src: &str, dst: &str) -> Result<(), String> {
        let reply = self
            .transact(link, &path_op_body(OP_DATA_MOVE, &[src, dst]))
            .ok_or("move got no reply")?;
        expect_ok(&reply, "move the project")
    }

    /// Swaps two data slots (e.g. `/projects/3` ↔ `/projects/4`).
    pub fn swap_data(&mut self, link: &mut MidiLink, a: &str, b: &str) -> Result<(), String> {
        let reply = self
            .transact(link, &path_op_body(OP_DATA_SWAP, &[a, b]))
            .ok_or("swap got no reply")?;
        expect_ok(&reply, "swap the projects")
    }

    /// Clears one data slot (e.g. `/projects/7`). Destructive — callers must
    /// have downloaded and stored the payload first (backup-first rule).
    pub fn clear_data(&mut self, link: &mut MidiLink, path: &str) -> Result<(), String> {
        let reply = self
            .transact(link, &path_op_body(OP_DATA_CLEAR, &[path]))
            .ok_or("clear got no reply")?;
        expect_ok(&reply, "clear the slot")
    }

    /// Asks the device to load a +Drive sample file into a project RAM slot
    /// (the SAMPLES → LOAD TO PROJ operation, remotely).
    pub fn ram_load_slot(
        &mut self,
        link: &mut MidiLink,
        drive_path: &str,
        slot: u8,
    ) -> Result<(), String> {
        let mut body = vec![OP_RAM_LOAD_SLOT, 0];
        push_path(&mut body, drive_path);
        body.push(0xFF); // track: none — load into the slot only
        body.extend_from_slice(&u16::from(slot).to_be_bytes());
        body.push(0x01);
        let reply = self.transact(link, &body).ok_or("load got no reply")?;
        if reply.get(5) != Some(&1) {
            return Err("device refused to load the sample into the slot".into());
        }
        Ok(())
    }

    /// Creates a +Drive sample directory.
    pub fn create_sample_dir(&mut self, link: &mut MidiLink, path: &str) -> Result<(), String> {
        let reply = self
            .transact(link, &path_op_body(OP_SAMPLE_CREATE_DIR, &[path]))
            .ok_or("create-directory got no reply")?;
        expect_ok(&reply, "create the directory")
    }

    /// Deletes a +Drive sample directory (the device refuses non-empty ones;
    /// callers must preflight emptiness anyway and never rely on that).
    pub fn delete_sample_dir(&mut self, link: &mut MidiLink, path: &str) -> Result<(), String> {
        let reply = self
            .transact(link, &path_op_body(OP_SAMPLE_DELETE_DIR, &[path]))
            .ok_or("delete-directory got no reply")?;
        expect_ok(&reply, "delete the directory")
    }

    /// Deletes one +Drive sample file. Irreversible on the device side —
    /// callers must have downloaded a local copy first (backup-first rule).
    pub fn delete_sample_file(&mut self, link: &mut MidiLink, path: &str) -> Result<(), String> {
        let reply = self
            .transact(link, &path_op_body(OP_SAMPLE_DELETE_FILE, &[path]))
            .ok_or("delete-file got no reply")?;
        expect_ok(&reply, "delete the file")
    }

    /// Renames a +Drive sample file: `dst` is a full path, so a destination
    /// in another directory moves the file. The content hash is unchanged —
    /// project slot tables (which reference by hash) keep resolving.
    pub fn rename_sample_path(
        &mut self,
        link: &mut MidiLink,
        src: &str,
        dst: &str,
    ) -> Result<(), String> {
        let reply = self
            .transact(link, &path_op_body(OP_SAMPLE_RENAME, &[src, dst]))
            .ok_or("rename got no reply")?;
        expect_ok(&reply, "rename the file")
    }

    /// Downloads one +Drive sample file (open → block reads → close).
    /// `progress` receives (received, total) raw byte counts.
    pub fn download_sample(
        &mut self,
        link: &mut MidiLink,
        path: &str,
        mut progress: impl FnMut(usize, usize),
    ) -> Result<SampleDownload, String> {
        let mut body = vec![OP_SAMPLE_READ_OPEN];
        push_path(&mut body, path);
        let reply = self
            .transact(link, &body)
            .ok_or("sample-open got no reply")?;
        match reply.get(5) {
            Some(&1) => {}
            // Factory content answers with its own status code.
            Some(&2) => return Err("the device does not serve factory samples".into()),
            _ => return Err("device refused to open the sample for reading".into()),
        }
        let id = be_u32_checked(&reply, 6).ok_or("sample-open reply came back short")?;
        let total = be_u32_checked(&reply, 10).ok_or("sample-open reply came back short")? as usize;
        if total < SAMPLE_FILE_HEADER_LEN {
            // Leave nothing open on the device before bailing.
            self.close_sample_read(link, id);
            return Err(format!(
                "sample reports only {total} bytes — not a sample file"
            ));
        }

        let mut raw = Vec::with_capacity(total);
        let result = loop {
            if raw.len() >= total {
                break Ok(());
            }
            let req = (total - raw.len()).min(SAMPLE_READ_BLOCK_BYTES as usize);
            let mut body = vec![OP_SAMPLE_READ_BLOCK];
            body.extend_from_slice(&id.to_be_bytes());
            body.extend_from_slice(&(req as u32).to_be_bytes());
            body.extend_from_slice(&(raw.len() as u32).to_be_bytes());
            let Some(reply) = self.transact(link, &body) else {
                break Err("sample-read got no reply".to_owned());
            };
            if let Err(e) = expect_ok(&reply, "read a sample block") {
                break Err(e);
            }
            let Some(chunk) =
                reply.get(SAMPLE_READ_PAYLOAD_OFFSET..SAMPLE_READ_PAYLOAD_OFFSET + req)
            else {
                break Err("sample block came back short".to_owned());
            };
            raw.extend_from_slice(chunk);
            progress(raw.len(), total);
        };

        // Always close the read job — a mid-transfer failure must not leave
        // it open (whether the device would refuse the next open is unknown).
        self.close_sample_read(link, id);
        result?;
        parse_sample_file(&raw)
    }

    /// Best-effort close of a sample read job (failure doesn't invalidate
    /// already-received bytes).
    fn close_sample_read(&mut self, link: &mut MidiLink, id: u32) {
        let mut body = vec![OP_SAMPLE_READ_CLOSE];
        body.extend_from_slice(&id.to_be_bytes());
        let _ = self.transact(link, &body);
    }

    /// Sends `body` (opcode + args) and waits for its matching reply,
    /// returning the decoded reply message.
    fn transact(&mut self, link: &mut MidiLink, body: &[u8]) -> Option<Vec<u8>> {
        let seq = NEXT_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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
                    hash: slot.hash,
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

/// Builds a path-argument request body: opcode + each path NUL-terminated.
/// Public so protocol tests can assert exact frames without a device.
pub fn path_op_body(opcode: u8, paths: &[&str]) -> Vec<u8> {
    let mut body = vec![opcode];
    for path in paths {
        push_path(&mut body, path);
    }
    body
}

/// Whether writes to this +Drive path must be refused: everything under the
/// write-protected factory tree. Case-insensitive — hash lookups have
/// returned lowercase `/factory/...` paths.
pub fn is_protected_drive_path(path: &str) -> bool {
    path.trim_start_matches('/')
        .split('/')
        .next()
        .is_some_and(|first| first.eq_ignore_ascii_case("factory"))
}

/// Checks the status byte of a write-op reply (1 = success).
fn expect_ok(reply: &[u8], what: &str) -> Result<(), String> {
    if reply.get(5) == Some(&1) {
        Ok(())
    } else {
        Err(format!("device refused to {what}"))
    }
}

/// Splits a downloaded sample file into header fields + LE PCM. The header's
/// own byte-size field must account for the received PCM exactly — a wrong
/// payload offset or a short block would otherwise become a silently corrupt
/// backup WAV, discovered only when it is needed.
/// Public for offline tests against synthetic payloads.
pub fn parse_sample_file(raw: &[u8]) -> Result<SampleDownload, String> {
    if raw.len() < SAMPLE_FILE_HEADER_LEN {
        return Err(format!(
            "sample file is {} bytes — shorter than its {SAMPLE_FILE_HEADER_LEN}-byte header",
            raw.len()
        ));
    }
    let stereo = raw[1] != 0;
    let declared = be_u32(raw, 4) as usize;
    let rate = be_u32(raw, 8);
    let wire_pcm = &raw[SAMPLE_FILE_HEADER_LEN..];
    if declared != wire_pcm.len() {
        return Err(format!(
            "sample header declares {declared} PCM bytes but {} were received — \
             refusing to store a corrupt backup",
            wire_pcm.len()
        ));
    }
    let mut pcm_le = Vec::with_capacity(wire_pcm.len());
    for pair in wire_pcm.chunks_exact(2) {
        pcm_le.push(pair[1]);
        pcm_le.push(pair[0]);
    }
    Ok(SampleDownload {
        pcm_le,
        stereo,
        rate,
    })
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

/// Parses a `DATA_READ_OPEN` reply: status at 5 (non-zero = ok), job id at
/// 6, chunk size at 10, compression flag at 14.
pub fn parse_data_open(reply: &[u8]) -> Option<(u32, bool)> {
    if reply.len() < 15 || reply[5] == 0 {
        return None;
    }
    Some((be_u32(reply, 6), reply[14] != 0))
}

/// Parses a `DATA_READ_PARTIAL` reply: status at 5, job id at 6, chunk seq
/// at 10, status word at 14, last flag at 18, crc at 19, size at 23, data
/// from 27.
pub fn parse_data_chunk(reply: &[u8]) -> Option<DataChunk> {
    if reply.len() < 27 || reply[5] == 0 {
        return None;
    }
    let size = be_u32(reply, 23) as usize;
    let data = reply.get(27..27 + size)?.to_vec();
    Some(DataChunk {
        last: reply[18] != 0,
        crc: be_u32(reply, 19),
        data,
    })
}

/// CRC-32 (IEEE, zlib-compatible) over `data` starting from `init` as the
/// running value — used to check device-reported chunk CRCs. Pass 0 for the
/// standard zlib result; the probe determines which init the device uses.
pub fn crc32(init: u32, data: &[u8]) -> u32 {
    let mut crc = !init;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xEDB8_8320 & (0u32.wrapping_sub(crc & 1)));
        }
    }
    !crc
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

/// [`be_u32`] with a bounds check, for replies whose length is not
/// guaranteed past the status byte.
fn be_u32_checked(data: &[u8], pos: usize) -> Option<u32> {
    let bytes: [u8; 4] = data.get(pos..pos + 4)?.try_into().ok()?;
    Some(u32::from_be_bytes(bytes))
}

fn be_u32(data: &[u8], pos: usize) -> u32 {
    u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]])
}
