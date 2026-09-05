//! Sample import panel: WAVs dropped on the window are converted here and
//! uploaded to the device over SDS.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::thread;

use eframe::egui::{self, Color32, RichText};

use crate::audio::{convert_wav_file, ConvertedSample};
use crate::device::detect::is_rytm_port;
use crate::device::samples::{upload_sample, UploadOutcome, UploadProgress};
use crate::midi::link::MidiLink;
use crate::midi::ports;

/// One imported file and where it is in its life cycle.
pub struct ImportItem {
    pub sample: ConvertedSample,
    pub state: ImportState,
}

#[derive(Clone)]
pub enum ImportState {
    Converted,
    Uploading(UploadProgress),
    Sent { open_loop: bool },
    Failed(String),
}

/// The import queue + at most one running upload worker.
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
            let state = match convert_wav_file(&path) {
                Ok(sample) => ImportItem {
                    sample,
                    state: ImportState::Converted,
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
                },
            };
            self.items.push(Arc::new(Mutex::new(state)));
        }
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Renders the queue; `device_in_port` enables the upload buttons.
    pub fn show(&mut self, ui: &mut egui::Ui, device_in_port: Option<&str>, ctx: &egui::Context) {
        ui.label(RichText::new("Imported samples").strong());
        ui.label(
            RichText::new(
                "Files convert to 16-bit / 48 kHz mono. Uploads land in the +Drive \
                 directory marked UPLOAD HERE (INCOMING/ by default).",
            )
            .weak(),
        );
        ui.add_space(4.0);

        let mut to_remove = None;
        for (i, item_arc) in self.items.iter().enumerate() {
            let item = item_arc.lock().expect("import item lock poisoned");
            ui.horizontal(|ui| {
                ui.label(&item.sample.name);
                match &item.state {
                    ImportState::Converted => {
                        ui.label(
                            RichText::new(format!(
                                "{:.2}s · {} KB · from {} Hz/{}ch",
                                item.sample.seconds(),
                                item.sample.byte_size() / 1024,
                                item.sample.source_rate,
                                item.sample.source_channels,
                            ))
                            .weak(),
                        );
                        if let Some(port) = device_in_port {
                            if ui.small_button("⇪ upload").clicked() {
                                self.start_upload(Arc::clone(item_arc), port.to_owned(), ctx);
                            }
                        }
                    }
                    ImportState::Uploading(p) => {
                        ui.add(
                            egui::ProgressBar::new(
                                p.packets_sent as f32 / p.packets_total.max(1) as f32,
                            )
                            .desired_width(120.0)
                            .text(format!("{}/{}", p.packets_sent, p.packets_total)),
                        );
                    }
                    ImportState::Sent { open_loop } => {
                        ui.label(
                            RichText::new(if *open_loop {
                                "sent (open loop)"
                            } else {
                                "sent ✓"
                            })
                            .color(Color32::from_rgb(0x2E, 0xCC, 0x71)),
                        );
                    }
                    ImportState::Failed(err) => {
                        ui.label(RichText::new(err.as_str()).color(Color32::LIGHT_RED));
                    }
                }
                if ui.small_button("✕").clicked() {
                    to_remove = Some(i);
                }
            });
        }
        if let Some(i) = to_remove {
            self.items.remove(i);
        }
    }

    fn start_upload(&self, item: Arc<Mutex<ImportItem>>, in_port: String, ctx: &egui::Context) {
        let sample_number = self.next_sample_number;
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
