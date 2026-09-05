//! The application: renders the watcher's state.

use eframe::egui::{self, Color32, RichText};

use crate::device::detect::DeviceState;
use crate::device::watcher::DeviceWatcher;

pub struct RytmdashApp {
    watcher: DeviceWatcher,
}

impl RytmdashApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let ctx = cc.egui_ctx.clone();
        Self {
            watcher: DeviceWatcher::spawn(move || ctx.request_repaint()),
        }
    }
}

impl eframe::App for RytmdashApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let snapshot = self.watcher.snapshot();

        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("rytmdash");
            ui.add_space(8.0);

            match &snapshot.device {
                DeviceState::Connected(info) => {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("●").color(Color32::from_rgb(0x2E, 0xCC, 0x71)));
                        ui.label(RichText::new(&info.port_name).strong());
                    });
                    ui.label(info.describe());
                }
                DeviceState::Searching => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Searching for an Analog Rytm…");
                    });
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(
                            "Connect the Rytm over USB. On the device, set \
                             GLOBAL SETTINGS → SYSTEM → USB CONFIG to USB-MIDI, and \
                             MIDI CONFIG → PORT CONFIG input/output to USB.",
                        )
                        .weak(),
                    );
                }
            }

            ui.add_space(12.0);
            ui.collapsing("MIDI ports", |ui| {
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
