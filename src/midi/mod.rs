//! MIDI transport layer: pure sysex build/parse plus OS port enumeration.
//!
//! Everything device-agnostic lives here; recognizing *the Rytm* specifically
//! is `device`'s job.

pub mod inquiry;
pub mod link;
pub mod ports;
pub mod sds;
