//! Central tunable constants — single source of truth for behavior knobs.

use std::time::Duration;

/// How often the device watcher re-enumerates MIDI ports.
pub const PORT_POLL_INTERVAL: Duration = Duration::from_secs(1);

/// How long to wait for a MIDI Device Inquiry reply before treating the
/// device as identified-by-port-name only.
pub const INQUIRY_TIMEOUT: Duration = Duration::from_millis(750);

/// Elektron's 3-byte MIDI manufacturer id (as it appears in sysex).
pub const ELEKTRON_MANUFACTURER_ID: [u8; 3] = [0x00, 0x20, 0x3C];

/// Case-insensitive substring that identifies an Analog Rytm USB-MIDI port.
pub const RYTM_PORT_NAME_HINT: &str = "rytm";

/// Client name rytmdash registers with the OS MIDI system.
pub const MIDI_CLIENT_NAME: &str = "rytmdash";

/// How long to wait for the reply to one object query during sync.
pub const OBJECT_REPLY_TIMEOUT: Duration = Duration::from_millis(2000);

/// Consecutive unanswered object queries before a sync gives up.
pub const SYNC_MISS_LIMIT: usize = 8;

/// How long to wait for an SDS handshake (ACK/NAK/WAIT) per packet.
pub const SDS_HANDSHAKE_TIMEOUT: Duration = Duration::from_millis(500);

/// Inter-packet rest when the receiver never handshakes (open-loop mode).
pub const SDS_OPEN_LOOP_REST: Duration = Duration::from_millis(20);

/// NAK-driven resends of one packet before giving up.
pub const SDS_RETRY_LIMIT: usize = 8;

/// How long to wait for a file-protocol reply.
pub const FILE_REPLY_TIMEOUT: Duration = Duration::from_millis(2000);
