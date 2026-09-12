//! Import queue model: WAVs dropped on the window are converted here and
//! uploaded to the device over SDS. Rendering lives in
//! [`crate::gui::views::import_page`]; this module owns the queue state and
//! the background workers.
//!
//! Both stages run off the UI thread, and both are *serial*: conversion is
//! CPU-bound, and SDS carries exactly one dump at a time — parallel senders
//! would interleave packets and handshakes on the same MIDI port.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use eframe::egui;

use crate::audio::{convert_wav_file, ConvertedSample};
use crate::device::detect::is_rytm_port;
use crate::device::samples::{upload_sample, UploadOutcome, UploadProgress};
use crate::midi::link::MidiLink;
use crate::midi::ports;

/// One imported file and where it is in its life cycle.
pub struct ImportItem {
    pub sample: ConvertedSample,
    pub state: ImportState,
    /// Included in "Upload selected" batch actions.
    pub selected: bool,
}

#[derive(Clone)]
pub enum ImportState {
    Converting,
    Converted,
    Uploading(UploadProgress),
    Sent { open_loop: bool },
    Failed(String),
}

/// The import queue + its background workers.
#[derive(Default)]
pub struct ImportPanel {
    items: Vec<Arc<Mutex<ImportItem>>>,
    cancel: Arc<AtomicBool>,
    /// Set while the upload worker owns the MIDI link, so a second batch
    /// cannot start a competing dump on the same port.
    uploading: Arc<AtomicBool>,
    /// Next SDS sample number to use this session (device-side slot of the
    /// dump protocol, not a project sample slot).
    next_sample_number: u16,
}

