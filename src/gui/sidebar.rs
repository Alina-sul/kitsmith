//! Left sidebar (PRD §7): top-level navigation plus the collapsible project
//! browser. Text-only rows — no icon glyphs, so nothing depends on font
//! coverage.

use eframe::egui::{self, CollapsingHeader, RichText, ScrollArea};

use crate::gui::app::{KitsmithApp, Nav, ProjectSelection};
use crate::gui::{theme, widgets};
use crate::model::names;

pub fn show(app: &mut KitsmithApp, ui: &mut egui::Ui) {
    nav_item(app, ui, Nav::Projects, "Projects");
    nav_item(app, ui, Nav::Drive, "+Drive");
    let import_label = if !app.import.is_empty() {
        format!("Import ({})", app.import.len())
    } else {
        "Import".to_owned()
    };
    nav_item(app, ui, Nav::Import, &import_label);
    nav_item(app, ui, Nav::Activity, "Activity");

    if app.ui_state.nav == Nav::Projects {
        ui.add_space(theme::GAP_M);
        let _ = CollapsingHeader::new(
            RichText::new("PROJECTS")
                .color(theme::TEXT_MUTED)
                .size(theme::FONT_META)
                .strong(),
        )
        .default_open(true)
        .show(ui, |ui| {
            project_browser(app, ui);
        })
        .header_response
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    }

    ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
        nav_item(app, ui, Nav::Diagnostics, "Device & Diagnostics");
    });
}

fn nav_item(app: &mut KitsmithApp, ui: &mut egui::Ui, nav: Nav, label: &str) {
    let selected = app.ui_state.nav == nav;
    let clicked = widgets::list_row(ui, selected, 34.0, |ui| {
        ui.label(RichText::new(label).color(if selected {
            theme::TEXT_PRIMARY
        } else {
            theme::TEXT_SECONDARY
        }));
    })
    .clicked();
    if clicked {
        app.ui_state.nav = nav;
    }
}

/// Payload carried when dragging a project row (drop on an empty slot to
/// copy it there).
pub(crate) struct ProjectDrag {
    pub src: u32,
}

