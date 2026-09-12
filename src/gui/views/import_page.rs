//! Import workspace (PRD §24): the batch queue table that replaces the
//! prototype bottom tray. Drop audio files anywhere in the window to add them.

use eframe::egui::{self, ProgressBar, RichText, ScrollArea};

use crate::gui::app::page_title;
use crate::gui::import::{ImportPanel, ImportState};
use crate::gui::{theme, widgets};

pub fn show(
    ui: &mut egui::Ui,
    import: &mut ImportPanel,
    device_in_port: Option<&str>,
    device_busy: bool,
    ctx: &egui::Context,
) {
    page_title(ui, "Import");
    ui.label(
        RichText::new(
            "Converted for Rytm: 48 kHz · 16-bit · mono. Uploads land in the +Drive \
             directory marked UPLOAD HERE (INCOMING/ by default).",
        )
        .color(theme::TEXT_SECONDARY),
    );
    ui.add_space(theme::GAP_M);

    if import.is_empty() {
        ui.add_space(theme::GAP_XL);
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new("The queue is empty")
                    .size(theme::FONT_SECTION)
                    .color(theme::TEXT_SECONDARY),
            );
            ui.label(
                RichText::new("Drop .wav files anywhere in the window to add them.")
                    .color(theme::TEXT_MUTED),
            );
        });
        return;
    }

    toolbar(ui, import, device_in_port, device_busy, ctx);
    ui.add_space(theme::GAP_S);

    let mut to_remove = None;
    ScrollArea::vertical().show(ui, |ui| {
        for (index, item_arc) in import.items().iter().enumerate() {
            let mut item = item_arc.lock().expect("import item lock poisoned");
            ui.horizontal(|ui| {
                let _ = ui
                    .checkbox(&mut item.selected, "")
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                ui.label(
                    RichText::new(&item.sample.name)
                        .color(theme::TEXT_PRIMARY)
                        .strong(),
                );
                if item.sample.source_rate > 0 {
                    ui.label(
                        RichText::new(format!(
                            "{:.2}s · {} KB · from {} Hz / {} ch",
                            item.sample.seconds(),
                            item.sample.byte_size() / 1024,
                            item.sample.source_rate,
                            item.sample.source_channels,
                        ))
                        .color(theme::TEXT_MUTED)
                        .size(theme::FONT_META),
                    );
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("×").on_hover_text("Remove from queue").clicked() {
                        to_remove = Some(index);
                    }
                    status_cell(ui, &item.state);
                });
            });
            ui.separator();
        }
    });
    if let Some(index) = to_remove {
        import.remove(index);
    }
}

fn toolbar(
    ui: &mut egui::Ui,
    import: &mut ImportPanel,
    device_in_port: Option<&str>,
    device_busy: bool,
    ctx: &egui::Context,
) {
    ui.horizontal(|ui| {
        if ui.button("Select all").clicked() {
            import.set_all_selected(true);
        }
        if ui.button("Select none").clicked() {
            import.set_all_selected(false);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let uploading = import.is_uploading();
            let response = ui
                .add_enabled(
                    device_in_port.is_some() && !device_busy,
                    egui::Button::new("⇪ Upload selected"),
                )
                .on_disabled_hover_text(if uploading {
                    "An upload batch is already running — samples transfer one at a time."
                } else if device_busy {
                    "Another device operation is running"
                } else {
                    "Connect the Analog Rytm first"
                });
            if response.clicked() {
                if let Some(port) = device_in_port {
                    import.upload_selected(port, ctx);
                }
            }
            if uploading && ui.button("Cancel upload").clicked() {
                import.cancel_uploads();
            }
        });
    });
}

fn status_cell(ui: &mut egui::Ui, state: &ImportState) {
    match state {
        ImportState::Converting => {
            ui.spinner();
            ui.label(
                RichText::new("Converting…")
                    .color(theme::TEXT_MUTED)
                    .size(theme::FONT_META),
            );
        }
        ImportState::Converted => {
            widgets::badge(ui, "READY", theme::TEXT_MUTED);
        }
        ImportState::Uploading(p) => {
            ui.add(
                ProgressBar::new(p.packets_sent as f32 / p.packets_total.max(1) as f32)
                    .desired_width(120.0)
                    .text(format!("{}/{}", p.packets_sent, p.packets_total)),
            );
        }
        ImportState::Sent { open_loop } => {
            widgets::badge(
                ui,
                if *open_loop {
                    "SENT (OPEN LOOP)"
                } else {
                    "SENT"
                },
                theme::SUCCESS,
            );
        }
        ImportState::Failed(err) => {
            ui.label(RichText::new("FAILED").color(theme::DANGER))
                .on_hover_text(err);
        }
    }
}
