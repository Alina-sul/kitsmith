//! The device-mutation interface plans execute against.

use std::fmt;
use std::path::PathBuf;

/// Failure of a single operation step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpError {
    /// The request was invalid before touching the device.
    InvalidRequest(String),
    /// Transport or protocol failure.
    Device(String),
    /// The write went through but read-back disagreed.
    VerificationFailed(String),
}

impl fmt::Display for OpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRequest(msg) => write!(f, "invalid request: {msg}"),
            Self::Device(msg) => write!(f, "device error: {msg}"),
            Self::VerificationFailed(msg) => write!(f, "verification failed: {msg}"),
        }
    }
}

impl std::error::Error for OpError {}

/// Everything a [`super::Plan`] needs from a device. Implemented for the
/// real link ([`super::LiveBackend`]) and mocked in tests.
pub trait DeviceBackend {
    /// Pulls a full object backup (kits, patterns, sounds, globals, settings)
    /// and writes it to disk. Returns the file path.
    fn backup_project(&mut self) -> Result<PathBuf, OpError>;

    /// Sets the sample slot of one kit track's Sound and writes the kit.
    fn write_kit_sample_slot(
        &mut self,
        kit_index: usize,
        track: usize,
        slot: u8,
    ) -> Result<(), OpError>;

    /// Reads the current sample slot of one kit track's Sound from the device.
    fn read_kit_sample_slot(
        &mut self,
        kit_index: usize,
        track: usize,
    ) -> Result<Option<u8>, OpError>;
}
