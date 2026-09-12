//! +Drive management modals: rename, move, new folder, delete. Every confirm
//! compiles an `ops::Plan` and launches it through the shared `ExecutePanel`
//! — preview, session backup, preflight, write, read-back verify.

use std::sync::{Arc, Mutex};
use std::thread;

use eframe::egui::{self, RichText};

use crate::analysis::DriveRefScan;
use crate::cache;
use crate::device::files::DriveEntry;
use crate::gui::app::KitsmithApp;
use crate::gui::{theme, widgets};
use crate::ops::Plan;

/// One +Drive entry a modal is acting on.
#[derive(Clone)]
pub struct DriveTarget {
    /// Directory holding the entry.
    pub dir: String,
    pub entry: DriveEntry,
}

impl DriveTarget {
    fn full_path(&self) -> String {
        if self.dir == "/" {
            format!("/{}", self.entry.name)
        } else {
            format!("{}/{}", self.dir, self.entry.name)
        }
    }
}

/// State of the delete modal: the reference scan runs on a worker thread
/// (dozens of project payloads parse off the UI thread).
pub struct DriveDeleteModal {
    pub target: DriveTarget,
    pub confirm_draft: String,
    scan: Arc<Mutex<Option<DriveRefScan>>>,
}

/// What to refresh once the launched plan reports success.
pub struct DriveRefresh {
    /// Directory listings that are stale after the write.
    pub invalidate: Vec<String>,
    /// Exact loaded-sample path rewrite `(old, new)` after a file move/rename
    /// (the content hash is unchanged, so the slot keeps resolving).
    pub path_patch: Option<(String, String)>,
}

/// Opens the delete modal and starts the reference scan in the background.
pub fn open_delete(app: &mut KitsmithApp, dir: String, entry: DriveEntry, ctx: &egui::Context) {
    let scan = Arc::new(Mutex::new(None));
    let result = Arc::clone(&scan);
    let model = app.model.clone().unwrap_or_default();
    let hash = entry.hash;
    let size = entry.size;
    let repaint = ctx.clone();
    thread::Builder::new()
        .name("drive-refscan".into())
        .spawn(move || {
            let cache_dir = cache::default_dir();
            let manifest = cache::load_project_manifest(&cache_dir);
            let found = crate::analysis::refscan::scan(
                &model,
                &manifest,
                |slot| cache::load_project_backup(&cache_dir, slot),
                hash,
                size,
            );
            *result.lock().expect("refscan lock poisoned") = Some(found);
            repaint.request_repaint();
        })
        .expect("failed to spawn refscan thread");
    app.ui_state.drive_delete = Some(DriveDeleteModal {
        target: DriveTarget { dir, entry },
        confirm_draft: String::new(),
        scan,
    });
}

/// Renders whichever +Drive modal is open. Call every frame.
pub fn show(app: &mut KitsmithApp, ctx: &egui::Context) {
    rename_modal(app, ctx);
    move_modal(app, ctx);
    new_folder_modal(app, ctx);
    delete_modal(app, ctx);
}

fn rename_modal(app: &mut KitsmithApp, ctx: &egui::Context) {
    let Some(target) = app.ui_state.drive_rename.clone() else {
        return;
    };
    let mut close = false;
    let mut launch: Option<(Plan, DriveRefresh)> = None;
    egui::Window::new(format!("Rename {}?", target.entry.name))
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("New name").color(theme::TEXT_SECONDARY));
                ui.add(
                    egui::TextEdit::singleline(&mut app.ui_state.drive_rename_draft)
                        .desired_width(theme::NAME_FIELD_WIDTH),
                );
            });
            let plan = Plan::rename_drive_entry(
                &target.dir,
                &target.entry,
                app.ui_state.drive_rename_draft.trim(),
                app.execute.backup_taken(),
            );
            plan_footer(app, ui, &plan, "Rename", &mut close, |plan| {
                let new_name = app.ui_state.drive_rename_draft.trim();
                let new_path = join(&target.dir, new_name);
                launch = Some((
                    plan,
                    DriveRefresh {
                        invalidate: vec![target.dir.clone()],
                        path_patch: (!target.entry.is_dir).then(|| (target.full_path(), new_path)),
                    },
                ));
            });
        });
    finish(app, ctx, close, launch, |ui_state| {
        ui_state.drive_rename = None;
    });
}

