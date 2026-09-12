//! A persistent duplex sysex link to one device.
//!
//! Owns an open input+output connection pair; received sysex messages queue on
//! an internal channel. This is the transport the sync engine drives — send a
//! query, then drain replies with a timeout.

use std::fmt;
use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use midir::{Ignore, MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};

use crate::config::MIDI_CLIENT_NAME;

/// Failure to open or use the link.
#[derive(Debug)]
pub enum LinkError {
    /// The named port was not found (device unplugged?).
    PortNotFound(String),
    /// The OS MIDI system refused the operation.
    Midi(String),
}

impl fmt::Display for LinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PortNotFound(name) => write!(f, "MIDI port not found: {name}"),
            Self::Midi(err) => write!(f, "MIDI error: {err}"),
        }
    }
}

impl std::error::Error for LinkError {}

/// Reassembles sysex messages from transport fragments.
///
/// CoreMIDI (and other backends) may split one sysex message across several
/// packets: only the first starts with `F0`, only the last ends with `F7`,
/// and continuation fragments begin with plain data bytes. Real-time status
/// bytes (`F8..=FF`) may interleave anywhere and must not break assembly.
#[derive(Default)]
pub struct SysexAssembler {
    pending: Vec<u8>,
}

impl SysexAssembler {
    /// Feeds one transport chunk; returns a complete `F0…F7` message when
    /// one finishes.
    pub fn push(&mut self, msg: &[u8]) -> Option<Vec<u8>> {
        let first = *msg.first()?;
        if first == 0xF0 {
            // New message starts (any unfinished one is abandoned).
            self.pending.clear();
            self.pending.extend_from_slice(msg);
        } else if first < 0x80 || first == 0xF7 {
            // Continuation data (or a lone terminator) for an open message.
            if self.pending.is_empty() {
                return None;
            }
            self.pending.extend_from_slice(msg);
        } else {
            // Real-time or channel message — not part of the sysex stream.
            return None;
        }
        if self.pending.last() == Some(&0xF7) {
            return Some(std::mem::take(&mut self.pending));
        }
        None
    }
}

/// An open in+out connection pair to one device.
pub struct MidiLink {
    out: MidiOutputConnection,
    rx: Receiver<Vec<u8>>,
    // Held to keep the input callback alive.
    _input: MidiInputConnection<()>,
}

impl MidiLink {
    /// Opens input and output connections to the ports named `in_port` and
    /// `out_port`, queueing every received sysex message internally.
    pub fn open(in_port: &str, out_port: &str) -> Result<Self, LinkError> {
        let mut midi_in =
            MidiInput::new(MIDI_CLIENT_NAME).map_err(|e| LinkError::Midi(e.to_string()))?;
        midi_in.ignore(Ignore::None);
        let in_p = midi_in
            .ports()
            .into_iter()
            .find(|p| midi_in.port_name(p).is_ok_and(|n| n == in_port))
            .ok_or_else(|| LinkError::PortNotFound(in_port.to_owned()))?;

        let midi_out =
            MidiOutput::new(MIDI_CLIENT_NAME).map_err(|e| LinkError::Midi(e.to_string()))?;
        let out_p = midi_out
            .ports()
            .into_iter()
            .find(|p| midi_out.port_name(p).is_ok_and(|n| n == out_port))
            .ok_or_else(|| LinkError::PortNotFound(out_port.to_owned()))?;

        let (tx, rx) = mpsc::channel();
        let mut assembler = SysexAssembler::default();
        let input = midi_in
            .connect(
                &in_p,
                "kitsmith-link-in",
                move |_ts, msg, _| {
                    // Large sysex arrives fragmented across packets;
                    // reassemble before queueing. Non-sysex is dropped.
                    if let Some(complete) = assembler.push(msg) {
                        let _ = tx.send(complete);
                    }
                },
                (),
            )
            .map_err(|e| LinkError::Midi(e.to_string()))?;
        let out = midi_out
            .connect(&out_p, "kitsmith-link-out")
            .map_err(|e| LinkError::Midi(e.to_string()))?;

        Ok(Self {
            out,
            rx,
            _input: input,
        })
    }

    /// Sends one message to the device.
    pub fn send(&mut self, bytes: &[u8]) -> Result<(), LinkError> {
        self.out
            .send(bytes)
            .map_err(|e| LinkError::Midi(e.to_string()))
    }

    /// Waits up to `timeout` for the next queued sysex message.
    pub fn recv_timeout(&self, timeout: Duration) -> Option<Vec<u8>> {
        self.rx.recv_timeout(timeout).ok()
    }
}
