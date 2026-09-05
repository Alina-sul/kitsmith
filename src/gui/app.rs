//! The application: connection status, sync controls, and the dashboard.

use eframe::egui::{self, Color32, RichText};

use crate::cache;
use crate::device::detect::DeviceState;
use crate::device::watcher::DeviceWatcher;
use crate::gui::dashboard;
use crate::gui::import::ImportPanel;
use crate::gui::sync::{SyncController, SyncStatus};
use crate::model::ProjectModel;

pub struct RytmdashApp {
    watcher: DeviceWatcher,
    sync: SyncController,
    import: ImportPanel,
    model: Option<ProjectModel>,
    last_sync_note: Option<String>,
}

impl RytmdashApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let ctx = cc.egui_ctx.clone();
        Self {
            watcher: DeviceWatcher::spawn(move || ctx.request_repaint()),
            sync: SyncController::default(),
            import: ImportPanel::default(),
            model: cache::load(),
            last_sync_note: None,
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let snapshot = self.watcher.snapshot();
        ui.horizontal(|ui| {
            ui.heading("rytmdash");
            ui.separator();
            match &snapshot.device {
                DeviceState::Connected(info) => {
                    ui.label(RichText::new("●").color(Color32::from_rgb(0x2E, 0xCC, 0x71)));
                    ui.label(&info.port_name).on_hover_text(info.describe());
                    self.sync_controls(ui, ctx, &info.port_name);
                }
                DeviceState::Searching => {
                    ui.spinner();
                    ui.label("Searching for an Analog Rytm…").on_hover_text(
                        "Connect the Rytm over USB. On the device: GLOBAL SETTINGS → SYSTEM → \
                         USB CONFIG → USB-MIDI, and MIDI CONFIG → PORT CONFIG input/output → USB.",
                    );
                }
            }
        });
    }

    fn sync_controls(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, in_port: &str) {
        match self.sync.status() {
            SyncStatus::Running(progress) => {
                ui.add(
                    egui::ProgressBar::new(progress.done as f32 / progress.total.max(1) as f32)
                        .desired_width(160.0)
                        .text(format!(
                            "{} {}/{}",
                            progress.stage, progress.done, progress.total
                        )),
                );
                if ui.small_button("cancel").clicked() {
                    self.sync.cancel();
                }
            }
            status => {
                if ui.button("⟳ Sync from device").clicked() {
                    let repaint_ctx = ctx.clone();
                    self.last_sync_note = None;
                    self.sync
                        .start(in_port.to_owned(), move || repaint_ctx.request_repaint());
                }
                if let SyncStatus::Failed(err) = status {
                    ui.label(
                        RichText::new(format!("sync failed: {err}")).color(Color32::LIGHT_RED),
                    );
                }
            }
        }
    }
}

impl eframe::App for RytmdashApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        if let Some((model, missed)) = self.sync.take_finished() {
            self.last_sync_note = Some(if missed == 0 {
                "sync complete".to_owned()
            } else {
                format!("sync complete — {missed} object(s) did not answer")
            });
            self.model = Some(model);
        }

        egui::Panel::top("status-bar").show(ui, |ui| {
            self.top_bar(ui, &ctx);
        });

        // Files dropped anywhere on the window enter the import queue.
        let dropped: Vec<std::path::PathBuf> = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        if !dropped.is_empty() {
            self.import.add_dropped_files(dropped);
        }

        egui::CentralPanel::default().show(ui, |ui| {
            if let Some(note) = &self.last_sync_note {
                ui.label(RichText::new(note).weak());
                ui.add_space(4.0);
            }
            if !self.import.is_empty() {
                let in_port = match &self.watcher.snapshot().device {
                    DeviceState::Connected(info) => Some(info.port_name.clone()),
                    DeviceState::Searching => None,
                };
                self.import.show(ui, in_port.as_deref(), &ctx);
                ui.add_space(8.0);
                ui.separator();
            }
            match &self.model {
                Some(model) => dashboard::show(ui, model),
                None => {
                    ui.add_space(24.0);
                    ui.vertical_centered(|ui| {
                        ui.label(
                            RichText::new(
                                "No project data yet — connect the Rytm and press \
                                 “Sync from device” to pull the active project.",
                            )
                            .weak(),
                        );
                    });
                }
            }
            ui.add_space(12.0);
            ui.collapsing("MIDI ports", |ui| {
                let snapshot = self.watcher.snapshot();
                port_list(ui, "Inputs", &snapshot.ports.inputs);
                port_list(ui, "Outputs", &snapshot.ports.outputs);
            });
        });
    }
}

fn port_list(ui: &mut egui::Ui, label: &str, names: &[String]) {
    ui.label(RichText::new(label).strong());
    if names.is_empty() {
        ui.label(RichText::new("(none)").weak());
    }
    for name in names {
        ui.label(format!("  {name}"));
    }
}
