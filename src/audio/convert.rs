//! WAV → Rytm-native conversion pipeline (decode → mono → 48 kHz → 16-bit).

use std::fmt;
use std::path::Path;

use hound::{SampleFormat, WavReader};

use super::{resample, NATIVE_SAMPLE_RATE};

/// A sample converted to the Rytm's native format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConvertedSample {
    /// Suggested device name: source file stem.
    pub name: String,
    /// 16-bit mono PCM at 48 kHz.
    pub pcm: Vec<i16>,
    pub source_rate: u32,
    pub source_channels: u16,
}

impl ConvertedSample {
    /// Length in seconds at the native rate.
    pub fn seconds(&self) -> f64 {
        self.pcm.len() as f64 / f64::from(NATIVE_SAMPLE_RATE)
    }

    /// Size in bytes as it counts against the project sample budget.
    pub fn byte_size(&self) -> usize {
        self.pcm.len() * 2
    }
}

/// Conversion failure.
#[derive(Debug)]
pub enum ConvertError {
    /// Not a readable WAV file.
    Read(String),
    /// The file decoded to zero samples.
    Empty,
}

impl fmt::Display for ConvertError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(err) => write!(f, "cannot read WAV: {err}"),
            Self::Empty => write!(f, "file contains no audio"),
        }
    }
}

impl std::error::Error for ConvertError {}

/// Largest integer sample magnitude for a bit depth, as an f64 divisor.
///
/// `bits_per_sample` comes from the file: hound validates it for plain `fmt`
/// chunks but copies `wValidBitsPerSample` through unchecked for
/// WAVE_FORMAT_EXTENSIBLE, so a non-standard header can present a depth of 0
/// or well past 32. Clamping keeps the shift in range instead of panicking.
fn int_full_scale(bits_per_sample: u16) -> f64 {
    let bits = bits_per_sample.clamp(1, 32);
    f64::from(1u32 << (bits - 1))
}

/// Converts a WAV file (any rate, channel count, int/float format) to
/// 16-bit / 48 kHz / mono — the Rytm's native sample format.
pub fn convert_wav_file(path: &Path) -> Result<ConvertedSample, ConvertError> {
    let mut reader = WavReader::open(path).map_err(|e| ConvertError::Read(e.to_string()))?;
    let spec = reader.spec();

    // Decode to normalized f64 frames.
    let interleaved: Vec<f64> = match spec.sample_format {
        SampleFormat::Float => reader
            .samples::<f32>()
            .map(|s| s.map(f64::from))
            .collect::<Result<_, _>>(),
        SampleFormat::Int => {
            let scale = int_full_scale(spec.bits_per_sample);
            reader
                .samples::<i32>()
                .map(|s| s.map(|v| f64::from(v) / scale))
                .collect::<Result<_, _>>()
        }
    }
    .map_err(|e| ConvertError::Read(e.to_string()))?;
    if interleaved.is_empty() {
        return Err(ConvertError::Empty);
    }

    // Downmix to mono by averaging channels, like Transfer's summing.
    let channels = usize::from(spec.channels.max(1));
    let mono: Vec<f64> = interleaved
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f64>() / frame.len() as f64)
        .collect();

    let resampled = resample(&mono, spec.sample_rate, NATIVE_SAMPLE_RATE);

    let pcm = resampled
        .iter()
        .map(|&s| (s.clamp(-1.0, 1.0) * f64::from(i16::MAX)).round() as i16)
        .collect();

    let name = path
        .file_stem()
        .map_or_else(|| "sample".to_owned(), |s| s.to_string_lossy().into_owned());

    Ok(ConvertedSample {
        name,
        pcm,
        source_rate: spec.sample_rate,
        source_channels: spec.channels,
    })
}
