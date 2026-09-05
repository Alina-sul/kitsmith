//! Pure device-recognition rules — no I/O, unit-testable.

use crate::config::RYTM_PORT_NAME_HINT;
use crate::midi::inquiry::IdentityReply;

/// True when a MIDI port name looks like an Analog Rytm.
///
/// macOS names the port "Analog Rytm MKII" (sometimes prefixed "Elektron");
/// matching on the distinctive substring covers both and MKI.
pub fn is_rytm_port(name: &str) -> bool {
    name.to_lowercase().contains(RYTM_PORT_NAME_HINT)
}

/// What the watcher currently knows about the device.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum DeviceState {
    /// No Rytm-looking port is visible.
    #[default]
    Searching,
    /// A Rytm port is present. `identity` is `Some` when the device answered
    /// the MIDI Device Inquiry, `None` when it was recognized by port name
    /// only (e.g. inquiry timed out).
    Connected(DeviceInfo),
}

/// A recognized device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    pub port_name: String,
    pub identity: Option<IdentityReply>,
}

impl DeviceInfo {
    /// One-line description for display/logging.
    pub fn describe(&self) -> String {
        match &self.identity {
            Some(id) if id.is_elektron() => format!(
                "{} · Elektron (family {}, member {}) · OS {}",
                self.port_name,
                id.family,
                id.member,
                id.version_string()
            ),
            Some(id) => format!(
                "{} · manufacturer {:02X?} · version {}",
                self.port_name,
                id.manufacturer,
                id.version_string()
            ),
            None => format!(
                "{} · identified by port name (no inquiry reply)",
                self.port_name
            ),
        }
    }
}
