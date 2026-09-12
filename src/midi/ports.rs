//! OS MIDI port enumeration.

use midir::{MidiInput, MidiOutput};

use crate::config::MIDI_CLIENT_NAME;

/// A snapshot of the MIDI ports visible to the OS.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PortSnapshot {
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
}

/// Enumerates the current input and output port names.
///
/// Errors from the OS MIDI system are reported as an empty snapshot — for a
/// polling watcher, "nothing visible right now" and "enumeration failed" call
/// for the same reaction (retry on the next tick).
pub fn snapshot() -> PortSnapshot {
    let inputs = MidiInput::new(MIDI_CLIENT_NAME)
        .map(|midi_in| {
            midi_in
                .ports()
                .iter()
                .filter_map(|p| midi_in.port_name(p).ok())
                .collect()
        })
        .unwrap_or_default();
    let outputs = MidiOutput::new(MIDI_CLIENT_NAME)
        .map(|midi_out| {
            midi_out
                .ports()
                .iter()
                .filter_map(|p| midi_out.port_name(p).ok())
                .collect()
        })
        .unwrap_or_default();
    PortSnapshot { inputs, outputs }
}
