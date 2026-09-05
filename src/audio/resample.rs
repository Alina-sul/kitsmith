//! Windowed-sinc sample-rate conversion.
//!
//! Quality-over-speed is the right trade here: conversion happens once per
//! imported file, off the UI thread, on material that ends up in a track.

use std::f64::consts::PI;

/// Half-width of the sinc kernel in source samples (2N+1 taps per output).
const KERNEL_HALF_WIDTH: usize = 24;

/// Resamples `input` from `from_rate` to `to_rate` using a Hann-windowed sinc
/// kernel, low-passed to the lower Nyquist for downsampling.
///
/// Returns the input unchanged when the rates already match.
pub fn resample(input: &[f64], from_rate: u32, to_rate: u32) -> Vec<f64> {
    if from_rate == to_rate || input.is_empty() {
        return input.to_vec();
    }
    let ratio = f64::from(to_rate) / f64::from(from_rate);
    // Cut off at the lower of the two Nyquist frequencies (as a fraction of
    // the *source* rate) so downsampling does not alias.
    let cutoff = 0.5 * ratio.min(1.0);
    let out_len = ((input.len() as f64) * ratio).round() as usize;

    let mut output = Vec::with_capacity(out_len);
    for n in 0..out_len {
        // Position of this output sample on the source timeline.
        let center = n as f64 / ratio;
        let first = (center.floor() as isize) - KERNEL_HALF_WIDTH as isize;
        let last = (center.floor() as isize) + KERNEL_HALF_WIDTH as isize + 1;

        let mut acc = 0.0;
        let mut weight_sum = 0.0;
        for i in first..=last {
            let Some(&sample) = usize::try_from(i).ok().and_then(|i| input.get(i)) else {
                continue;
            };
            let offset = center - i as f64;
            let weight = windowed_sinc(offset, cutoff, (last - first) as f64 / 2.0);
            acc += sample * weight;
            weight_sum += weight;
        }
        // Normalizing by the actual weight sum keeps edges and non-integer
        // ratios gain-accurate.
        output.push(if weight_sum.abs() > f64::EPSILON {
            acc / weight_sum
        } else {
            0.0
        });
    }
    output
}

/// Low-passed sinc, Hann-windowed over `half_width` source samples.
fn windowed_sinc(offset: f64, cutoff: f64, half_width: f64) -> f64 {
    if offset.abs() >= half_width {
        return 0.0;
    }
    let window = 0.5 * (1.0 + (PI * offset / half_width).cos());
    sinc(2.0 * cutoff * offset) * window
}

fn sinc(x: f64) -> f64 {
    if x.abs() < 1e-12 {
        1.0
    } else {
        (PI * x).sin() / (PI * x)
    }
}
