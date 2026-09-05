//! Sysex reassembly: fragmented messages must be stitched back together —
//! the cause of "reply received but not understood" partial syncs.

use rytmdash::midi::link::SysexAssembler;

#[test]
fn whole_message_passes_through() {
    let mut asm = SysexAssembler::default();
    assert_eq!(
        asm.push(&[0xF0, 1, 2, 3, 0xF7]),
        Some(vec![0xF0, 1, 2, 3, 0xF7])
    );
}

#[test]
fn fragmented_message_is_reassembled() {
    let mut asm = SysexAssembler::default();
    assert_eq!(asm.push(&[0xF0, 1, 2]), None);
    assert_eq!(asm.push(&[3, 4]), None);
    assert_eq!(asm.push(&[5, 0xF7]), Some(vec![0xF0, 1, 2, 3, 4, 5, 0xF7]));
}

#[test]
fn lone_terminator_fragment_completes() {
    let mut asm = SysexAssembler::default();
    assert_eq!(asm.push(&[0xF0, 9]), None);
    assert_eq!(asm.push(&[0xF7]), Some(vec![0xF0, 9, 0xF7]));
}

#[test]
fn realtime_bytes_do_not_break_assembly() {
    let mut asm = SysexAssembler::default();
    assert_eq!(asm.push(&[0xF0, 1]), None);
    assert_eq!(asm.push(&[0xF8]), None, "MIDI clock interleaved");
    assert_eq!(asm.push(&[0xFE]), None, "active sensing interleaved");
    assert_eq!(asm.push(&[2, 0xF7]), Some(vec![0xF0, 1, 2, 0xF7]));
}

#[test]
fn stray_continuation_without_start_is_dropped() {
    let mut asm = SysexAssembler::default();
    assert_eq!(asm.push(&[1, 2, 3]), None);
    assert_eq!(asm.push(&[0xF7]), None);
    // A later complete message still works.
    assert_eq!(asm.push(&[0xF0, 7, 0xF7]), Some(vec![0xF0, 7, 0xF7]));
}

#[test]
fn new_start_abandons_unfinished_message() {
    let mut asm = SysexAssembler::default();
    assert_eq!(asm.push(&[0xF0, 1, 2]), None);
    // The device restarted transmission; the stale prefix must not leak in.
    assert_eq!(asm.push(&[0xF0, 8, 0xF7]), Some(vec![0xF0, 8, 0xF7]));
}

#[test]
fn channel_messages_are_ignored() {
    let mut asm = SysexAssembler::default();
    assert_eq!(asm.push(&[0x90, 60, 100]), None, "note-on dropped");
    assert_eq!(asm.push(&[0xF0, 1, 0xF7]), Some(vec![0xF0, 1, 0xF7]));
}
