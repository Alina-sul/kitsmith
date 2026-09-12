//! MIDI Sample Dump Standard message tests.

use rytmdash::midi::sds::{
    data_packet, data_packets, dump_header, packet_count, parse_handshake, sample_name,
    unpack_word, Handshake, WORDS_PER_PACKET,
};

#[test]
fn dump_header_layout_for_48k_16bit() {
    let header = dump_header(0, 5, 48_000, 1000);
    assert_eq!(header.len(), 21);
    assert_eq!(&header[..4], &[0xF0, 0x7E, 0x00, 0x01]);
    assert_eq!(&header[4..6], &[5, 0]); // sample number 5
    assert_eq!(header[6], 16); // bits
                               // period = 1e9/48000 = 20833 ns, 21-bit LSB-first
    assert_eq!(&header[7..10], &[0x61, 0x22, 0x01]);
    // length 1000 words
    assert_eq!(
        &header[10..13],
        &[(1000u32 & 0x7F) as u8, (1000u32 >> 7) as u8, 0]
    );
    // no loop: both points at last word, loop type 0x7F
    assert_eq!(&header[13..16], &header[16..19].to_vec()[..]);
    assert_eq!(header[19], 0x7F);
    assert_eq!(header[20], 0xF7);
}

#[test]
fn packets_are_127_bytes_numbered_and_checksummed() {
    let pcm: Vec<i16> = (0..100).map(|n| n * 300).collect(); // 3 packets
    let packets = data_packets(0, &pcm);
    assert_eq!(packets.len(), packet_count(pcm.len()));
    assert_eq!(packets.len(), 3);
    for (i, p) in packets.iter().enumerate() {
        assert_eq!(p.len(), 127);
        assert_eq!(&p[..4], &[0xF0, 0x7E, 0x00, 0x02]);
        assert_eq!(p[4], i as u8);
        // Checksum: XOR from byte 1 (0x7E) through the payload.
        let expected = p[1..125].iter().fold(0u8, |acc, &b| acc ^ b) & 0x7F;
        assert_eq!(p[125], expected);
        assert_eq!(p[126], 0xF7);
        // All data bytes must be 7-bit clean.
        assert!(p[1..126].iter().all(|&b| b <= 0x7F));
    }
}

#[test]
fn packet_numbers_wrap_at_128() {
    let pcm = vec![0i16; WORDS_PER_PACKET * 130];
    let packets = data_packets(3, &pcm);
    assert_eq!(packets[127][4], 127);
    assert_eq!(packets[128][4], 0);
    assert_eq!(packets[129][4], 1);
}

#[test]
fn word_packing_round_trips_extremes() {
    for value in [i16::MIN, -1, 0, 1, 12345, i16::MAX] {
        let packet = data_packet(0, 0, &[value]);
        let bytes = [packet[5], packet[6], packet[7]];
        assert_eq!(unpack_word(bytes), value, "value {value}");
    }
}

#[test]
fn sample_name_message_layout() {
    let msg = sample_name(0, 130, "Kick 01");
    assert_eq!(&msg[..5], &[0xF0, 0x7E, 0x00, 0x05, 0x03]);
    assert_eq!(&msg[5..7], &[130 & 0x7F, 1]); // sample number 130 = 0x02,0x01
    assert_eq!(msg[7], 7); // length
    assert_eq!(&msg[8..15], b"Kick 01");
    assert_eq!(*msg.last().unwrap(), 0xF7);
}

#[test]
fn sample_name_strips_non_ascii() {
    let msg = sample_name(0, 0, "汉字kick");
    assert_eq!(msg[7], 4);
    assert_eq!(&msg[8..12], b"kick");
}

#[test]
fn parses_handshakes() {
    assert_eq!(
        parse_handshake(&[0xF0, 0x7E, 0, 0x7F, 9, 0xF7]),
        Some(Handshake::Ack(9))
    );
    assert_eq!(
        parse_handshake(&[0xF0, 0x7E, 0, 0x7E, 2, 0xF7]),
        Some(Handshake::Nak(2))
    );
    assert_eq!(
        parse_handshake(&[0xF0, 0x7E, 0, 0x7D, 0, 0xF7]),
        Some(Handshake::Cancel(0))
    );
    assert_eq!(
        parse_handshake(&[0xF0, 0x7E, 0, 0x7C, 1, 0xF7]),
        Some(Handshake::Wait(1))
    );
    // A dump header is not a handshake.
    assert_eq!(parse_handshake(&dump_header(0, 0, 48_000, 10)), None);
    assert_eq!(parse_handshake(&[]), None);
}

#[test]
fn short_final_packet_is_padded_with_encoded_silence() {
    // One word short of a full packet: the tail must decode to silence, not
    // to the -32768 that a raw-zero triplet would mean in offset encoding.
    let pcm = vec![1234i16; WORDS_PER_PACKET - 1];
    let packets = data_packets(0, &pcm);
    assert_eq!(packets.len(), 1);
    let tail = [
        packets[0][5 + 3 * (WORDS_PER_PACKET - 1)],
        packets[0][6 + 3 * (WORDS_PER_PACKET - 1)],
        packets[0][7 + 3 * (WORDS_PER_PACKET - 1)],
    ];
    assert_eq!(unpack_word(tail), 0);
}
