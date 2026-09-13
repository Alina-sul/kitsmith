//! Pure, hardware-free monitor plumbing: which audio input is the Rytm,
//! which stream config to pick, and how frames map to the stereo path.
//! Kept free of cpal types so all of it unit-tests without devices.

use crate::audio::NATIVE_SAMPLE_RATE;
use crate::device::detect::is_rytm_port;

/// True if an audio input device name identifies the Rytm's class-compliant
/// USB audio interface (same name hint as its MIDI port).
///
/// `name`: the OS-reported audio device name.
pub fn is_rytm_audio_input(name: &str) -> bool {
    is_rytm_port(name)
}

/// The sample formats the monitor can stream; anything else is rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SampleFormatKind {
    F32,
    I16,
    U16,
    Other,
}

/// One supported stream config, reduced to what selection needs.
pub struct ConfigCandidate {
    /// Interleaved channel count the config offers.
    pub channels: u16,
    /// Lowest supported sample rate, Hz.
    pub min_rate: u32,
    /// Highest supported sample rate, Hz.
    pub max_rate: u32,
    /// The config's sample format.
    pub format: SampleFormatKind,
}

/// Picks the best candidate that can run at the Rytm's native 48 kHz:
/// f32 over i16 over u16, stereo over anything else, mono last.
///
/// `candidates`: the device's supported configs.
/// Returns the index of the winner, or `None` if nothing fits.
pub fn pick_config(candidates: &[ConfigCandidate]) -> Option<usize> {
    candidates
        .iter()
        .enumerate()
        .filter(|(_, c)| {
            c.channels >= 1
                && c.format != SampleFormatKind::Other
                && (c.min_rate..=c.max_rate).contains(&NATIVE_SAMPLE_RATE)
        })
        .max_by_key(|(_, c)| {
            let format_rank = match c.format {
                SampleFormatKind::F32 => 2,
                SampleFormatKind::I16 => 1,
                SampleFormatKind::U16 => 0,
                SampleFormatKind::Other => unreachable!("filtered above"),
            };
            let channel_rank = match c.channels {
                2 => 2,
                n if n > 2 => 1,
                _ => 0,
            };
            (channel_rank, format_rank)
        })
        .map(|(i, _)| i)
}

/// Folds one interleaved input frame down to the monitor's stereo pair:
/// mono is duplicated, extra channels beyond L/R are ignored.
///
/// `frame`: one input frame (`channels` samples). Must not be empty.
pub fn frame_to_stereo(frame: &[f32]) -> (f32, f32) {
    match frame {
        [] => (0.0, 0.0),
        [m] => (*m, *m),
        [l, r, ..] => (*l, *r),
    }
}

/// Spreads one stereo pair across an interleaved output frame: mono gets the
/// L/R average, wider frames get L, R and silence on the rest.
///
/// `l`, `r`: the stereo pair.
/// `frame`: one output frame to fill in place.
pub fn stereo_to_frame(l: f32, r: f32, frame: &mut [f32]) {
    match frame {
        [] => {}
        [m] => *m = (l + r) * 0.5,
        [fl, fr, rest @ ..] => {
            *fl = l;
            *fr = r;
            rest.fill(0.0);
        }
    }
}
