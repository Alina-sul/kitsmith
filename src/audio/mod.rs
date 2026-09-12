//! Audio import: converting arbitrary WAV files to the Rytm's native format.
//!
//! Mirrors what Elektron Transfer does on drop: decode → downmix to mono →
//! resample to 48 kHz → 16-bit. `convert` is the pipeline; `resample` the
//! hand-rolled windowed-sinc resampler it uses.

mod convert;
mod resample;

pub use convert::{convert_wav_file, ConvertError, ConvertedSample};
pub use resample::resample;

/// The Rytm's native sample rate.
pub const NATIVE_SAMPLE_RATE: u32 = 48_000;

/// Per-project sample memory budget (manual §5.2.4).
pub const PROJECT_SAMPLE_BUDGET_BYTES: usize = 64 * 1024 * 1024;
