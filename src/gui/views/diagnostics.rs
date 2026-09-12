//! Device & Diagnostics: the technical view (PRD §33 data-quality panel).
//! Raw MIDI ports and protocol detail live here, out of the main UI.

use eframe::egui::{self, RichText, ScrollArea};

use crate::device::detect::DeviceState;
use crate::device::watcher::DeviceWatcher;
use crate::gui::app::{ago, page_title, SyncInfo};
use crate::gui::{theme, widgets};
use crate::model::ProjectModel;

pub fn show(
    ui: &mut egui::Ui,
    watcher: &DeviceWatcher,
    model: Option<&ProjectModel>,
    last_sync: Option<SyncInfo>,
) {
    page_title(ui, "Device & Diagnostics");
    ui.add_space(theme::GAP_M);
    let snapshot = watcher.snapshot();

    ScrollArea::vertical().show(ui, |ui| {
        widgets::inspector_group(ui, "DEVICE", |ui| match &snapshot.device {
            DeviceState::Connected(info) => {
                ui.horizontal(|ui| {
                    widgets::status_dot(ui, theme::SUCCESS, "Connected");
                });
                widgets::detail_line(ui, "Port", &info.port_name);
                ui.label(
                    RichText::new(info.describe())
                        .color(theme::TEXT_SECONDARY)
                        .size(theme::FONT_META),
                );
            }
            DeviceState::Searching => {
                ui.horizontal(|ui| {
                    widgets::status_dot(ui, theme::TEXT_MUTED, "No device");
                });
                ui.label(
                    RichText::new(
                        "Connect the Rytm over USB. On the device: GLOBAL SETTINGS → SYSTEM → \
                         USB CONFIG → USB-MIDI, and MIDI CONFIG → PORT CONFIG input/output → USB.",
                    )
                    .color(theme::TEXT_SECONDARY)
                    .size(theme::FONT_META),
                );
            }
        });

        widgets::inspector_group(ui, "DATA QUALITY", |ui| {
            match model {
                Some(model) => {
                    let used = model.slot_usage().len();
                    let resolved = model
                        .slot_usage()
                        .keys()
                        .filter(|slot| model.sample_name(**slot).is_some())
                        .count();
                    widgets::detail_line(
                        ui,
                        "Sample names",
                        &format!("{resolved}/{used} used slots resolved"),
                    );
                    widgets::detail_line(
                        ui,
                        "Project slots listed",
                        &model.projects.len().to_string(),
                    );
                    widgets::detail_line(ui, "Kits synced", &model.kits.len().to_string());
                    widgets::detail_line(ui, "Patterns synced", &model.patterns.len().to_string());
                }
                None => {
                    ui.label(RichText::new("No synced data yet.").color(theme::TEXT_MUTED));
                }
            }
            match last_sync {
                Some(info) if info.missed > 0 => {
                    widgets::detail_line(
                        ui,
                        "Last sync",
                        &format!(
                            "{} · {} object(s) did not answer",
                            ago(info.at),
                            info.missed
                        ),
                    );
                }
                Some(info) => widgets::detail_line(ui, "Last sync", &ago(info.at)),
                None => widgets::detail_line(ui, "Last sync", "not run this session"),
            }
        });

        widgets::inspector_group(ui, "MIDI INPUTS", |ui| {
            port_list(ui, &snapshot.ports.inputs);
        });
        widgets::inspector_group(ui, "MIDI OUTPUTS", |ui| {
            port_list(ui, &snapshot.ports.outputs);
        });
    });
}

fn port_list(ui: &mut egui::Ui, names: &[String]) {
    if names.is_empty() {
        ui.label(RichText::new("(none)").color(theme::TEXT_MUTED));
    }
    for name in names {
        ui.label(name);
    }
}
