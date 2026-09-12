//! MIDI Sample Dump Standard (SDS) + the sample-name extension — pure
//! message building/parsing, no I/O.
//!
//! This is the openly documented transport the Rytm accepts for sample upload
//! (manual §14.2.4; uploads land in the +Drive `INCOMING/` directory or the
//! directory marked UPLOAD HERE). Format details cross-checked against the
//! MIDI 1.0 SDS spec and elektroid's implementation (read as spec, GPLv3).

/// Data bytes carried by one packet.
pub const PACKET_PAYLOAD_LEN: usize = 120;

/// 16-bit words per packet: each word packs into three 7-bit bytes.
pub const WORDS_PER_PACKET: usize = PACKET_PAYLOAD_LEN / 3;

/// Loop-type value meaning "no loop".
const LOOP_TYPE_OFF: u8 = 0x7F;

/// Builds the 21-byte Dump Header for a 16-bit mono sample.
///
/// `sample_number` is the 14-bit target slot on the receiver; `sample_rate`
/// the playback rate in Hz; `length_words` the sample length in words.
pub fn dump_header(
    channel: u8,
    sample_number: u16,
    sample_rate: u32,
    length_words: u32,
) -> Vec<u8> {
    let period_ns = 1_000_000_000 / sample_rate;
    // No sustain loop: both points at the last word, loop type off.
    let loop_point = length_words.saturating_sub(1);
    let mut msg = vec![
        0xF0,
        0x7E,
        channel & 0x7F,
        0x01,
        (sample_number & 0x7F) as u8,
        (sample_number >> 7) as u8 & 0x7F,
        16, // significant bits per word
    ];
    push_u21(&mut msg, period_ns);
    push_u21(&mut msg, length_words);
    push_u21(&mut msg, loop_point);
    push_u21(&mut msg, loop_point);
    msg.push(LOOP_TYPE_OFF);
    msg.push(0xF7);
    msg
}

/// Splits PCM into numbered, checksummed data packets (packet numbers wrap
/// at 128; the final packet is zero-padded).
pub fn data_packets(channel: u8, pcm: &[i16]) -> Vec<Vec<u8>> {
    pcm.chunks(WORDS_PER_PACKET)
        .enumerate()
        .map(|(i, words)| data_packet(channel, (i % 128) as u8, words))
        .collect()
}

/// Builds one 127-byte data packet from up to [`WORDS_PER_PACKET`] words.
pub fn data_packet(channel: u8, packet_number: u8, words: &[i16]) -> Vec<u8> {
    debug_assert!(words.len() <= WORDS_PER_PACKET);
    let mut msg = Vec::with_capacity(127);
    msg.extend_from_slice(&[0xF0, 0x7E, channel & 0x7F, 0x02, packet_number & 0x7F]);
    for &word in words {
        push_word(&mut msg, word);
    }
    // Pad with *encoded silence*, not raw zeros: words are unsigned-offset, so
    // a zero triplet decodes to -32768 — a full-scale DC click on the tail of
    // any sample whose length is not a multiple of WORDS_PER_PACKET.
    while msg.len() < 5 + PACKET_PAYLOAD_LEN {
        push_word(&mut msg, 0);
    }
    // Checksum: XOR of everything from 0x7E through the payload, in 7 bits.
    let checksum = msg[1..].iter().fold(0u8, |acc, &b| acc ^ b) & 0x7F;
    msg.push(checksum);
    msg.push(0xF7);
    msg
}

/// Builds a Sample Name Transmission (SDS extension) for the given slot.
pub fn sample_name(channel: u8, sample_number: u16, name: &str) -> Vec<u8> {
    let ascii: Vec<u8> = name
        .chars()
        .filter(|c| c.is_ascii_graphic() || *c == ' ')
        .take(127)
        .map(|c| c as u8)
        .collect();
    let mut msg = vec![
        0xF0,
        0x7E,
        channel & 0x7F,
        0x05,
        0x03,
        (sample_number & 0x7F) as u8,
        (sample_number >> 7) as u8 & 0x7F,
        ascii.len() as u8,
    ];
    msg.extend_from_slice(&ascii);
    msg.push(0xF7);
    msg
}

/// A receiver's handshake reply to a header or data packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handshake {
    /// Packet received correctly — send the next one.
    Ack(u8),
    /// Packet corrupt — resend it.
    Nak(u8),
    /// Abort the dump.
    Cancel(u8),
    /// Receiver busy — pause until it ACKs.
    Wait(u8),
}

/// Parses an SDS handshake message, if `msg` is one.
pub fn parse_handshake(msg: &[u8]) -> Option<Handshake> {
    let &[0xF0, 0x7E, _channel, kind, packet, 0xF7] = msg else {
        return None;
    };
    match kind {
        0x7F => Some(Handshake::Ack(packet)),
        0x7E => Some(Handshake::Nak(packet)),
        0x7D => Some(Handshake::Cancel(packet)),
        0x7C => Some(Handshake::Wait(packet)),
        _ => None,
    }
}

/// Number of data packets a sample of `words` words will need.
pub fn packet_count(words: usize) -> usize {
    words.div_ceil(WORDS_PER_PACKET)
}

/// Appends a 21-bit value as three 7-bit bytes, LSB first.
fn push_u21(msg: &mut Vec<u8>, value: u32) {
    msg.push((value & 0x7F) as u8);
    msg.push((value >> 7) as u8 & 0x7F);
    msg.push((value >> 14) as u8 & 0x7F);
}

/// Appends one 16-bit word, unsigned and left-justified in 21 bits,
/// as three 7-bit bytes MSB first (per the SDS spec).
fn push_word(msg: &mut Vec<u8>, word: i16) {
    let unsigned = (i32::from(word) + 0x8000) as u32; // 16-bit unsigned
    let justified = unsigned << 5; // left-justified in 21 bits
    msg.push((justified >> 14) as u8 & 0x7F);
    msg.push((justified >> 7) as u8 & 0x7F);
    msg.push(justified as u8 & 0x7F);
}

/// Decodes one packed word (test/verification helper, inverse of the packer).
pub fn unpack_word(bytes: [u8; 3]) -> i16 {
    let justified = (u32::from(bytes[0]) << 14) | (u32::from(bytes[1]) << 7) | u32::from(bytes[2]);
    ((justified >> 5) as i32 - 0x8000) as i16
}
