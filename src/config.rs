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
