//! Elektron file-protocol framing and parser tests (synthetic fixtures
//! built per the reverse-engineered wire format).

use kitsmith::device::files::{
    frame, is_protected_drive_path, pack7, parse_data_dir, parse_hash_path, parse_ram_slots,
    parse_sample_dir, parse_sample_file, path_op_body, unframe, unpack7,
};

#[test]
fn pack7_round_trips_all_byte_values() {
    let data: Vec<u8> = (0..=255).collect();
    let packed = pack7(&data);
    // Every wire byte must be 7-bit clean.
    assert!(packed.iter().all(|&b| b <= 0x7F));
    assert_eq!(unpack7(&packed), data);
    // Non-multiple-of-7 lengths too.
    for len in [1usize, 3, 6, 7, 8, 13, 20] {
        let d = &data[..len];
        assert_eq!(unpack7(&pack7(d)), d, "len {len}");
    }
}

#[test]
fn pack7_places_msbs_in_leading_byte() {
    // One byte 0x80: MSB set → leading byte bit 6, data byte 0.
    assert_eq!(pack7(&[0x80]), vec![0x40, 0x00]);
    // Second byte's MSB lands in bit 5.
    assert_eq!(pack7(&[0x00, 0xFF]), vec![0x20, 0x00, 0x7F]);
}

#[test]
fn frame_and_unframe_round_trip() {
    let msg = vec![0x00, 0x01, 0x00, 0x00, 0x10, b'/', 0x00];
    let sysex = frame(&msg);
    assert_eq!(&sysex[..6], &[0xF0, 0x00, 0x20, 0x3C, 0x10, 0x00]);
    assert_eq!(*sysex.last().unwrap(), 0xF7);
    assert!(sysex[1..sysex.len() - 1].iter().all(|&b| b <= 0x7F));
    // Unframe pads to whole 7-byte groups; the message must be a prefix.
    let decoded = unframe(&sysex).expect("unframe");
    assert_eq!(&decoded[..msg.len()], &msg[..]);
    // Foreign sysex is rejected.
    assert!(unframe(&[0xF0, 0x7E, 0x00, 0xF7]).is_none());
}

/// Builds a decoded reply: 2 pad bytes, seq, type, then payload.
fn reply(msg_type: u8, payload: &[u8]) -> Vec<u8> {
    let mut r = vec![0, 0, 0, 1, msg_type];
    r.extend_from_slice(payload);
    r
}

#[test]
fn parses_sample_dir_entries() {
    // Two entries: a write-protected dir "factory", a file "kick.wav".
    let mut payload = Vec::new();
    payload.extend_from_slice(&0xAABBCCDDu32.to_be_bytes());
    payload.extend_from_slice(&0u32.to_be_bytes());
    payload.push(1); // write protected
    payload.push(b'D');
    payload.extend_from_slice(b"factory\0");
    payload.extend_from_slice(&0x11223344u32.to_be_bytes());
    payload.extend_from_slice(&48000u32.to_be_bytes());
    payload.push(0);
    payload.push(b'F');
    payload.extend_from_slice(b"kick.wav\0");

    let entries = parse_sample_dir(&reply(0x90, &payload));
    assert_eq!(entries.len(), 2);
    assert!(entries[0].is_dir);
    assert!(entries[0].write_protected);
    assert_eq!(entries[0].name, "factory");
    assert!(!entries[1].is_dir);
    assert_eq!(entries[1].name, "kick.wav");
    assert_eq!(entries[1].size, 48000);
    assert_eq!(entries[1].hash, 0x11223344);
}

#[test]
fn parses_project_data_dir() {
    // Reply payload starts at byte 18: pad the header region after type.
    let mut msg = reply(0xD3, &[0; 13]);
    assert_eq!(msg.len(), 18);
    // A project slot: name, no children, type 2, id 3, size, ops, valid, meta.
    msg.extend_from_slice(b"MY PROJECT\0");
    msg.push(0); // has_children
    msg.push(2); // type: slot
    msg.extend_from_slice(&3u32.to_be_bytes());
    msg.extend_from_slice(&1024u32.to_be_bytes());
    msg.extend_from_slice(&0u16.to_be_bytes());
    msg.push(1); // has_valid_data
    msg.push(0); // has_metadata

    let entries = parse_data_dir(&msg);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "MY PROJECT");
    assert_eq!(entries[0].id, Some(3));
    assert!(entries[0].has_valid_data);
    assert!(!entries[0].is_dir);
}

