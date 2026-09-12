//! Audio import pipeline tests: WAV decode, downmix, resample, 16-bit out.

use std::f64::consts::TAU;
use std::path::PathBuf;

use hound::{SampleFormat, WavSpec, WavWriter};
use rytmdash::audio::{convert_wav_file, resample, NATIVE_SAMPLE_RATE};

fn temp_wav(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rytmdash-audio-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir.join(name)
}

/// Writes a sine WAV and returns its path.
fn write_sine_wav(
    name: &str,
    rate: u32,
    channels: u16,
    bits: u16,
    format: SampleFormat,
    freq: f64,
    seconds: f64,
) -> PathBuf {
    let path = temp_wav(name);
    let spec = WavSpec {
        channels,
        sample_rate: rate,
        bits_per_sample: bits,
        sample_format: format,
    };
    let mut writer = WavWriter::create(&path, spec).expect("create wav");
    let frames = (seconds * f64::from(rate)) as usize;
    for n in 0..frames {
        let value = 0.5 * (TAU * freq * n as f64 / f64::from(rate)).sin();
        for _ in 0..channels {
            match format {
                SampleFormat::Float => writer.write_sample(value as f32).expect("write"),
                SampleFormat::Int => {
                    let scale = f64::from(1u32 << (bits - 1)) - 1.0;
                    writer.write_sample((value * scale) as i32).expect("write");
                }
            }
        }
    }
    writer.finalize().expect("finalize wav");
    path
}

/// Estimates a sine's frequency from zero crossings.
fn estimated_freq(pcm: &[i16], rate: u32) -> f64 {
    let crossings = pcm.windows(2).filter(|w| (w[0] < 0) != (w[1] < 0)).count();
    crossings as f64 * f64::from(rate) / (2.0 * pcm.len() as f64)
}

#[test]
fn converts_stereo_44k_int16_to_native() {
    let path = write_sine_wav("st44k.wav", 44_100, 2, 16, SampleFormat::Int, 440.0, 0.5);
    let converted = convert_wav_file(&path).expect("convert");
    std::fs::remove_file(&path).ok();

    // Length scaled by the rate ratio (±1 sample).
    let expected = (0.5 * f64::from(NATIVE_SAMPLE_RATE)) as usize;
    assert!((converted.pcm.len() as isize - expected as isize).abs() <= 1);
    // Pitch preserved.
    let freq = estimated_freq(&converted.pcm, NATIVE_SAMPLE_RATE);
    assert!((freq - 440.0).abs() < 5.0, "freq drifted to {freq}");
    // Amplitude preserved (peak ~0.5 FS).
    let peak = converted
        .pcm
        .iter()
        .map(|&s| i32::from(s).abs())
        .max()
        .unwrap();
    assert!((12000..=20000).contains(&peak), "peak {peak}");
    assert_eq!(converted.source_rate, 44_100);
    assert_eq!(converted.source_channels, 2);
    assert_eq!(converted.name, "st44k");
}

#[test]
fn converts_float_and_24_bit_formats() {
    for (name, bits, format) in [
        ("f32.wav", 32, SampleFormat::Float),
        ("i24.wav", 24, SampleFormat::Int),
    ] {
        let path = write_sine_wav(name, 48_000, 1, bits, format, 220.0, 0.25);
        let converted = convert_wav_file(&path).expect(name);
        std::fs::remove_file(&path).ok();
        let freq = estimated_freq(&converted.pcm, NATIVE_SAMPLE_RATE);
        assert!((freq - 220.0).abs() < 5.0, "{name}: freq {freq}");
    }
}

#[test]
fn matching_rate_passes_through_resampler() {
    let input: Vec<f64> = (0..100).map(|n| f64::from(n) / 100.0).collect();
    assert_eq!(resample(&input, 48_000, 48_000), input);
}

#[test]
fn resampler_preserves_frequency_upsampling_and_downsampling() {
    for (from, to) in [(44_100u32, 48_000u32), (96_000, 48_000)] {
        let frames = from / 2; // 0.5 s
        let input: Vec<f64> = (0..frames)
            .map(|n| (TAU * 1000.0 * f64::from(n) / f64::from(from)).sin())
            .collect();
        let output = resample(&input, from, to);
        let pcm: Vec<i16> = output.iter().map(|&s| (s * 32767.0) as i16).collect();
        let freq = estimated_freq(&pcm, to);
        assert!((freq - 1000.0).abs() < 10.0, "{from}->{to}: freq {freq}");
    }
}

#[test]
fn rejects_non_wav_files() {
    let path = temp_wav("not-audio.wav");
    std::fs::write(&path, b"definitely not a wav").unwrap();
    assert!(convert_wav_file(&path).is_err());
    std::fs::remove_file(&path).ok();
}
