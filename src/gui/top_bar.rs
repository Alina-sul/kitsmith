//! The 52 px top app bar (PRD §6): wordmark, device pill, breadcrumb,
//! sync control with human-readable states, Import shortcut.

use eframe::egui::{self, CornerRadius, Frame, Margin, RichText, Sense, Stroke};

use crate::device::detect::DeviceState;
use crate::gui::app::{ago, Nav, ProjectTab, RytmdashApp};
use crate::gui::sync::SyncStatus;
use crate::gui::{theme, widgets};
use crate::model::names;

pub fn show(app: &mut RytmdashApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    ui.horizontal_centered(|ui| {
        let toggle = if app.ui_state.sidebar_hidden {
            "»"
        } else {
            "«"
        };
        if ui
            .button(toggle)
            .on_hover_text(if app.ui_state.sidebar_hidden {
                "Show sidebar"
            } else {
                "Hide sidebar"
            })
            .clicked()
        {
            app.ui_state.sidebar_hidden = !app.ui_state.sidebar_hidden;
        }
        ui.label(
            RichText::new("rytmdash")
                .size(theme::FONT_SECTION + 2.0)
                .strong(),
        );
        ui.add_space(theme::GAP_M);
        device_pill(app, ui);
        ui.add_space(theme::GAP_L);
        breadcrumb(app, ui);

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .button("⚙")
                .on_hover_text("Device & Diagnostics")
                .clicked()
            {
                app.ui_state.nav = Nav::Diagnostics;
            }
            if ui.button("Import").clicked() {
                app.ui_state.nav = Nav::Import;
            }
            sync_control(app, ui, ctx);
        });
    });
}

/// Connected-device pill; opens Device & Diagnostics (PRD §6, not a raw
/// MIDI-port menu).
fn device_pill(app: &mut RytmdashApp, ui: &mut egui::Ui) {
    let snapshot = app.watcher.snapshot();
    let (color, label, hover) = match &snapshot.device {
        DeviceState::Connected(info) => (theme::SUCCESS, "Analog Rytm MKII", info.describe()),
        DeviceState::Searching => (
            theme::TEXT_MUTED,
            "No device",
            "Connect the Rytm over USB. On the device: GLOBAL SETTINGS → SYSTEM → USB CONFIG → \
             USB-MIDI, and MIDI CONFIG → PORT CONFIG input/output → USB."
                .to_owned(),
        ),
    };
    let response = Frame::new()
        .fill(theme::SURFACE)
        .stroke(Stroke::new(1.0, theme::BORDER))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(Margin::symmetric(10, 5))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                widgets::status_dot(ui, color, label);
            });
        })
        .response
        .interact(Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(hover);
    if response.clicked() {
        app.ui_state.nav = Nav::Diagnostics;
    }
}

/// Clickable breadcrumb while inside the project workspace.
fn breadcrumb(app: &mut RytmdashApp, ui: &mut egui::Ui) {
    if app.ui_state.nav != Nav::Projects {
        return;
    }
    let mut go_projects = false;
    let mut go_tab = false;
    ui.horizontal(|ui| {
        if ui
            .link(RichText::new("Projects").color(theme::TEXT_SECONDARY))
            .clicked()
        {
            go_projects = true;
        }
        ui.label(RichText::new("/").color(theme::TEXT_MUTED));
        ui.label(RichText::new("Active project").color(theme::TEXT_SECONDARY));
        let tab_name = match app.ui_state.tab {
            ProjectTab::Kits => "Kits",
            ProjectTab::Samples => "Samples",
            ProjectTab::Patterns => "Patterns",
            ProjectTab::Overview => "Overview",
        };
        ui.label(RichText::new("/").color(theme::TEXT_MUTED));
        if ui
            .link(RichText::new(tab_name).color(theme::TEXT_SECONDARY))
            .clicked()
        {
            go_tab = true;
        }
        if app.ui_state.tab == ProjectTab::Kits {
            if let Some(model) = &app.model {
                if let Some(kit) = app
                    .ui_state
                    .kits
                    .selected_kit
                    .and_then(|k| model.kits.get(k))
                {
                    ui.label(RichText::new("/").color(theme::TEXT_MUTED));
                    ui.label(
                        RichText::new(names::kit_display_name(&kit.name, kit.index).text())
                            .color(theme::TEXT_PRIMARY),
                    );
                }
            }
        }
    });
    if go_projects {
        app.ui_state.nav = Nav::Projects;
        app.ui_state.kits.selected_track = None;
    }
    if go_tab {
        app.ui_state.kits.selected_track = None;
    }
}

/// The Sync control and its states (PRD §6).
fn sync_control(app: &mut RytmdashApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    let in_port = app.connected_in_port();
    match app.sync.status() {
        SyncStatus::Running(progress) => {
            if ui.button("×").on_hover_text("Cancel sync").clicked() {
                app.sync.cancel();
            }
            ui.spinner();
            ui.label(
                RichText::new(format!(
                    "{} {}/{}",
                    progress.stage, progress.done, progress.total
                ))
                .color(theme::TEXT_SECONDARY),
            );
        }
        status => {
            let enabled = in_port.is_some();
            let response = ui
                .add_enabled(enabled, egui::Button::new("⟳ Sync"))
                .on_hover_text("Refresh device data")
                .on_disabled_hover_text("Connect the Analog Rytm first");
            if response.clicked() {
                if let Some(port) = in_port {
                    let repaint_ctx = ctx.clone();
                    app.sync.start(port, move || repaint_ctx.request_repaint());
                }
            }
            if let SyncStatus::Failed(err) = status {
                ui.label(RichText::new("Sync failed").color(theme::DANGER))
                    .on_hover_text(err);
            }
            sync_result_badge(app, ui);
        }
    }
}

/// Partial/success state after the last sync; partial opens Diagnostics.
fn sync_result_badge(app: &mut RytmdashApp, ui: &mut egui::Ui) {
    let Some(info) = app.last_sync else {
        return;
    };
    if info.missed > 0 {
        let response = ui
            .label(
                RichText::new(format!("Partial sync · {} unavailable", info.missed))
                    .color(theme::WARNING),
            )
            .interact(Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text("Some objects did not answer. Click for details.");
        if response.clicked() {
            app.ui_state.nav = Nav::Diagnostics;
        }
    } else {
        ui.label(
            RichText::new(format!("Synced {}", ago(info.at)))
                .color(theme::TEXT_MUTED)
                .size(theme::FONT_META),
        );
    }
}
