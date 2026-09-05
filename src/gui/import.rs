//! Import queue model: WAVs dropped on the window are converted here and
//! uploaded to the device over SDS. Rendering lives in
//! [`crate::gui::views::import_page`]; this module owns the queue state and
//! the upload workers.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
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
    Converted,
    Uploading(UploadProgress),
    Sent { open_loop: bool },
    Failed(String),
}

/// The import queue + upload workers.
#[derive(Default)]
pub struct ImportPanel {
    items: Vec<Arc<Mutex<ImportItem>>>,
    cancel: Arc<AtomicBool>,
    /// Next SDS sample number to use this session (device-side slot of the
    /// dump protocol, not a project sample slot).
    next_sample_number: u16,
}

impl ImportPanel {
    /// Converts newly dropped files and queues them.
    pub fn add_dropped_files(&mut self, paths: Vec<PathBuf>) {
        for path in paths {
            let item = match convert_wav_file(&path) {
                Ok(sample) => ImportItem {
                    sample,
                    state: ImportState::Converted,
                    selected: true,
                },
                Err(e) => ImportItem {
                    sample: ConvertedSample {
                        name: path
                            .file_name()
                            .map_or_else(|| "?".into(), |n| n.to_string_lossy().into_owned()),
                        pcm: Vec::new(),
                        source_rate: 0,
                        source_channels: 0,
                    },
                    state: ImportState::Failed(e.to_string()),
                    selected: false,
                },
            };
            self.items.push(Arc::new(Mutex::new(item)));
        }
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

    /// Starts uploads for every selected, converted item (one worker each).
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
        for item in queued {
            self.start_upload(item, in_port.to_owned(), ctx);
        }
    }

    /// Starts one upload worker for `item`.
    pub fn start_upload(
        &mut self,
        item: Arc<Mutex<ImportItem>>,
        in_port: String,
        ctx: &egui::Context,
    ) {
        let sample_number = self.next_sample_number;
        self.next_sample_number = self.next_sample_number.wrapping_add(1);
        let cancel = Arc::clone(&self.cancel);
        let repaint = ctx.clone();
        thread::Builder::new()
            .name("sds-upload".into())
            .spawn(move || {
                let sample = {
                    let mut guard = item.lock().expect("import item lock poisoned");
                    guard.state = ImportState::Uploading(UploadProgress::default());
                    guard.sample.clone()
                };
                repaint.request_repaint();

                let outcome =
                    run_upload(&in_port, sample_number, &sample, &cancel, &item, &repaint);

                let mut guard = item.lock().expect("import item lock poisoned");
                guard.state = match outcome {
                    UploadOutcome::Sent { open_loop } => ImportState::Sent { open_loop },
                    UploadOutcome::Cancelled => ImportState::Converted,
                    UploadOutcome::Rejected(e) | UploadOutcome::LinkFailed(e) => {
                        ImportState::Failed(e)
                    }
                };
                repaint.request_repaint();
            })
            .expect("failed to spawn upload thread");
    }
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
