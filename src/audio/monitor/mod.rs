//! Live audio monitor: the Rytm's class-compliant USB audio input (GLOBAL
//! SETTINGS → SYSTEM → USB CONFIG → USB AUDIO/MIDI, OS 1.50+) passed through
//! to the computer's default output, so pressing Play on the device is heard
//! in the app. Read-only — no sysex, no device writes.
//!
//! One dedicated `audio-monitor` thread owns both cpal streams (`Stream` is
//! `!Send`); the realtime callbacks exchange interleaved stereo f32 via the
//! lock-free [`ring`]. The GUI talks to the thread through [`MonitorHandle`].

mod discover;
mod ring;

pub use discover::{
    frame_to_stereo, is_rytm_audio_input, pick_config, stereo_to_frame, ConfigCandidate,
    SampleFormatKind,
};
pub use ring::{ring, Consumer, Producer};

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SampleRate, SizedSample};

use crate::audio::NATIVE_SAMPLE_RATE;
use crate::config::{MONITOR_CHANNELS, MONITOR_PREFILL_FRAMES, MONITOR_RING_FRAMES};

/// Frames a realtime callback stages at once (bounds the scratch buffers —
/// no allocation happens inside a callback).
const SCRATCH_FRAMES: usize = 4096;

/// Why the monitor is not running.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MonitorError {
    /// No audio input device matching the Rytm was found.
    NoRytmAudioInput,
    /// The computer has no default output device.
    NoOutputDevice,
    /// A device exists but offers no stream config the monitor can use.
    UnsupportedFormat(String),
    /// Opening or starting a stream failed.
    Stream(String),
    /// A running stream reported an error (usually the device was unplugged).
    Disconnected,
}

impl MonitorError {
    /// Human-readable one-liner for toasts and tooltips.
    pub fn describe(&self) -> String {
        match self {
            Self::NoRytmAudioInput => "No Rytm audio input found".to_owned(),
            Self::NoOutputDevice => "No output device available".to_owned(),
            Self::UnsupportedFormat(detail) => format!("Unsupported audio format: {detail}"),
            Self::Stream(detail) => format!("Audio stream failed: {detail}"),
            Self::Disconnected => "Audio stream lost (device unplugged?)".to_owned(),
        }
    }
}

/// The monitor's externally visible state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MonitorStatus {
    Idle,
    Starting,
    Running {
        input_name: String,
        output_name: String,
    },
    Failed(MonitorError),
}

enum Cmd {
    Start,
    Stop,
    StreamError,
    Shutdown,
}

/// Handle to the monitor thread; owned by the GUI. Dropping it shuts the
/// thread down and stops any running streams.
pub struct MonitorHandle {
    cmd: Sender<Cmd>,
    status: Arc<Mutex<MonitorStatus>>,
    underruns: Arc<AtomicU32>,
    overruns: Arc<AtomicU32>,
    thread: Option<JoinHandle<()>>,
}

impl MonitorHandle {
    /// Spawns the monitor thread (streams stay closed until [`start`]).
    ///
    /// `on_change`: called after every status change (e.g. request a repaint).
    /// Returns the handle the GUI keeps.
    ///
    /// [`start`]: MonitorHandle::start
    pub fn spawn(on_change: impl Fn() + Send + 'static) -> Self {
        let (cmd, rx) = mpsc::channel();
        let status = Arc::new(Mutex::new(MonitorStatus::Idle));
        let underruns = Arc::new(AtomicU32::new(0));
        let overruns = Arc::new(AtomicU32::new(0));
        let worker = Worker {
            rx,
            cmd: cmd.clone(),
            status: Arc::clone(&status),
            underruns: Arc::clone(&underruns),
            overruns: Arc::clone(&overruns),
            on_change: Box::new(on_change),
        };
        let thread = std::thread::Builder::new()
            .name("audio-monitor".to_owned())
            .spawn(move || worker.run())
            .expect("spawn audio-monitor thread");
        Self {
            cmd,
            status,
            underruns,
            overruns,
            thread: Some(thread),
        }
    }

    /// Requests stream start (async; watch [`status`]).
    ///
    /// [`status`]: MonitorHandle::status
    pub fn start(&self) {
        let _ = self.cmd.send(Cmd::Start);
    }

    /// Requests stream stop.
    pub fn stop(&self) {
        let _ = self.cmd.send(Cmd::Stop);
    }

    /// Current state snapshot.
    pub fn status(&self) -> MonitorStatus {
        self.status.lock().expect("monitor status lock").clone()
    }

    /// Glitch counters since the last start: (underruns, overruns).
    pub fn glitches(&self) -> (u32, u32) {
        (
            self.underruns.load(Ordering::Relaxed),
            self.overruns.load(Ordering::Relaxed),
        )
    }
}