/// The project browser (PRD §7.2–7.4): the synced active project pinned,
/// then all 128 +Drive slots — used ones as draggable rows, empty ones as
/// drop targets for project copies.
fn project_browser(app: &mut KitsmithApp, ui: &mut egui::Ui) {
    ui.add(
        egui::TextEdit::singleline(&mut app.ui_state.project_search)
            .hint_text("Search projects…")
            .desired_width(f32::INFINITY),
    );
    backup_controls(app, ui);
    ui.horizontal(|ui| {
        ui.toggle_value(&mut app.ui_state.rearrange, "Rearrange")
            .on_hover_text(
                "Shows empty slots and lets you drag a project onto one to move it there \
                 (copying stays one click away in the dialog).",
            );
        if app.ui_state.rearrange {
            ui.label(
                RichText::new("drag a project onto an empty slot")
                    .color(theme::TEXT_MUTED)
                    .size(theme::FONT_META),
            );
        }
    });
    ui.add_space(theme::GAP_S);

    let mut select = None;
    let mut rename_req: Option<(u32, String)> = None;
    let mut copy_req: Option<u32> = None;
    let mut move_req: Option<u32> = None;
    let mut swap_req: Option<u32> = None;
    let mut clear_req: Option<u32> = None;
    let mut drop_req: Option<(u32, u32)> = None;

    ScrollArea::vertical().show(ui, |ui| {
        // While a project is being dragged, auto-scroll when the pointer
        // nears the top/bottom edge so far-away slots are reachable.
        if egui::DragAndDrop::payload::<ProjectDrag>(ui.ctx()).is_some() {
            if let Some(pos) = ui.ctx().pointer_hover_pos() {
                let clip = ui.clip_rect();
                let margin = 56.0;
                let speed = 14.0;
                if pos.y < clip.top() + margin {
                    ui.scroll_with_delta(egui::vec2(0.0, speed));
                } else if pos.y > clip.bottom() - margin {
                    ui.scroll_with_delta(egui::vec2(0.0, -speed));
                }
                ui.ctx().request_repaint();
            }
        }

        active_project_row(app, ui);

        let Some(model) = &app.model else {
            ui.add_space(theme::GAP_S);
            ui.label(RichText::new("No device data yet — press Sync.").color(theme::TEXT_MUTED));
            return;
        };
        if model.projects.is_empty() {
            ui.label(
                RichText::new("Project slots appear after a sync with file-protocol support.")
                    .color(theme::TEXT_MUTED),
            );
            return;
        }
        let query = app.ui_state.project_search.to_lowercase();
        let busy = app.fileops.is_running();
        let rearrange = app.ui_state.rearrange;
        let mut any_shown = false;
        for id in 1..=128u32 {
            match model.projects.iter().find(|p| p.id == id) {
                Some(p) => {
                    let alias = app
                        .local_meta
                        .project_alias(p.id, &p.name)
                        .map(str::to_owned);
                    if !query.is_empty()
                        && !p.name.to_lowercase().contains(&query)
                        && !alias
                            .as_ref()
                            .is_some_and(|a| a.to_lowercase().contains(&query))
                        && !format!("{id:03}").contains(&query)
                    {
                        continue;
                    }
                    any_shown = true;
                    project_row(
                        app,
                        ui,
                        id,
                        &p.name,
                        alias.as_deref(),
                        busy,
                        rearrange,
                        &mut select,
                        &mut rename_req,
                        &mut copy_req,
                        &mut move_req,
                        &mut swap_req,
                        &mut clear_req,
                    );
                }
                None => {
                    // Empty slots show only in Rearrange mode (and never
                    // while searching); they are the drop targets for
                    // project copies (PRD §7.4).
                    if rearrange && query.is_empty() {
                        if let Some(src) = empty_slot_row(ui, id) {
                            drop_req = Some((src, id));
                        }
                    }
                }
            }
        }
        if !any_shown {
            ui.label(RichText::new("No project matches this search.").color(theme::TEXT_MUTED));
        }
    });

    if let Some(selection) = select {
        app.ui_state.project = selection;
    }
    if let Some((slot, native)) = rename_req {
        app.ui_state.rename_modal_slot = Some(slot);
        app.ui_state.rename_draft = names::clean(&native).to_owned();
    }
    if let Some(src) = copy_req {
        let dst = first_empty_slot(app).unwrap_or(1);
        app.ui_state.copy_modal = Some((src, dst));
    }
    if let Some(src) = move_req {
        let dst = first_empty_slot(app).unwrap_or(1);
        app.ui_state.slot_op_ack = false;
        app.ui_state.move_modal = Some((src, dst));
    }
    if let Some(a) = swap_req {
        // Default the partner to the next occupied slot.
        let b = app
            .model
            .as_ref()
            .and_then(|m| m.projects.iter().map(|p| p.id).find(|&id| id != a))
            .unwrap_or(a);
        app.ui_state.slot_op_ack = false;
        app.ui_state.swap_modal = Some((a, b));
    }
    if let Some(slot) = clear_req {
        app.ui_state.slot_op_ack = false;
        app.ui_state.clear_confirm_draft.clear();
        app.ui_state.clear_modal_slot = Some(slot);
    }
    if let Some((src, dst)) = drop_req {
        // Rearrange means relocation: a drop opens the Move dialog (Copy
        // stays one click away inside it).
        app.ui_state.slot_op_ack = false;
        app.ui_state.move_modal = Some((src, dst));
    }
}

/// Smallest project slot that holds no project.
fn first_empty_slot(app: &KitsmithApp) -> Option<u32> {
    let model = app.model.as_ref()?;
    (1..=128u32).find(|id| !model.projects.iter().any(|p| p.id == *id))
}

