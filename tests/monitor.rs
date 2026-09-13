//! Hardware-free tests for the live audio monitor: the SPSC ring, device
//! discovery/config selection, and channel mapping.

use kitsmith::audio::monitor::{
    frame_to_stereo, is_rytm_audio_input, pick_config, ring, stereo_to_frame, ConfigCandidate,
    SampleFormatKind,
};

// --- ring buffer -----------------------------------------------------------

#[test]
fn ring_round_trips_in_order() {
    let (producer, consumer) = ring(8);
    assert_eq!(producer.push_slice(&[1.0, 2.0, 3.0]), 3);
    let mut out = [0.0; 3];
    assert_eq!(consumer.pop_slice(&mut out), 3);
    assert_eq!(out, [1.0, 2.0, 3.0]);
}

#[test]
fn ring_reports_available() {
    let (producer, consumer) = ring(8);
    assert_eq!(consumer.available(), 0);
    producer.push_slice(&[0.5; 5]);
    assert_eq!(consumer.available(), 5);
    let mut out = [0.0; 2];
    consumer.pop_slice(&mut out);
    assert_eq!(consumer.available(), 3);
}

#[test]
fn ring_drops_incoming_when_full_and_never_unread_data() {
    let (producer, consumer) = ring(4);
    assert_eq!(producer.push_slice(&[1.0, 2.0, 3.0, 4.0]), 4);
    // Full: the incoming block is dropped, not the buffered samples.
    assert_eq!(producer.push_slice(&[9.0, 9.0]), 0);
    let mut out = [0.0; 4];
    assert_eq!(consumer.pop_slice(&mut out), 4);
    assert_eq!(out, [1.0, 2.0, 3.0, 4.0]);
}

#[test]
fn ring_pop_from_empty_returns_zero() {
    let (_producer, consumer) = ring(4);
    let mut out = [7.0; 2];
    assert_eq!(consumer.pop_slice(&mut out), 0);
    assert_eq!(out, [7.0, 7.0]); // untouched
}

#[test]
fn ring_wraps_around() {
    let (producer, consumer) = ring(4);
    let mut out = [0.0; 3];
    for round in 0..100 {
        let base = round as f32 * 3.0;
        assert_eq!(producer.push_slice(&[base, base + 1.0, base + 2.0]), 3);
        assert_eq!(consumer.pop_slice(&mut out), 3);
        assert_eq!(out, [base, base + 1.0, base + 2.0]);
    }
}

#[test]
fn ring_rounds_capacity_up_to_power_of_two() {
    // Capacity 5 rounds to 8: six samples must fit.
    let (producer, _consumer) = ring(5);
    assert_eq!(producer.push_slice(&[0.0; 6]), 6);
}

#[test]
fn ring_survives_concurrent_producer_consumer() {
    // Two threads hammer the ring; the consumer must read the exact sequence
    // the producer wrote, in order, with nothing lost or duplicated.
    const TOTAL: usize = 100_000;
    let (producer, consumer) = ring(64);
    let writer = std::thread::spawn(move || {
        let mut next = 0usize;
        while next < TOTAL {
            let end = (next + 7).min(TOTAL);
            let chunk: Vec<f32> = (next..end).map(|i| i as f32).collect();
            let pushed = producer.push_slice(&chunk);
            next += pushed;
            if pushed == 0 {
                std::thread::yield_now();
            }
        }
    });
    let mut expected = 0usize;
    let mut buf = [0.0f32; 13];
    while expected < TOTAL {
        let got = consumer.pop_slice(&mut buf);
        for sample in &buf[..got] {
            assert_eq!(*sample, expected as f32);
            expected += 1;
        }
        if got == 0 {
            std::thread::yield_now();
        }
    }
    writer.join().unwrap();
}

// --- device discovery ------------------------------------------------------

#[test]
fn recognizes_rytm_audio_input_names() {
    assert!(is_rytm_audio_input("Elektron Analog Rytm MKII"));
    assert!(is_rytm_audio_input("Analog Rytm MKII"));
    assert!(is_rytm_audio_input("RYTM"));
    assert!(!is_rytm_audio_input("MacBook Pro Microphone"));
    assert!(!is_rytm_audio_input("Scarlett 2i2 USB"));
    assert!(!is_rytm_audio_input(""));
}

fn candidate(channels: u16, min: u32, max: u32, format: SampleFormatKind) -> ConfigCandidate {
    ConfigCandidate {
        channels,
        min_rate: min,
        max_rate: max,
        format,
    }
}

#[test]
fn pick_config_requires_48khz_support() {
    let candidates = [
        candidate(2, 44_100, 44_100, SampleFormatKind::F32),
        candidate(2, 44_100, 48_000, SampleFormatKind::I16),
    ];
    assert_eq!(pick_config(&candidates), Some(1));
    let only_441 = [candidate(2, 44_100, 44_100, SampleFormatKind::F32)];
    assert_eq!(pick_config(&only_441), None);
}

#[test]
fn pick_config_prefers_f32_stereo() {
    let candidates = [
        candidate(2, 48_000, 48_000, SampleFormatKind::U16),
        candidate(2, 48_000, 48_000, SampleFormatKind::F32),
        candidate(2, 48_000, 48_000, SampleFormatKind::I16),
    ];
    assert_eq!(pick_config(&candidates), Some(1));
}

#[test]
fn pick_config_prefers_stereo_over_wide_over_mono() {
    let candidates = [
        candidate(1, 48_000, 48_000, SampleFormatKind::F32),
        candidate(8, 48_000, 48_000, SampleFormatKind::F32),
        candidate(2, 48_000, 48_000, SampleFormatKind::F32),
    ];
    assert_eq!(pick_config(&candidates), Some(2));
}

#[test]
fn pick_config_accepts_mono_only_device() {
    let candidates = [candidate(1, 8_000, 96_000, SampleFormatKind::I16)];
    assert_eq!(pick_config(&candidates), Some(0));
}

#[test]
fn pick_config_rejects_unknown_formats_and_empty() {
    let candidates = [candidate(2, 48_000, 48_000, SampleFormatKind::Other)];
    assert_eq!(pick_config(&candidates), None);
    assert_eq!(pick_config(&[]), None);
}

// --- channel mapping -------------------------------------------------------

#[test]
fn input_frames_fold_to_stereo() {
    assert_eq!(frame_to_stereo(&[0.3]), (0.3, 0.3)); // mono duplicated
    assert_eq!(frame_to_stereo(&[0.1, -0.2]), (0.1, -0.2));
    assert_eq!(frame_to_stereo(&[0.1, -0.2, 0.9, 0.9]), (0.1, -0.2)); // extras ignored
    assert_eq!(frame_to_stereo(&[]), (0.0, 0.0));
}

#[test]
fn stereo_spreads_to_output_frames() {
    let mut mono = [9.0];
    stereo_to_frame(0.4, 0.2, &mut mono);
    assert!((mono[0] - 0.3).abs() < f32::EPSILON); // averaged

    let mut stereo = [9.0, 9.0];
    stereo_to_frame(0.4, 0.2, &mut stereo);
    assert_eq!(stereo, [0.4, 0.2]);

    let mut wide = [9.0; 5];
    stereo_to_frame(0.4, 0.2, &mut wide);
    assert_eq!(wide, [0.4, 0.2, 0.0, 0.0, 0.0]); // rest silenced
}
