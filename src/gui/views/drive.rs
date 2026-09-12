//! +Drive browser (PRD §23): live folder browsing when the device is
//! connected — right-click a file to load it into a project sample slot.
//! Falls back to the files known from loaded slots when offline.

use std::collections::BTreeMap;

use eframe::egui::{self, RichText, ScrollArea};

use crate::gui::app::{page_title, KitsmithApp};
use crate::gui::{theme, widgets};
use crate::model::ProjectModel;

pub fn show(app: &mut KitsmithApp, ui: &mut egui::Ui) {
    page_title(ui, "+Drive");
    let Some(port) = app.connected_in_port() else {
        ui.label(
            RichText::new("Global sample storage shared by all projects.")
                .color(theme::TEXT_SECONDARY),
        );
        ui.add_space(theme::GAP_S);
        ui.label(
            RichText::new(
                "Connect the Rytm to browse. Shown below: files known from the \
                 active project's loaded slots.",
            )
            .color(theme::TEXT_MUTED)
            .size(theme::FONT_META),
        );
        ui.add_space(theme::GAP_L);
        offline_view(ui, app.model.as_ref());
        return;
    };

    if app.ui_state.drive_path.is_empty() {
        app.ui_state.drive_path = "/".to_owned();
    }
    let path = app.ui_state.drive_path.clone();

    // Breadcrumb + folder actions.
    let in_factory = crate::device::files::is_protected_drive_path(&path);
    ui.horizontal(|ui| {
        let mut go: Option<String> = None;
        if ui.link("+Drive").clicked() {
            go = Some("/".to_owned());
        }
        let mut acc = String::new();
        for part in path.split('/').filter(|p| !p.is_empty()) {
            ui.label(RichText::new("/").color(theme::TEXT_MUTED));
            acc.push('/');
            acc.push_str(part);
            if ui.link(part).clicked() {
                go = Some(acc.clone());
            }
        }
        if let Some(target) = go {
            app.ui_state.drive_path = target;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if !in_factory && ui.button("New folder…").clicked() {
                app.ui_state.drive_new_folder_open = true;
            }
        });
    });
    match app.ui_state.pending_pad_assign {
        Some((kit_index, track)) => {
            ui.horizontal(|ui| {
                widgets::badge(ui, "ASSIGNING", theme::SAMPLE_PURPLE);
                ui.label(format!(
                    "Pick a sample for {} (kit {:02}) — right-click it and choose “Load + assign”.",
                    crate::model::names::track_label(track),
                    kit_index + 1
                ));
                if ui.button("Cancel").clicked() {
                    app.ui_state.pending_pad_assign = None;
                }
            });
        }
        None => {
            ui.label(
                RichText::new("Right-click a sample to load it into a free project slot.")
                    .color(theme::TEXT_MUTED)
                    .size(theme::FONT_META),
            );
        }
    }
    ui.add_space(theme::GAP_S);

    let path = app.ui_state.drive_path.clone();
    match app.drive.get(&path) {
        None => {
            let ctx = ui.ctx().clone();
            app.drive.request(port, path, move || ctx.request_repaint());
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Reading directory…");
            });
        }
        Some(entries) => {
            let mut descend: Option<String> = None;
            let mut load: Option<String> = None;
            let mut rename: Option<crate::device::files::DriveEntry> = None;
            let mut move_to: Option<crate::device::files::DriveEntry> = None;
            let mut delete: Option<crate::device::files::DriveEntry> = None;
            let loaded_paths: Vec<&str> = app
                .model
                .as_ref()
                .map(|m| m.samples.iter().map(|s| s.path.as_str()).collect())
                .unwrap_or_default();
            ScrollArea::vertical().show(ui, |ui| {
                if entries.is_empty() {
                    ui.label(RichText::new("Empty directory.").color(theme::TEXT_MUTED));
                }
                for entry in &entries {
                    let full = if path == "/" {
                        format!("/{}", entry.name)
                    } else {
                        format!("{path}/{}", entry.name)
                    };
                    // Factory content and write-protected entries get no
                    // write actions at all — reads only.
                    let writable = !in_factory && !entry.write_protected;
                    if entry.is_dir {
                        let row = widgets::list_row(ui, false, 32.0, |ui| {
                            ui.horizontal(|ui| {
                                widgets::folder_icon(ui);
                                ui.label(RichText::new(&entry.name).color(theme::TEXT_PRIMARY));
                            });
                        });
                        if row.clicked() {
                            descend = Some(full);
                        }
                        if writable {
                            row.context_menu(|ui| {
                                // Folder rename/move waits on the hardware
                                // probe (docs/hardware-verification.md).
                                if ui.button("Delete empty folder…").clicked() {
                                    delete = Some(entry.clone());
                                    ui.close();
                                }
                            });
                        }
                    } else {
                        let row = widgets::list_row(ui, false, 32.0, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("~").color(theme::SAMPLE_PURPLE));
                                ui.label(RichText::new(&entry.name).color(theme::TEXT_PRIMARY));
                                ui.label(
                                    RichText::new(format!("{} KB", entry.size / 1024))
                                        .color(theme::TEXT_MUTED)
                                        .size(theme::FONT_META),
                                );
                                if loaded_paths.contains(&full.as_str()) {
                                    widgets::badge(ui, "IN PROJECT", theme::SAMPLE_PURPLE);
                                }
                                if entry.write_protected {
                                    ui.label(
                                        RichText::new("write-protected")
                                            .color(theme::TEXT_MUTED)
                                            .size(theme::FONT_META),
                                    );
                                }
                            });
                        });
                        let assign_target = app.ui_state.pending_pad_assign;
                        row.context_menu(|ui| {
                            let label = match assign_target {
                                Some((_, track)) => format!(
                                    "Load + assign to {}…",
                                    crate::model::names::track_label(track)
                                ),
                                None => "Load into project slot…".to_owned(),
                            };
                            if ui.button(label).clicked() {
                                load = Some(full.clone());
                                ui.close();
                            }
                            if writable {
                                ui.separator();
                                if ui.button("Rename…").clicked() {
                                    rename = Some(entry.clone());
                                    ui.close();
                                }
                                if ui.button("Move to folder…").clicked() {
                                    move_to = Some(entry.clone());
                                    ui.close();
                                }
                                if ui.button("Delete…").clicked() {
                                    delete = Some(entry.clone());
                                    ui.close();
                                }
                            }
                        });
                    }
                }
            });
            if let Some(target) = descend {
                app.ui_state.drive_path = target;
            }
            if let Some(file) = load {
                app.ui_state.load_sample_slot = first_free_sample_slot(app.model.as_ref());
                app.ui_state.load_sample_modal = Some(file);
            }
            if let Some(entry) = rename {
                app.ui_state.drive_rename_draft = entry.name.clone();
                app.ui_state.drive_rename = Some(crate::gui::drive_modals::DriveTarget {
                    dir: path.clone(),
                    entry,
                });
            }
            if let Some(entry) = move_to {
                app.ui_state.drive_move_dest.clear();
                app.ui_state.drive_move = Some(crate::gui::drive_modals::DriveTarget {
                    dir: path.clone(),
                    entry,
                });
            }
            if let Some(entry) = delete {
                let ctx = ui.ctx().clone();
                crate::gui::drive_modals::open_delete(app, path.clone(), entry, &ctx);
            }
        }
    }
}

