//! Activity center (PRD §45): one consistent place for sync, upload and plan
//! execution status plus the session history.

use std::time::Instant;

use eframe::egui::{self, RichText, ScrollArea};

use crate::gui::app::{ago, page_title};
use crate::gui::import::{ImportPanel, ImportState};
use crate::gui::sync::{SyncController, SyncStatus};
use crate::gui::{theme, widgets};

/// Severity/flavor of an activity entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityKind {
    Info,
    Success,
    Warning,
    Error,
}

/// One completed event in the session log.
pub struct ActivityEntry {
    pub kind: ActivityKind,
    pub title: String,
    pub detail: Option<String>,
    pub at: Instant,
}

pub fn show(
    ui: &mut egui::Ui,
    entries: &[ActivityEntry],
    sync: &SyncController,
    import: &ImportPanel,
) {
    page_title(ui, "Activity");
    ui.add_space(theme::GAP_M);

    widgets::section_label(ui, "RUNNING");
    ui.add_space(4.0);
    let mut anything_running = false;
    if let SyncStatus::Running(progress) = sync.status() {
        anything_running = true;
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label("Device sync");
            ui.label(
                RichText::new(format!(
                    "{} {}/{}",
                    progress.stage, progress.done, progress.total
                ))
                .color(theme::TEXT_SECONDARY),
            );
        });
    }
    for item in import.items() {
        let guard = item.lock().expect("import item lock poisoned");
        match &guard.state {
            ImportState::Converting => {
                anything_running = true;
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(format!("Converting {}", guard.sample.name));
                });
            }
            ImportState::Uploading(p) => {
                anything_running = true;
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(format!("Uploading {}", guard.sample.name));
                    ui.label(
                        RichText::new(format!("{}/{} packets", p.packets_sent, p.packets_total))
                            .color(theme::TEXT_SECONDARY),
                    );
                });
            }
            _ => {}
        }
    }
    if !anything_running {
        ui.label(RichText::new("Nothing running.").color(theme::TEXT_MUTED));
    }

    ui.add_space(theme::GAP_L);
    widgets::section_label(ui, "THIS SESSION");
    ui.add_space(4.0);
    if entries.is_empty() {
        ui.label(RichText::new("No activity yet.").color(theme::TEXT_MUTED));
        return;
    }
    ScrollArea::vertical().show(ui, |ui| {
        for (index, entry) in entries.iter().rev().enumerate() {
            ui.horizontal(|ui| {
                let color = match entry.kind {
                    ActivityKind::Info => theme::TEXT_SECONDARY,
                    ActivityKind::Success => theme::SUCCESS,
                    ActivityKind::Warning => theme::WARNING,
                    ActivityKind::Error => theme::DANGER,
                };
                widgets::status_dot(ui, color, &entry.title);
                ui.label(
                    RichText::new(ago(entry.at))
                        .color(theme::TEXT_MUTED)
                        .size(theme::FONT_META),
                );
            });
            if let Some(detail) = &entry.detail {
                ui.indent(("activity-detail", index), |ui| {
                    ui.label(
                        RichText::new(detail)
                            .color(theme::TEXT_MUTED)
                            .size(theme::FONT_META),
                    );
                });
            }
        }
    });
}