impl Drop for MonitorHandle {
    fn drop(&mut self) {
        let _ = self.cmd.send(Cmd::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct Worker {
    rx: Receiver<Cmd>,
    cmd: Sender<Cmd>,
    status: Arc<Mutex<MonitorStatus>>,
    underruns: Arc<AtomicU32>,
    overruns: Arc<AtomicU32>,
    on_change: Box<dyn Fn() + Send>,
}

impl Worker {
    fn run(self) {
        // The streams must live (and die) on this thread: cpal's Stream is
        // !Send and CoreAudio wants teardown where setup happened.
        let mut streams: Option<(cpal::Stream, cpal::Stream)> = None;
        while let Ok(cmd) = self.rx.recv() {
            match cmd {
                Cmd::Start => {
                    streams = None;
                    self.set_status(MonitorStatus::Starting);
                    self.underruns.store(0, Ordering::Relaxed);
                    self.overruns.store(0, Ordering::Relaxed);
                    match self.build_streams() {
                        Ok((input, output, input_name, output_name)) => {
                            streams = Some((input, output));
                            self.set_status(MonitorStatus::Running {
                                input_name,
                                output_name,
                            });
                        }
                        Err(err) => self.set_status(MonitorStatus::Failed(err)),
                    }
                }
                Cmd::Stop => {
                    streams = None;
                    self.set_status(MonitorStatus::Idle);
                }
                Cmd::StreamError => {
                    // Only meaningful while running; a stale error from a
                    // stream we already dropped must not clobber the state.
                    if streams.take().is_some() {
                        self.set_status(MonitorStatus::Failed(MonitorError::Disconnected));
                    }
                }
                Cmd::Shutdown => break,
            }
        }
    }

    fn set_status(&self, status: MonitorStatus) {
        *self.status.lock().expect("monitor status lock") = status;
        (self.on_change)();
    }

    /// Finds the devices, opens both streams and starts them. Returns the
    /// playing (input, output) pair plus the device names for the UI.
    fn build_streams(&self) -> Result<(cpal::Stream, cpal::Stream, String, String), MonitorError> {
        let host = cpal::default_host();
        let input_device = host
            .input_devices()
            .map_err(|e| MonitorError::Stream(e.to_string()))?
            .find(|d| d.name().is_ok_and(|n| is_rytm_audio_input(&n)))
            .ok_or(MonitorError::NoRytmAudioInput)?;
        let output_device = host
            .default_output_device()
            .ok_or(MonitorError::NoOutputDevice)?;
        let input_name = input_device.name().unwrap_or_else(|_| "input".to_owned());
        let output_name = output_device.name().unwrap_or_else(|_| "output".to_owned());

        let input_ranges: Vec<_> = input_device
            .supported_input_configs()
            .map_err(|e| MonitorError::Stream(e.to_string()))?
            .collect();
        let output_ranges: Vec<_> = output_device
            .supported_output_configs()
            .map_err(|e| MonitorError::Stream(e.to_string()))?
            .collect();
        let input_config = choose_config(&input_ranges).ok_or_else(|| {
            MonitorError::UnsupportedFormat(format!("{input_name}: no 48 kHz input config"))
        })?;
        let output_config = choose_config(&output_ranges).ok_or_else(|| {
            MonitorError::UnsupportedFormat(format!("{output_name}: no 48 kHz output config"))
        })?;

        let (producer, consumer) = ring::ring(MONITOR_RING_FRAMES * MONITOR_CHANNELS);
        let input_stream = build_input_stream(
            &input_device,
            &input_config,
            producer,
            Arc::clone(&self.overruns),
            self.cmd.clone(),
        )?;
        let output_stream = build_output_stream(
            &output_device,
            &output_config,
            consumer,
            Arc::clone(&self.underruns),
            self.cmd.clone(),
        )?;
        input_stream
            .play()
            .map_err(|e| MonitorError::Stream(e.to_string()))?;
        output_stream
            .play()
            .map_err(|e| MonitorError::Stream(e.to_string()))?;
        Ok((input_stream, output_stream, input_name, output_name))
    }
}

/// Reduces cpal's supported ranges to [`ConfigCandidate`]s, picks the best
/// one and pins it to the Rytm's native 48 kHz.
fn choose_config(
    ranges: &[cpal::SupportedStreamConfigRange],
) -> Option<cpal::SupportedStreamConfig> {
    let candidates: Vec<ConfigCandidate> = ranges
        .iter()
        .map(|r| ConfigCandidate {
            channels: r.channels(),
            min_rate: r.min_sample_rate().0,
            max_rate: r.max_sample_rate().0,
            format: match r.sample_format() {
                SampleFormat::F32 => SampleFormatKind::F32,
                SampleFormat::I16 => SampleFormatKind::I16,
                SampleFormat::U16 => SampleFormatKind::U16,
                _ => SampleFormatKind::Other,
            },
        })
        .collect();
    let index = pick_config(&candidates)?;
    Some(ranges[index].with_sample_rate(SampleRate(NATIVE_SAMPLE_RATE)))
}

fn build_input_stream(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    producer: Producer,
    overruns: Arc<AtomicU32>,
    cmd: Sender<Cmd>,
) -> Result<cpal::Stream, MonitorError> {
    match config.sample_format() {
        SampleFormat::F32 => typed_input_stream::<f32>(device, config, producer, overruns, cmd),
        SampleFormat::I16 => typed_input_stream::<i16>(device, config, producer, overruns, cmd),
        SampleFormat::U16 => typed_input_stream::<u16>(device, config, producer, overruns, cmd),
        other => Err(MonitorError::UnsupportedFormat(other.to_string())),
    }
}

fn typed_input_stream<T>(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    producer: Producer,
    overruns: Arc<AtomicU32>,
    cmd: Sender<Cmd>,
) -> Result<cpal::Stream, MonitorError>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let channels = config.channels() as usize;
    let mut scratch = vec![0.0f32; SCRATCH_FRAMES * MONITOR_CHANNELS];
    device
        .build_input_stream(
            &config.config(),
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                let mut dropped = false;
                for block in data.chunks(channels * SCRATCH_FRAMES) {
                    let frames = block.len() / channels;
                    for (f, frame) in block.chunks_exact(channels).enumerate() {
                        let mut pair = [0.0f32; 2];
                        let used = frame.len().min(2);
                        for (dst, src) in pair.iter_mut().zip(frame.iter()) {
                            *dst = (*src).to_sample();
                        }
                        let (l, r) = frame_to_stereo(&pair[..used]);
                        scratch[2 * f] = l;
                        scratch[2 * f + 1] = r;
                    }
                    let want = frames * MONITOR_CHANNELS;
                    if producer.push_slice(&scratch[..want]) < want {
                        dropped = true;
                    }
                }
                if dropped {
                    overruns.fetch_add(1, Ordering::Relaxed);
                }
            },
            move |_err| {
                let _ = cmd.send(Cmd::StreamError);
            },
            None,
        )
        .map_err(|e| MonitorError::Stream(e.to_string()))
}

fn build_output_stream(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    consumer: Consumer,
    underruns: Arc<AtomicU32>,
    cmd: Sender<Cmd>,
) -> Result<cpal::Stream, MonitorError> {
    match config.sample_format() {
        SampleFormat::F32 => typed_output_stream::<f32>(device, config, consumer, underruns, cmd),
        SampleFormat::I16 => typed_output_stream::<i16>(device, config, consumer, underruns, cmd),
        SampleFormat::U16 => typed_output_stream::<u16>(device, config, consumer, underruns, cmd),
        other => Err(MonitorError::UnsupportedFormat(other.to_string())),
    }
}

fn typed_output_stream<T>(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    consumer: Consumer,
    underruns: Arc<AtomicU32>,
    cmd: Sender<Cmd>,
) -> Result<cpal::Stream, MonitorError>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = config.channels() as usize;
    let mut scratch = vec![0.0f32; SCRATCH_FRAMES * MONITOR_CHANNELS];
    let mut frame_buf = vec![0.0f32; channels];
    // Emit silence until the ring holds the prefill, so playback starts with
    // headroom instead of an immediate underrun.
    let mut primed = false;
    device
        .build_output_stream(
            &config.config(),
            move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                if !primed {
                    if consumer.available() < MONITOR_PREFILL_FRAMES * MONITOR_CHANNELS {
                        data.fill(T::EQUILIBRIUM);
                        return;
                    }
                    primed = true;
                }
                let mut starved = false;
                for block in data.chunks_mut(channels * SCRATCH_FRAMES) {
                    let frames = block.len() / channels;
                    let want = frames * MONITOR_CHANNELS;
                    let got = consumer.pop_slice(&mut scratch[..want]);
                    scratch[got..want].fill(0.0);
                    if got < want {
                        starved = true;
                    }
                    for (f, frame) in block.chunks_exact_mut(channels).enumerate() {
                        stereo_to_frame(scratch[2 * f], scratch[2 * f + 1], &mut frame_buf);
                        for (dst, src) in frame.iter_mut().zip(frame_buf.iter()) {
                            *dst = T::from_sample(*src);
                        }
                    }
                }
                if starved {
                    underruns.fetch_add(1, Ordering::Relaxed);
                }
            },
            move |_err| {
                let _ = cmd.send(Cmd::StreamError);
            },
            None,
        )
        .map_err(|e| MonitorError::Stream(e.to_string()))
}