fn move_modal(app: &mut KitsmithApp, ctx: &egui::Context) {
    let Some(target) = app.ui_state.drive_move.clone() else {
        return;
    };
    let mut close = false;
    let mut launch: Option<(Plan, DriveRefresh)> = None;
    let known_dirs = app.drive.cached_dirs();
    egui::Window::new(format!("Move {} where?", target.entry.name))
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Destination folder").color(theme::TEXT_SECONDARY));
                ui.add(
                    egui::TextEdit::singleline(&mut app.ui_state.drive_move_dest)
                        .desired_width(240.0),
                );
            });
            if !known_dirs.is_empty() {
                ui.label(
                    RichText::new("Folders browsed this session:")
                        .color(theme::TEXT_MUTED)
                        .size(theme::FONT_META),
                );
                for dir in &known_dirs {
                    if ui.link(dir).clicked() {
                        app.ui_state.drive_move_dest = dir.clone();
                    }
                }
            }
            let plan = Plan::move_drive_entry(
                &target.dir,
                &target.entry,
                app.ui_state.drive_move_dest.trim(),
                app.execute.backup_taken(),
            );
            plan_footer(app, ui, &plan, "Move", &mut close, |plan| {
                let dest_dir = app.ui_state.drive_move_dest.trim().to_owned();
                let new_path = join(&dest_dir, &target.entry.name);
                launch = Some((
                    plan,
                    DriveRefresh {
                        invalidate: vec![target.dir.clone(), dest_dir],
                        path_patch: (!target.entry.is_dir).then(|| (target.full_path(), new_path)),
                    },
                ));
            });
        });
    finish(app, ctx, close, launch, |ui_state| {
        ui_state.drive_move = None;
    });
}

fn new_folder_modal(app: &mut KitsmithApp, ctx: &egui::Context) {
    if !app.ui_state.drive_new_folder_open {
        return;
    }
    let parent = app.ui_state.drive_path.clone();
    let mut close = false;
    let mut launch: Option<(Plan, DriveRefresh)> = None;
    egui::Window::new("New folder")
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(RichText::new(format!("Inside {parent}")).color(theme::TEXT_SECONDARY));
            ui.add(
                egui::TextEdit::singleline(&mut app.ui_state.drive_new_folder_draft)
                    .hint_text("folder name")
                    .desired_width(theme::NAME_FIELD_WIDTH),
            );
            let plan = Plan::create_drive_dir(
                &parent,
                app.ui_state.drive_new_folder_draft.trim(),
                app.execute.backup_taken(),
            );
            plan_footer(app, ui, &plan, "Create", &mut close, |plan| {
                launch = Some((
                    plan,
                    DriveRefresh {
                        invalidate: vec![parent.clone()],
                        path_patch: None,
                    },
                ));
            });
        });
    finish(app, ctx, close, launch, |ui_state| {
        ui_state.drive_new_folder_open = false;
        ui_state.drive_new_folder_draft.clear();
    });
}

fn delete_modal(app: &mut KitsmithApp, ctx: &egui::Context) {
    let Some(modal) = &app.ui_state.drive_delete else {
        return;
    };
    let target = modal.target.clone();
    let scan = modal.scan.lock().expect("refscan lock poisoned").clone();
    let mut close = false;
    let mut launch: Option<(Plan, DriveRefresh)> = None;
    let mut confirm_draft = modal.confirm_draft.clone();

    egui::Window::new(format!("Delete {}?", target.entry.name))
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            if target.entry.is_dir {
                ui.label("Only an empty folder can be deleted.");
                let plan =
                    Plan::delete_drive_dir(&target.dir, &target.entry, app.execute.backup_taken());
                plan_footer(app, ui, &plan, "Delete folder", &mut close, |plan| {
                    launch = Some((
                        plan,
                        DriveRefresh {
                            invalidate: vec![target.dir.clone(), target.full_path()],
                            path_patch: None,
                        },
                    ));
                });
                return;
            }

            match &scan {
                None => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Scanning known projects for uses of this sample…");
                    });
                }
                Some(scan) => {
                    delete_scan_summary(ui, scan);
                    ui.add_space(theme::GAP_S);
                    ui.label(
                        RichText::new(
                            "Deleting cannot be undone from Kitsmith. A local WAV copy is \
                             saved to the cache first, but it cannot be re-uploaded yet.",
                        )
                        .color(theme::WARNING),
                    );
                    ui.add_space(theme::GAP_S);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("Type “{}” to confirm", target.entry.name))
                                .color(theme::TEXT_SECONDARY),
                        );
                        ui.add(
                            egui::TextEdit::singleline(&mut confirm_draft)
                                .desired_width(theme::NAME_FIELD_WIDTH),
                        );
                    });
                    let confirmed = confirm_draft == target.entry.name;
                    let plan = Plan::delete_drive_file(
                        &target.dir,
                        &target.entry,
                        scan,
                        app.execute.backup_taken(),
                    );
                    if confirmed {
                        plan_footer(app, ui, &plan, "Delete sample", &mut close, |plan| {
                            launch = Some((
                                plan,
                                DriveRefresh {
                                    invalidate: vec![target.dir.clone()],
                                    path_patch: None,
                                },
                            ));
                        });
                    } else {
                        if let Err(e) = &plan {
                            ui.label(RichText::new(e.to_string()).color(theme::DANGER));
                        }
                        ui.horizontal(|ui| {
                            ui.add_enabled(false, egui::Button::new("Delete sample"))
                                .on_disabled_hover_text("Type the exact file name first");
                            if ui.button("Cancel").clicked() {
                                close = true;
                            }
                        });
                    }
                }
            }
        });

    if let Some(modal) = &mut app.ui_state.drive_delete {
        modal.confirm_draft = confirm_draft;
    }
    finish(app, ctx, close, launch, |ui_state| {
        ui_state.drive_delete = None;
    });
}