#[test]
fn parses_ram_slot_table() {
    let mut payload = Vec::new();
    payload.extend_from_slice(&2u32.to_be_bytes()); // count
    for (hash, size, id, used) in [(0xAAu32, 4800u32, 1u16, 1u8), (0, 0, 2, 0)] {
        payload.extend_from_slice(&hash.to_be_bytes());
        payload.extend_from_slice(&size.to_be_bytes());
        payload.extend_from_slice(&id.to_be_bytes());
        payload.push(used);
    }
    let slots = parse_ram_slots(&reply(0x97, &payload)).expect("parse");
    assert_eq!(slots.len(), 2);
    assert!(slots[0].used);
    assert_eq!((slots[0].id, slots[0].hash, slots[0].size), (1, 0xAA, 4800));
    assert!(!slots[1].used);
}

#[test]
fn parses_hash_path_reply() {
    // Status 1 at byte 5, path cstr at byte 14.
    let mut msg = reply(0xA3, &[1]); // len 6
    msg.extend_from_slice(&[0; 8]); // bytes 6..14
    msg.extend_from_slice(b"/incoming/kick.wav\0");
    assert_eq!(parse_hash_path(&msg).as_deref(), Some("/incoming/kick.wav"));
    // Status 2 (factory / not found) → None.
    let mut missing = reply(0xA3, &[2]);
    missing.extend_from_slice(&[0; 8]);
    missing.extend_from_slice(b"/x\0");
    assert!(parse_hash_path(&missing).is_none());
}

#[test]
fn write_op_bodies_frame_opcode_and_nul_terminated_paths() {
    // mkdir (0x11), rmdir (0x12), delete file (0x20): opcode + path NUL.
    assert_eq!(path_op_body(0x11, &["/loops"]), b"\x11/loops\0");
    assert_eq!(path_op_body(0x12, &["/loops"]), b"\x12/loops\0");
    assert_eq!(path_op_body(0x20, &["/loops/K1"]), b"\x20/loops/K1\0");
    // rename (0x21): opcode + src NUL + dst NUL — dst is a full path, so a
    // different directory moves the file.
    assert_eq!(
        path_op_body(0x21, &["/loops/K1", "/oneshots/K1"]),
        b"\x21/loops/K1\0/oneshots/K1\0"
    );
}

#[test]
fn factory_tree_is_recognized_as_protected() {
    assert!(is_protected_drive_path("/factory"));
    assert!(is_protected_drive_path("/FACTORY/kicks"));
    assert!(is_protected_drive_path("factory/x"));
    assert!(!is_protected_drive_path("/"));
    assert!(!is_protected_drive_path("/incoming"));
    assert!(!is_protected_drive_path("/my-factory-picks"));
}

#[test]
fn sample_file_parse_splits_header_and_swaps_pcm_to_le() {
    // 64-byte header: mono, rate 48000, then two BE samples.
    let mut raw = vec![0u8; 64];
    raw[1] = 0; // mono
    raw[8..12].copy_from_slice(&48000u32.to_be_bytes());
    raw.extend_from_slice(&0x1234u16.to_be_bytes());
    raw.extend_from_slice(&0xFFFEu16.to_be_bytes());
    let parsed = parse_sample_file(&raw).expect("parse");
    assert!(!parsed.stereo);
    assert_eq!(parsed.rate, 48000);
    assert_eq!(parsed.pcm_le, vec![0x34, 0x12, 0xFE, 0xFF]);
    // Shorter than the header is not a sample file.
    assert!(parse_sample_file(&[0u8; 10]).is_err());
}

#[test]
fn ram_slot_count_is_clamped_to_the_reply_length() {
    // A garbled count must not drive the allocation: only the entries the
    // reply can actually carry are parsed.
    let mut reply = vec![0, 0, 0, 0, 0x97, 0xFF, 0xFF, 0xFF, 0xFF];
    reply.extend(std::iter::repeat_n(0u8, 11)); // exactly one entry
    let slots = parse_ram_slots(&reply).expect("parses");
    assert_eq!(slots.len(), 1);
}
