//! MIDI Device Inquiry (universal non-realtime sysex), build + parse only.
//!
//! No I/O here — callers send [`device_inquiry_request`] over any transport and
//! feed received sysex bytes to [`parse_identity_reply`]. Kept pure so the
//! protocol logic is unit-testable without hardware.

use crate::config::ELEKTRON_MANUFACTURER_ID;

/// Universal non-realtime Identity Request, addressed to all devices (0x7F).
const IDENTITY_REQUEST: [u8; 6] = [0xF0, 0x7E, 0x7F, 0x06, 0x01, 0xF7];

/// Sub-IDs of the General Information message pair.
const SUB_ID_GENERAL_INFORMATION: u8 = 0x06;
const SUB_ID_IDENTITY_REPLY: u8 = 0x02;

/// Returns the sysex bytes of a broadcast MIDI Device Inquiry request.
pub fn device_inquiry_request() -> [u8; 6] {
    IDENTITY_REQUEST
}

/// A parsed Identity Reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityReply {
    /// Device/sysex channel the reply was addressed from.
    pub channel: u8,
    /// Manufacturer id: 1 byte, or 3 bytes when the first byte is 0x00.
    pub manufacturer: Vec<u8>,
    /// Device family (14-bit, LSB-first on the wire).
    pub family: u16,
    /// Family member (14-bit, LSB-first on the wire).
    pub member: u16,
    /// Raw firmware/OS version bytes as sent by the device.
    pub version: [u8; 4],
}

impl IdentityReply {
    /// True when the manufacturer id is Elektron's.
    pub fn is_elektron(&self) -> bool {
        self.manufacturer == ELEKTRON_MANUFACTURER_ID
    }

    /// Human-readable version: ASCII bytes as text (e.g. "1.72"), otherwise
    /// dotted decimal.
    pub fn version_string(&self) -> String {
        if self
            .version
            .iter()
            .all(|&b| b.is_ascii_graphic() || b == b' ')
        {
            self.version
                .iter()
                .map(|&b| b as char)
                .collect::<String>()
                .trim()
                .to_owned()
        } else {
            let [a, b, c, d] = self.version;
            format!("{a}.{b}.{c}.{d}")
        }
    }
}

/// Parses a sysex message as an Identity Reply.
///
/// `msg` is one complete sysex message including 0xF0/0xF7 framing. Returns
/// `None` for anything that is not a well-formed Identity Reply.
pub fn parse_identity_reply(msg: &[u8]) -> Option<IdentityReply> {
    // F0 7E <ch> 06 02 <mfr:1|3> <family:2> <member:2> <version:4> F7
    let [0xF0, 0x7E, channel, SUB_ID_GENERAL_INFORMATION, SUB_ID_IDENTITY_REPLY, rest @ ..] = msg
    else {
        return None;
    };
    let (manufacturer, rest) = match rest {
        [0x00, a, b, tail @ ..] => (vec![0x00, *a, *b], tail),
        [m, tail @ ..] => (vec![*m], tail),
        [] => return None,
    };
    let &[fam_l, fam_m, mem_l, mem_m, v0, v1, v2, v3, 0xF7] = rest else {
        return None;
    };
    if [fam_l, fam_m, mem_l, mem_m].iter().any(|&b| b > 0x7F) {
        return None;
    }
    Some(IdentityReply {
        channel: *channel,
        manufacturer,
        family: u16::from(fam_l) | (u16::from(fam_m) << 7),
        member: u16::from(mem_l) | (u16::from(mem_m) << 7),
        version: [v0, v1, v2, v3],
    })
}
