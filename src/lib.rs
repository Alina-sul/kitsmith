//! kitsmith — smart dashboard for the Elektron Analog Rytm MKII.
//!
//! The library hosts everything so integration tests can exercise the core:
//! [`midi`] (transport-level sysex + port enumeration), [`device`] (recognizing
//! and watching the Rytm), and the private `gui` (egui dashboard shell).

pub mod analysis;
pub mod audio;
pub mod cache;
pub mod config;
pub mod device;
pub mod localmeta;
pub mod midi;
pub mod model;
pub mod ops;
pub mod transfer;

mod gui;

/// Launches the dashboard application. Blocks until the window closes.
pub fn run() -> eframe::Result {
    gui::run()
}
