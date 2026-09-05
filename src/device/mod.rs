//! Recognizing and watching the Analog Rytm.
//!
//! `detect` holds the pure recognition rules (port-name matching, identity
//! interpretation); `watcher` runs the background thread that polls ports,
//! performs the inquiry handshake, and publishes [`detect::DeviceState`].

pub mod detect;
pub mod watcher;
