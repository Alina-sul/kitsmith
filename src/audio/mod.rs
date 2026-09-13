//! Audio: sample import and live monitoring.
//!
//! `convert` + `resample` mirror what Elektron Transfer does on drop: decode →
//! downmix to mono → resample to 48 kHz → 16-bit. `monitor` passes the Rytm's
//! class-compliant USB audio through to the computer's output.

mod convert;
pub mod monitor;
mod resample;

pub use convert::{convert_wav_file, ConvertError, ConvertedSample};
pub use resample::resample;

/// The Rytm's native sample rate.
pub const NATIVE_SAMPLE_RATE: u32 = 48_000;

/// Per-project sample memory budget (manual §5.2.4).
pub const PROJECT_SAMPLE_BUDGET_BYTES: usize = 64 * 1024 * 1024;
