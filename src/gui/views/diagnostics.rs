//! Device & Diagnostics: the technical view (PRD §33 data-quality panel).
//! Raw MIDI ports and protocol detail live here, out of the main UI.

use eframe::egui::{self, RichText, ScrollArea};

use crate::audio::monitor::{MonitorError, MonitorHandle, MonitorStatus};
use crate::device::detect::DeviceState;
use crate::device::objects::MissedObject;
use crate::device::watcher::DeviceWatcher;
use crate::gui::app::{ago, page_title, SyncInfo};
use crate::gui::{theme, widgets};
use crate::model::ProjectModel;

pub fn show(
    ui: &mut egui::Ui,
    watcher: &DeviceWatcher,
    monitor: &MonitorHandle,
    model: Option<&ProjectModel>,
    last_sync: Option<SyncInfo>,
    sync_misses: &[MissedObject],
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
                    RichText::new(crate::config::DEVICE_CONNECT_HINT)
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

        if !sync_misses.is_empty() {
            widgets::inspector_group(ui, "OBJECTS THE LAST SYNC COULD NOT READ", |ui| {
                let rejected = sync_misses.iter().filter(|m| m.got_reply).count();
                let silent = sync_misses.len() - rejected;
                ui.label(
                    RichText::new(format!(
                        "{silent} timed out (device busy / port throttled) · {rejected} answered \
                         but could not be parsed (possible OS 1.72 structure difference)"
                    ))
                    .color(theme::TEXT_SECONDARY)
                    .size(theme::FONT_META),
                );
                ui.add_space(4.0);
                for miss in sync_misses {
                    ui.label(miss.describe());
                }
                ui.add_space(4.0);
                ui.label(
                    RichText::new(
                        "Timeouts: stop the sequencer and set PORT CONFIG → USB only, then \
                         re-sync. If they persist, USB CONFIG → USB MIDI is the leanest mode \
                         (note: it disables audio monitoring). Parse failures on the same \
                         objects every time are a protocol finding — worth reporting.",
                    )
                    .color(theme::TEXT_MUTED)
                    .size(theme::FONT_META),
                );
            });
        }

        widgets::inspector_group(ui, "AUDIO MONITOR", |ui| {
            audio_monitor_section(ui, monitor, &snapshot.device);
        });

        widgets::inspector_group(ui, "MIDI INPUTS", |ui| {
            port_list(ui, &snapshot.ports.inputs);
        });
        widgets::inspector_group(ui, "MIDI OUTPUTS", |ui| {
            port_list(ui, &snapshot.ports.outputs);
        });

        widgets::inspector_group(ui, "ABOUT", |ui| {
            widgets::detail_line(
                ui,
                crate::config::APP_NAME,
                concat!("v", env!("CARGO_PKG_VERSION")),
            );
            ui.add_space(4.0);
            ui.label(
                RichText::new(crate::config::LEGAL_DISCLAIMER)
                    .color(theme::TEXT_MUTED)
                    .size(theme::FONT_META),
            );
        });
    });
}

/// Live-monitor state, glitch counters and the start/stop control. The
/// USB CONFIG hint fires only when MIDI sees the device but audio doesn't —
/// that combination means the device is in the wrong USB mode.
fn audio_monitor_section(ui: &mut egui::Ui, monitor: &MonitorHandle, device: &DeviceState) {
    match monitor.status() {
        MonitorStatus::Idle => {
            ui.label(
                RichText::new("Hear the Rytm through this computer while you play.")
                    .color(theme::TEXT_SECONDARY)
                    .size(theme::FONT_META),
            );
            if ui.button("Start monitoring").clicked() {
                monitor.start();
            }
        }
        MonitorStatus::Starting => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Opening audio streams…");
            });
            if ui.button("Cancel").clicked() {
                monitor.stop();
            }
        }
        MonitorStatus::Running {
            input_name,
            output_name,
        } => {
            ui.horizontal(|ui| {
                widgets::status_dot(ui, theme::SUCCESS, "Monitoring");
            });
            widgets::detail_line(ui, "Input", &input_name);
            widgets::detail_line(ui, "Output", &output_name);
            let (underruns, overruns) = monitor.glitches();
            widgets::detail_line(
                ui,
                "Glitches",
                &format!("{underruns} underruns · {overruns} overruns"),
            );
            if ui.button("Stop monitoring").clicked() {
                monitor.stop();
            }
        }
        MonitorStatus::Failed(err) => {
            ui.horizontal(|ui| {
                widgets::status_dot(ui, theme::WARNING, "Not running");
            });
            widgets::detail_line(ui, "Reason", &err.describe());
            let hint = match (&err, device) {
                (MonitorError::NoRytmAudioInput, DeviceState::Connected(_)) => {
                    Some(crate::config::MONITOR_USB_CONFIG_HINT)
                }
                (MonitorError::NoRytmAudioInput, DeviceState::Searching) => {
                    Some("Device not found — connect the Rytm over USB first.")
                }
                _ => None,
            };
            if let Some(hint) = hint {
                ui.label(
                    RichText::new(hint)
                        .color(theme::TEXT_SECONDARY)
                        .size(theme::FONT_META),
                );
            }
            if ui.button("Try again").clicked() {
                monitor.start();
            }
        }
    }
}

fn port_list(ui: &mut egui::Ui, names: &[String]) {
    if names.is_empty() {
        ui.label(RichText::new("(none)").color(theme::TEXT_MUTED));
    }
    for name in names {
        ui.label(name);
    }
}
