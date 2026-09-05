//! Raw project payload parser: synthetic fixture built at the documented
//! offsets (verified against live AR MKII OS 1.72 downloads).

use rytmdash::device::projfile::{is_raw_project, parse_raw_project};

const PATTERNS_OFFSET: usize = 0x3364;
const PATTERN_SIZE: usize = 0x332D;
const PATTERN_KIT_OFFSET: usize = 0x3325;
const KITS_OFFSET: usize = PATTERNS_OFFSET + 128 * PATTERN_SIZE + 0xA36;
const SAMPLES_OFFSET: usize = 0x1F94C9;

fn fixture() -> Vec<u8> {
    let mut bytes = vec![0u8; SAMPLES_OFFSET + 128 * 16 + 64];
    bytes[..4].copy_from_slice(&[0xAC, 0x11, 0xD3, 0x03]);
    bytes[0x27..0x27 + 7].copy_from_slice(b"MY PROJ");

    // All patterns unsaved (0xFF) except A01 -> kit 0 and B01 -> kit 1.
    for i in 0..128 {
        bytes[PATTERNS_OFFSET + i * PATTERN_SIZE + PATTERN_KIT_OFFSET] = 0xFF;
    }
    bytes[PATTERNS_OFFSET + PATTERN_KIT_OFFSET] = 0;
    bytes[PATTERNS_OFFSET + 16 * PATTERN_SIZE + PATTERN_KIT_OFFSET] = 1;

    // Kit 0 named with one sound name; kit 1 unnamed.
    let kit0 = KITS_OFFSET;
    bytes[kit0 + 0x04..kit0 + 0x04 + 5].copy_from_slice(b"KIT A");
    let sound0 = kit0 + 0x2E + 0x0C;
    bytes[sound0..sound0 + 7].copy_from_slice(b"SOUND 1");

    // Sample slot 3 loaded: hash 0xAABBCCDD, size 1000.
    let entry = SAMPLES_OFFSET + 3 * 16;
    bytes[entry..entry + 4].copy_from_slice(&0xAABB_CCDDu32.to_be_bytes());
    bytes[entry + 4..entry + 8].copy_from_slice(&1000u32.to_be_bytes());
    bytes
}

#[test]
fn parses_name_kits_and_pattern_links() {
    let bytes = fixture();
    assert!(is_raw_project(&bytes));
    let snapshot = parse_raw_project(&bytes).expect("parse");
    assert_eq!(snapshot.name, "MY PROJ");
    assert_eq!(snapshot.kits.len(), 128);
    assert_eq!(snapshot.patterns.len(), 128);
    assert_eq!(snapshot.kits[0].name, "KIT A");
    assert_eq!(snapshot.kits[0].sound_names[0], "SOUND 1");
    assert_eq!(snapshot.kits[1].name, "");
    assert_eq!(snapshot.patterns[0].kit_link, Some(0));
    assert_eq!(snapshot.patterns[16].kit_link, Some(1));
    assert_eq!(snapshot.patterns[1].kit_link, None);
    assert_eq!(snapshot.linked_kits(), vec![0, 1]);
    assert!(snapshot.kit_in_use(0));
    assert!(snapshot.kit_in_use(1), "linked though unnamed");
    assert!(!snapshot.kit_in_use(2));
    assert_eq!(snapshot.samples.len(), 1);
    assert_eq!(snapshot.samples[0].slot, 3);
    assert_eq!(snapshot.samples[0].hash, 0xAABB_CCDD);
    assert_eq!(snapshot.samples[0].size, 1000);
}

#[test]
fn compressed_payloads_are_rejected_not_guessed() {
    // Same magic, but far too short to be the raw layout.
    let mut bytes = vec![0u8; 20_000];
    bytes[..4].copy_from_slice(&[0xAC, 0x11, 0xD3, 0x03]);
    assert!(!is_raw_project(&bytes));
    assert!(parse_raw_project(&bytes).is_none());
}
