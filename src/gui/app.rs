//! The application shell: status bar, dashboard, import tray, execute modal.

use eframe::egui::{self, Color32, RichText};

use crate::cache;
use crate::device::detect::DeviceState;
use crate::device::watcher::DeviceWatcher;
use crate::gui::dashboard::{self, DashboardState};
use crate::gui::execute::ExecutePanel;
use crate::gui::import::ImportPanel;
use crate::gui::sync::{SyncController, SyncStatus};
use crate::gui::theme::OK_COLOR;
use crate::model::ProjectModel;

pub struct RytmdashApp {
    watcher: DeviceWatcher,
    sync: SyncController,
    import: ImportPanel,
    execute: ExecutePanel,
    dashboard: DashboardState,
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
            execute: ExecutePanel::default(),
            dashboard: DashboardState::default(),
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
                    ui.label(RichText::new("●").color(OK_COLOR));
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
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.menu_button("⋯", |ui| {
                    port_list(ui, "MIDI inputs", &snapshot.ports.inputs);
                    port_list(ui, "MIDI outputs", &snapshot.ports.outputs);
                });
                if let Some(model) = &self.model {
                    if !model.projects.is_empty() {
                        ui.menu_button(format!("projects ({})", model.projects.len()), |ui| {
                            ui.label(
                                RichText::new(
                                    "The dashboard mirrors the *active* project. To work on \
                                     another one: load it on the device (GLOBAL SETTINGS → \
                                     PROJECT), then press ⟳ Sync.",
                                )
                                .weak()
                                .small(),
                            );
                            ui.separator();
                            egui::ScrollArea::vertical()
                                .max_height(300.0)
                                .show(ui, |ui| {
                                    for project in &model.projects {
                                        ui.label(format!("{:>3} · {}", project.id, project.name));
                                    }
                                });
                        });
                    }
                }
                if let Some(note) = &self.last_sync_note {
                    ui.label(RichText::new(note).weak().small());
                }
            });
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
                if ui
                    .button("⟳ Sync")
                    .on_hover_text("pull the active project")
                    .clicked()
                {
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

    fn empty_state(&self, ui: &mut egui::Ui, connected: bool) {
        ui.add_space(48.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("No project data yet").heading().weak());
            ui.add_space(8.0);
            if connected {
                ui.label("Press ⟳ Sync in the top bar to pull the active project.");
            } else {
                ui.label("1 · Connect the Analog Rytm over USB");
                ui.label("2 · On the device: USB CONFIG → USB-MIDI, PORT CONFIG → USB");
                ui.label("3 · Press ⟳ Sync in the top bar");
            }
            ui.add_space(8.0);
            ui.label(
                RichText::new(
                    "You can also drop .wav files anywhere in this window \
                     to convert them for upload.",
                )
                .weak(),
            );
        });
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

        let in_port = match &self.watcher.snapshot().device {
            DeviceState::Connected(info) => Some(info.port_name.clone()),
            DeviceState::Searching => None,
        };

        egui::Panel::top("status-bar").show(ui, |ui| {
            self.top_bar(ui, &ctx);
        });

        if !self.import.is_empty() {
            egui::Panel::bottom("import-tray")
                .resizable(false)
                .show(ui, |ui| {
                    self.import.show(ui, in_port.as_deref(), &ctx);
                });
        }

        egui::CentralPanel::default().show(ui, |ui| match &self.model {
            Some(model) => {
                if let Some(assign) = dashboard::show(ui, model, &mut self.dashboard) {
                    self.execute.pending = Some(assign);
                }
            }
            None => self.empty_state(ui, in_port.is_some()),
        });

        // Plan preview / execution modal, if a drag landed.
        self.execute.show(&ctx, in_port.as_deref());
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