#[allow(clippy::too_many_arguments)]
fn project_row(
    app: &KitsmithApp,
    ui: &mut egui::Ui,
    id: u32,
    name: &str,
    alias: Option<&str>,
    ops_busy: bool,
    rearrange: bool,
    select: &mut Option<ProjectSelection>,
    rename_req: &mut Option<(u32, String)>,
    copy_req: &mut Option<u32>,
    move_req: &mut Option<u32>,
    swap_req: &mut Option<u32>,
    clear_req: &mut Option<u32>,
) {
    let selected = app.ui_state.project == ProjectSelection::Slot(id);
    let mut row_response: Option<egui::Response> = None;
    let content = |ui: &mut egui::Ui| {
        row_response = Some(widgets::list_row(
            ui,
            selected,
            theme::LIST_ROW_HEIGHT,
            |ui| {
                ui.horizontal(|ui| {
                    if rearrange {
                        widgets::drag_handle(ui);
                    }
                    ui.label(
                        RichText::new(format!("{id:03}"))
                            .color(theme::TEXT_MUTED)
                            .monospace(),
                    );
                    let text = match alias {
                        Some(alias) => RichText::new(alias)
                            .color(theme::TEXT_PRIMARY)
                            .strong()
                            .italics(),
                        None => RichText::new(names::project_display_name(name, id).text())
                            .color(theme::TEXT_PRIMARY)
                            .strong(),
                    };
                    let label = ui.add(egui::Label::new(text).truncate());
                    if alias.is_some() {
                        label.on_hover_text(format!(
                            "Local label · native name: {}",
                            names::project_display_name(name, id).text()
                        ));
                    }
                });
            },
        ));
    };
    // Rows are drag sources only in Rearrange mode; normally they just
    // select on click.
    let response = if rearrange {
        let drag_id = egui::Id::new(("project-drag", id));
        ui.dnd_drag_source(drag_id, ProjectDrag { src: id }, content)
            .response
    } else {
        ui.scope(content);
        row_response.expect("list_row rendered")
    };
    response.context_menu(|ui| {
        if ui.button("View").clicked() {
            *select = Some(ProjectSelection::Slot(id));
            ui.close();
        }
        if ui
            .add_enabled(!ops_busy, egui::Button::new("Rename on device…"))
            .clicked()
        {
            *rename_req = Some((id, name.to_owned()));
            ui.close();
        }
        if ui
            .add_enabled(!ops_busy, egui::Button::new("Copy to empty slot…"))
            .clicked()
        {
            *copy_req = Some(id);
            ui.close();
        }
        if ui
            .add_enabled(!ops_busy, egui::Button::new("Move to empty slot…"))
            .clicked()
        {
            *move_req = Some(id);
            ui.close();
        }
        if ui
            .add_enabled(!ops_busy, egui::Button::new("Swap with…"))
            .clicked()
        {
            *swap_req = Some(id);
            ui.close();
        }
        if ui
            .add_enabled(!ops_busy, egui::Button::new("Clear slot…"))
            .clicked()
        {
            *clear_req = Some(id);
            ui.close();
        }
    });
    if response.clicked() {
        *select = Some(ProjectSelection::Slot(id));
    }
}

/// A dim `Empty` row that accepts a dragged project. Returns the source slot
/// when a drag was dropped here.
fn empty_slot_row(ui: &mut egui::Ui, id: u32) -> Option<u32> {
    let frame = egui::Frame::new()
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(8, 4));
    let (_, payload) = ui.dnd_drop_zone::<ProjectDrag, ()>(frame, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("{id:03}"))
                    .color(theme::TEXT_MUTED)
                    .monospace(),
            );
            ui.label(
                RichText::new("Empty")
                    .color(theme::TEXT_MUTED)
                    .size(theme::FONT_META),
            );
        });
    });
    payload.map(|drag| drag.src)
}

/// "Back up all projects" — downloads every listed project payload into the
/// local cache (read-only on the device).
fn backup_controls(app: &mut KitsmithApp, ui: &mut egui::Ui) {
    use crate::gui::backup::BackupStatus;
    match app.backup.status() {
        BackupStatus::Running {
            current,
            done,
            total,
        } => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(
                    RichText::new(format!("Backing up {done}/{total} · {current}"))
                        .color(theme::TEXT_SECONDARY)
                        .size(theme::FONT_META),
                );
                if ui.button("×").on_hover_text("Cancel").clicked() {
                    app.backup.cancel();
                }
            });
        }
        status => {
            let projects: Vec<(u32, String)> = app
                .model
                .as_ref()
                .map(|m| m.projects.iter().map(|p| (p.id, p.name.clone())).collect())
                .unwrap_or_default();
            let in_port = app.connected_in_port();
            let enabled = in_port.is_some() && !projects.is_empty();
            let response = ui
                .add_enabled(enabled, egui::Button::new("Back up all projects"))
                .on_hover_text(
                    "Download every project's raw data into the local cache. Read-only on the \
                     device.",
                )
                .on_disabled_hover_text("Connect the Rytm and sync first");
            if response.clicked() {
                if let Some(port) = in_port {
                    let ctx = ui.ctx().clone();
                    app.backup
                        .start(port, projects, move || ctx.request_repaint());
                }
            }
            if let BackupStatus::Failed(err) = status {
                ui.label(RichText::new("Backup failed").color(theme::DANGER))
                    .on_hover_text(err);
            }
        }
    }
}

/// The pinned row for the synced active project.
fn active_project_row(app: &mut KitsmithApp, ui: &mut egui::Ui) {
    let selected = app.ui_state.project == ProjectSelection::Active;
    let has_model = app.model.is_some();
    let clicked = widgets::list_row(ui, selected, theme::LIST_ROW_HEIGHT, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Active project")
                    .color(theme::TEXT_PRIMARY)
                    .strong(),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if has_model {
                    widgets::badge(ui, "ACTIVE", theme::SUCCESS);
                } else {
                    ui.label(
                        RichText::new("Not synced")
                            .color(theme::TEXT_MUTED)
                            .size(theme::FONT_META),
                    );
                }
            });
        });
    })
    .clicked();
    if clicked {
        app.ui_state.project = ProjectSelection::Active;
    }
}
