//! MIDI Device Inquiry build/parse tests.

use kitsmith::config::ELEKTRON_MANUFACTURER_ID;
use kitsmith::midi::inquiry::{device_inquiry_request, parse_identity_reply};

/// A plausible AR MKII reply: Elektron manufacturer, family 16 (the id
/// elektroid maps to AR MKII), ASCII version "1.72".
fn elektron_reply() -> Vec<u8> {
    vec![
        0xF0, 0x7E, 0x00, 0x06, 0x02, // header + identity reply
        0x00, 0x20, 0x3C, // Elektron
        0x10, 0x00, // family LSB/MSB = 16
        0x01, 0x00, // member LSB/MSB = 1
        b'1', b'.', b'7', b'2', // version
        0xF7,
    ]
}

#[test]
fn request_is_broadcast_identity_request() {
    assert_eq!(
        device_inquiry_request(),
        [0xF0, 0x7E, 0x7F, 0x06, 0x01, 0xF7]
    );
}

#[test]
fn parses_three_byte_manufacturer_reply() {
    let reply = parse_identity_reply(&elektron_reply()).expect("should parse");
    assert_eq!(reply.channel, 0x00);
    assert_eq!(reply.manufacturer, ELEKTRON_MANUFACTURER_ID);
    assert!(reply.is_elektron());
    assert_eq!(reply.family, 16);
    assert_eq!(reply.member, 1);
    assert_eq!(reply.version_string(), "1.72");
}

#[test]
fn parses_single_byte_manufacturer_reply() {
    let msg = [
        0xF0, 0x7E, 0x05, 0x06, 0x02, //
        0x41, // single-byte manufacturer (Roland)
        0x02, 0x01, // family = 2 + (1 << 7) = 130
        0x03, 0x00, // member = 3
        0x01, 0x02, 0x03, 0x04, // non-ASCII version
        0xF7,
    ];
    let reply = parse_identity_reply(&msg).expect("should parse");
    assert_eq!(reply.channel, 0x05);
    assert_eq!(reply.manufacturer, vec![0x41]);
    assert!(!reply.is_elektron());
    assert_eq!(reply.family, 130);
    assert_eq!(reply.member, 3);
    assert_eq!(reply.version_string(), "1.2.3.4");
}

#[test]
fn rejects_malformed_messages() {
    // Wrong sub-id (identity request, not reply).
    assert!(parse_identity_reply(&device_inquiry_request()).is_none());
    // Truncated reply.
    let mut short = elektron_reply();
    short.truncate(10);
    assert!(parse_identity_reply(&short).is_none());
    // Missing end-of-sysex.
    let mut unterminated = elektron_reply();
    *unterminated.last_mut().unwrap() = 0x00;
    assert!(parse_identity_reply(&unterminated).is_none());
    // Not sysex at all.
    assert!(parse_identity_reply(&[0x90, 0x3C, 0x7F]).is_none());
    // Empty.
    assert!(parse_identity_reply(&[]).is_none());
    // Data byte above 0x7F inside the payload.
    let mut bad_payload = elektron_reply();
    bad_payload[8] = 0x80; // family LSB
    assert!(parse_identity_reply(&bad_payload).is_none());
}
