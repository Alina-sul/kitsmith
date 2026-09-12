//! Object-sysex query/reply correlation.
//!
//! Header layout (probed against rytm-rs 0.1.3, which encodes what the device
//! sends): `F0 00 20 3C 07 <dev_id> <obj_type> <container:2> <obj_nr> …`, with
//! a query's command byte sitting 0x10 above its response's object type.

use kitsmith::device::objects::is_reply_to;

fn query(cmd: u8, obj_nr: u8) -> Vec<u8> {
    vec![
        0xF0, 0x00, 0x20, 0x3C, 0x07, 0x00, cmd, 0x01, 0x01, obj_nr, 0, 0, 0, 0x05, 0xF7,
    ]
}

fn reply(kind: u8, obj_nr: u8) -> Vec<u8> {
    vec![
        0xF0, 0x00, 0x20, 0x3C, 0x07, 0x00, kind, 0x01, 0x01, obj_nr, 0, 0,
    ]
}

#[test]
fn matches_the_answer_to_its_own_query() {
    for (cmd, kind) in [
        (0x62, 0x52),
        (0x63, 0x53),
        (0x64, 0x54),
        (0x66, 0x56),
        (0x67, 0x57),
    ] {
        assert!(
            is_reply_to(&reply(kind, 7), &query(cmd, 7)),
            "cmd {cmd:#04x} should be answered by type {kind:#04x}"
        );
    }
}

#[test]
fn rejects_a_late_reply_for_a_different_object() {
    // The failure this guards: kit 5's reply arrives after its query timed
    // out and is consumed as the answer to the kit 6 query in flight.
    assert!(!is_reply_to(&reply(0x52, 5), &query(0x62, 6)));
}

#[test]
fn rejects_a_reply_of_the_wrong_type() {
    assert!(!is_reply_to(&reply(0x54, 7), &query(0x62, 7)));
}

#[test]
fn rejects_messages_too_short_to_carry_a_header() {
    assert!(!is_reply_to(&[0xF0, 0x00, 0x20], &query(0x62, 7)));
    assert!(!is_reply_to(&reply(0x52, 7), &[0xF0, 0x00, 0x20]));
}