/// The scan verdict: who uses the sample, and what could not be checked.
fn delete_scan_summary(ui: &mut egui::Ui, scan: &DriveRefScan) {
    if scan.has_references() {
        widgets::section_label(ui, "STILL IN USE");
        if !scan.active_slots.is_empty() {
            ui.label(
                RichText::new(format!(
                    "Active project: sample slot{} {}",
                    if scan.active_slots.len() == 1 {
                        ""
                    } else {
                        "s"
                    },
                    scan.active_slots
                        .iter()
                        .map(|s| format!("{s:03}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
                .color(theme::DANGER),
            );
        }
        for hit in &scan.backed_up {
            ui.label(
                RichText::new(format!(
                    "Project {:03} “{}”: slot{} {}",
                    hit.slot_id,
                    hit.name,
                    if hit.sample_slots.len() == 1 { "" } else { "s" },
                    hit.sample_slots
                        .iter()
                        .map(|s| format!("{s:03}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
                .color(theme::DANGER),
            );
        }
    } else {
        ui.label(RichText::new("No known project uses this sample.").color(theme::TEXT_SECONDARY));
    }
    if !scan.unscanned.is_empty() {
        ui.label(
            RichText::new(format!(
                "{} device project{} ha{} never been backed up and could not be checked.",
                scan.unscanned.len(),
                if scan.unscanned.len() == 1 { "" } else { "s" },
                if scan.unscanned.len() == 1 { "s" } else { "ve" },
            ))
            .color(theme::WARNING),
        );
    }
}

/// Shared modal footer: backup/verify notices, collapsible steps, and the
/// confirm/cancel row. `on_confirm` fires with the built plan.
fn plan_footer(
    app: &KitsmithApp,
    ui: &mut egui::Ui,
    plan: &Result<Plan, crate::ops::OpError>,
    confirm_label: &str,
    close: &mut bool,
    on_confirm: impl FnOnce(Plan),
) {
    ui.add_space(theme::GAP_S);
    match plan {
        Ok(plan) => {
            if !app.execute.backup_taken() {
                ui.label(
                    RichText::new("A backup will be created before writing.")
                        .color(theme::TEXT_SECONDARY),
                );
            }
            ui.label(
                RichText::new("The result is verified by re-reading the folder.")
                    .color(theme::TEXT_SECONDARY),
            );
            ui.collapsing("Technical steps", |ui| {
                for step in &plan.steps {
                    ui.label(
                        RichText::new(format!("• {}", step.describe()))
                            .color(theme::TEXT_SECONDARY)
                            .size(theme::FONT_META),
                    );
                }
            });
        }
        Err(e) => {
            ui.label(RichText::new(e.to_string()).color(theme::DANGER));
        }
    }
    ui.add_space(theme::GAP_S);
    ui.horizontal(|ui| {
        let connected = app.connected_in_port().is_some();
        let busy = app.execute.is_busy();
        let hint = if !connected {
            "Connect the Analog Rytm first"
        } else if busy {
            "Another device operation is still running"
        } else {
            "Fix the error above first"
        };
        if ui
            .add_enabled(
                connected && plan.is_ok() && !busy,
                egui::Button::new(confirm_label),
            )
            .on_disabled_hover_text(hint)
            .clicked()
        {
            if let Ok(plan) = plan.clone() {
                on_confirm(plan);
            }
        }
        if ui.button("Cancel").clicked() {
            *close = true;
        }
    });
}

/// Launches a confirmed plan (recording the post-success refresh) and closes
/// the modal on launch or cancel.
fn finish(
    app: &mut KitsmithApp,
    ctx: &egui::Context,
    close: bool,
    launch: Option<(Plan, DriveRefresh)>,
    clear: impl FnOnce(&mut crate::gui::app::UiState),
) {
    if let Some((plan, refresh)) = launch {
        if let Some(port) = app.connected_in_port() {
            if app.execute.launch(plan, port, ctx.clone(), None) {
                app.ui_state.pending_drive = Some(refresh);
                clear(&mut app.ui_state);
            } else {
                app.toast(
                    "Another device operation is still running — try again when it finishes."
                        .to_owned(),
                );
            }
        }
    } else if close {
        clear(&mut app.ui_state);
    }
}

fn join(dir: &str, name: &str) -> String {
    if dir == "/" {
        format!("/{name}")
    } else {
        format!("{dir}/{name}")
    }
}