/// Smallest project sample slot with nothing loaded.
fn first_free_sample_slot(model: Option<&ProjectModel>) -> u8 {
    let Some(model) = model else {
        return 1;
    };
    (1..=127u8)
        .find(|slot| !model.samples.iter().any(|s| s.slot == *slot))
        .unwrap_or(1)
}

/// Offline fallback: the +Drive files the loaded slots point at.
fn offline_view(ui: &mut egui::Ui, model: Option<&ProjectModel>) {
    let Some(model) = model.filter(|m| !m.samples.is_empty()) else {
        ui.label(
            RichText::new("No resolved +Drive files yet — sync with the device connected.")
                .color(theme::TEXT_MUTED),
        );
        return;
    };
    let mut folders: BTreeMap<String, Vec<&crate::model::SampleSlotInfo>> = BTreeMap::new();
    for sample in &model.samples {
        let folder = sample
            .path
            .rsplit_once('/')
            .map_or("/", |(dir, _)| if dir.is_empty() { "/" } else { dir })
            .to_owned();
        folders.entry(folder).or_default().push(sample);
    }
    ScrollArea::vertical().show(ui, |ui| {
        for (folder, files) in &folders {
            widgets::section_label(ui, folder);
            ui.add_space(2.0);
            for file in files {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("~").color(theme::SAMPLE_PURPLE));
                    ui.label(RichText::new(&file.name).color(theme::TEXT_PRIMARY));
                    ui.label(
                        RichText::new(format!("{} KB", file.size / 1024))
                            .color(theme::TEXT_MUTED)
                            .size(theme::FONT_META),
                    );
                    widgets::badge(ui, "IN PROJECT", theme::SAMPLE_PURPLE);
                    ui.label(
                        RichText::new(format!("slot {:03}", file.slot))
                            .color(theme::TEXT_MUTED)
                            .size(theme::FONT_META),
                    );
                });
            }
            ui.add_space(theme::GAP_M);
        }
    });
}