impl ImportPanel {
    /// Queues newly dropped files and converts them on a worker thread.
    ///
    /// Conversion (decode + windowed-sinc resample) is seconds of work for a
    /// long file, so the queue rows appear immediately as `Converting` rather
    /// than freezing the window until every drop is decoded.
    pub fn add_dropped_files(&mut self, paths: Vec<PathBuf>, ctx: &egui::Context) {
        let pending: Vec<(PathBuf, Arc<Mutex<ImportItem>>)> = paths
            .into_iter()
            .map(|path| {
                let item = Arc::new(Mutex::new(ImportItem {
                    sample: placeholder_sample(&path),
                    state: ImportState::Converting,
                    selected: false,
                }));
                self.items.push(Arc::clone(&item));
                (path, item)
            })
            .collect();
        if pending.is_empty() {
            return;
        }
        let repaint = ctx.clone();
        thread::Builder::new()
            .name("wav-convert".into())
            .spawn(move || {
                for (path, item) in &pending {
                    let converted = convert_wav_file(path);
                    let mut guard = item.lock().expect("import item lock poisoned");
                    match converted {
                        Ok(sample) => {
                            guard.sample = sample;
                            guard.state = ImportState::Converted;
                            guard.selected = true;
                        }
                        Err(e) => guard.state = ImportState::Failed(e.to_string()),
                    }
                    drop(guard);
                    repaint.request_repaint();
                }
            })
            .expect("failed to spawn conversion thread");
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn items(&self) -> &[Arc<Mutex<ImportItem>>] {
        &self.items
    }

    /// True while the upload worker holds the device link.
    pub fn is_uploading(&self) -> bool {
        self.uploading.load(Ordering::Relaxed)
    }

    /// Asks the running batch to stop. The current transfer aborts at its next
    /// packet boundary and the rest of the batch is skipped; every item that
    /// did not finish returns to `Converted`, ready to be sent again.
    pub fn cancel_uploads(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub fn remove(&mut self, index: usize) {
        if index < self.items.len() {
            self.items.remove(index);
        }
    }

    pub fn set_all_selected(&mut self, selected: bool) {
        for item in &self.items {
            item.lock().expect("import item lock poisoned").selected = selected;
        }
    }

    /// Uploads every selected, converted item — one after another, over a
    /// single link. Does nothing while a batch is already running.
    pub fn upload_selected(&mut self, in_port: &str, ctx: &egui::Context) {
        let queued: Vec<Arc<Mutex<ImportItem>>> = self
            .items
            .iter()
            .filter(|item| {
                let guard = item.lock().expect("import item lock poisoned");
                guard.selected && matches!(guard.state, ImportState::Converted)
            })
            .cloned()
            .collect();
        self.start_uploads(queued, in_port.to_owned(), ctx);
    }

    /// Starts the (single) upload worker for `items`.
    fn start_uploads(
        &mut self,
        items: Vec<Arc<Mutex<ImportItem>>>,
        in_port: String,
        ctx: &egui::Context,
    ) {
        if items.is_empty() || self.uploading.swap(true, Ordering::Relaxed) {
            return;
        }
        let batch: Vec<(u16, Arc<Mutex<ImportItem>>)> = items
            .into_iter()
            .map(|item| {
                let sample_number = self.next_sample_number;
                self.next_sample_number = self.next_sample_number.wrapping_add(1);
                (sample_number, item)
            })
            .collect();
        let cancel = Arc::clone(&self.cancel);
        cancel.store(false, Ordering::Relaxed);
        let uploading = Arc::clone(&self.uploading);
        let repaint = ctx.clone();
        thread::Builder::new()
            .name("sds-upload".into())
            .spawn(move || {
                for (sample_number, item) in &batch {
                    if cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    upload_one(&in_port, *sample_number, item, &cancel, &repaint);
                }
                uploading.store(false, Ordering::Relaxed);
                repaint.request_repaint();
            })
            .expect("failed to spawn upload thread");
    }
}

/// A queue row for a file that has not been decoded yet.
fn placeholder_sample(path: &Path) -> ConvertedSample {
    ConvertedSample {
        name: path
            .file_name()
            .map_or_else(|| "?".into(), |n| n.to_string_lossy().into_owned()),
        pcm: Vec::new(),
        source_rate: 0,
        source_channels: 0,
    }
}

/// Runs one item's transfer start-to-finish, publishing its state.
fn upload_one(
    in_port: &str,
    sample_number: u16,
    item: &Arc<Mutex<ImportItem>>,
    cancel: &Arc<AtomicBool>,
    repaint: &egui::Context,
) {
    let sample = {
        let mut guard = item.lock().expect("import item lock poisoned");
        guard.state = ImportState::Uploading(UploadProgress::default());
        guard.sample.clone()
    };
    repaint.request_repaint();

    let outcome = run_upload(in_port, sample_number, &sample, cancel, item, repaint);

    let mut guard = item.lock().expect("import item lock poisoned");
    guard.state = match outcome {
        UploadOutcome::Sent { open_loop } => ImportState::Sent { open_loop },
        UploadOutcome::Cancelled => ImportState::Converted,
        UploadOutcome::Rejected(e) | UploadOutcome::LinkFailed(e) => ImportState::Failed(e),
    };
    drop(guard);
    repaint.request_repaint();
}

fn run_upload(
    in_port: &str,
    sample_number: u16,
    sample: &ConvertedSample,
    cancel: &Arc<AtomicBool>,
    item: &Arc<Mutex<ImportItem>>,
    repaint: &egui::Context,
) -> UploadOutcome {
    let Some(out_port) = ports::snapshot()
        .outputs
        .into_iter()
        .find(|n| is_rytm_port(n))
    else {
        return UploadOutcome::LinkFailed("no Rytm MIDI output port found".into());
    };
    let mut link = match MidiLink::open(in_port, &out_port) {
        Ok(link) => link,
        Err(e) => return UploadOutcome::LinkFailed(e.to_string()),
    };
    upload_sample(&mut link, 0, sample_number, sample, cancel, |progress| {
        item.lock().expect("import item lock poisoned").state = ImportState::Uploading(progress);
        repaint.request_repaint();
    })
}
