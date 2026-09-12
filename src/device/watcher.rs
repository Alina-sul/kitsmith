//! Background device watcher: polls MIDI ports, handshakes, publishes state.
//!
//! One thread loops forever: enumerate ports → if a Rytm port (dis)appeared,
//! update the shared [`DeviceState`] and fire the change callback (the GUI
//! passes a repaint trigger). On appearance it attempts a MIDI Device Inquiry
//! handshake; a silent device is still reported as connected, identified by
//! port name only.

use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;

use midir::{Ignore, MidiInput, MidiOutput};

use crate::config::{INQUIRY_TIMEOUT, MIDI_CLIENT_NAME, PORT_POLL_INTERVAL};
use crate::device::detect::{is_rytm_port, DeviceInfo, DeviceState};
use crate::midi::inquiry::{device_inquiry_request, parse_identity_reply, IdentityReply};
use crate::midi::ports::{self, PortSnapshot};

/// Everything the watcher publishes to the GUI.
#[derive(Debug, Clone, Default)]
pub struct WatcherSnapshot {
    pub device: DeviceState,
    pub ports: PortSnapshot,
}

/// Handle to the background watcher thread.
pub struct DeviceWatcher {
    shared: Arc<Mutex<WatcherSnapshot>>,
}

impl DeviceWatcher {
    /// Spawns the watcher. `on_change` fires after every state update
    /// (used by the GUI to request a repaint).
    pub fn spawn(on_change: impl Fn() + Send + 'static) -> Self {
        let shared = Arc::new(Mutex::new(WatcherSnapshot::default()));
        let thread_shared = Arc::clone(&shared);
        thread::Builder::new()
            .name("device-watcher".into())
            .spawn(move || watch_loop(&thread_shared, &on_change))
            .expect("failed to spawn device watcher thread");
        Self { shared }
    }

    /// The latest published state.
    pub fn snapshot(&self) -> WatcherSnapshot {
        self.shared.lock().expect("watcher lock poisoned").clone()
    }
}

fn watch_loop(shared: &Arc<Mutex<WatcherSnapshot>>, on_change: &impl Fn()) {
    loop {
        let ports = ports::snapshot();
        let rytm_port = ports.inputs.iter().find(|n| is_rytm_port(n)).cloned();

        let previous = shared.lock().expect("watcher lock poisoned").clone();
        let device = next_state(&previous.device, rytm_port.as_deref());

        let changed = device != previous.device || ports != previous.ports;
        if changed {
            *shared.lock().expect("watcher lock poisoned") = WatcherSnapshot { device, ports };
            on_change();
        }
        thread::sleep(PORT_POLL_INTERVAL);
    }
}

/// Decides the new device state for this poll tick, handshaking when a Rytm
/// port has just appeared.
fn next_state(previous: &DeviceState, rytm_port: Option<&str>) -> DeviceState {
    match (previous, rytm_port) {
        // Still connected to the same visible port: keep the identity.
        (DeviceState::Connected(info), Some(port)) if info.port_name == port => previous.clone(),
        // Port appeared (or changed): handshake.
        (_, Some(port)) => DeviceState::Connected(DeviceInfo {
            port_name: port.to_owned(),
            identity: inquire(port),
        }),
        (_, None) => DeviceState::Searching,
    }
}

/// Sends a MIDI Device Inquiry to `port_name` and waits for the reply.
///
/// Returns `None` on any failure — an unidentified Rytm is still usable, so
/// errors here degrade to name-only recognition rather than propagating.
fn inquire(port_name: &str) -> Option<IdentityReply> {
    let mut midi_in = MidiInput::new(MIDI_CLIENT_NAME).ok()?;
    midi_in.ignore(Ignore::None);
    let in_port = midi_in
        .ports()
        .into_iter()
        .find(|p| midi_in.port_name(p).is_ok_and(|n| n == port_name))?;

    let midi_out = MidiOutput::new(MIDI_CLIENT_NAME).ok()?;
    let out_port = midi_out
        .ports()
        .into_iter()
        .find(|p| midi_out.port_name(p).is_ok_and(|n| is_rytm_port(&n)))?;

    let (tx, rx) = mpsc::channel();
    let _in_conn = midi_in
        .connect(
            &in_port,
            "kitsmith-inquiry",
            move |_ts, msg, _| {
                if let Some(reply) = parse_identity_reply(msg) {
                    let _ = tx.send(reply);
                }
            },
            (),
        )
        .ok()?;
    let mut out_conn = midi_out.connect(&out_port, "kitsmith-inquiry").ok()?;
    out_conn.send(&device_inquiry_request()).ok()?;

    rx.recv_timeout(INQUIRY_TIMEOUT).ok()
}
