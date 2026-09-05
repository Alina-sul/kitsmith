//! Left sidebar (PRD §7): top-level navigation plus the project browser.

use eframe::egui::{self, RichText, ScrollArea};

use crate::gui::app::{Nav, ProjectSelection, RytmdashApp};
use crate::gui::{theme, widgets};
use crate::model::names;

pub fn show(app: &mut RytmdashApp, ui: &mut egui::Ui) {
    nav_item(app, ui, Nav::Projects, "▦", "Projects");
    nav_item(app, ui, Nav::Drive, "🖴", "+Drive");
    let import_label = if !app.import.is_empty() {
        format!("Import ({})", app.import.len())
    } else {
        "Import".to_owned()
    };
    nav_item(app, ui, Nav::Import, "⇪", &import_label);
    nav_item(app, ui, Nav::Activity, "≡", "Activity");

    if app.ui_state.nav == Nav::Projects {
        ui.add_space(theme::GAP_L);
        project_browser(app, ui);
    }

    ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
        nav_item(app, ui, Nav::Diagnostics, "🔧", "Device & Diagnostics");
    });
}

fn nav_item(app: &mut RytmdashApp, ui: &mut egui::Ui, nav: Nav, icon: &str, label: &str) {
    let selected = app.ui_state.nav == nav;
    let clicked = widgets::list_row(ui, selected, 34.0, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(icon).color(theme::TEXT_SECONDARY));
            ui.label(RichText::new(label).color(if selected {
                theme::TEXT_PRIMARY
            } else {
                theme::TEXT_SECONDARY
            }));
        });
    });
    if clicked {
        app.ui_state.nav = nav;
    }
}

/// The project list (PRD §7.2–7.3): the synced active project first, then all
/// +Drive project slots. Slot contents are not indexed yet (WP11), and the UI
/// says so instead of faking them.
fn project_browser(app: &mut RytmdashApp, ui: &mut egui::Ui) {
    widgets::section_label(ui, "PROJECTS");
    ui.add_space(4.0);
    ui.add(
        egui::TextEdit::singleline(&mut app.ui_state.project_search)
            .hint_text("Search projects…")
            .desired_width(f32::INFINITY),
    );
    ui.add_space(theme::GAP_S);

    ScrollArea::vertical().show(ui, |ui| {
        active_project_row(app, ui);

        let Some(model) = &app.model else {
            ui.add_space(theme::GAP_S);
            ui.label(RichText::new("No device data yet — press Sync.").color(theme::TEXT_MUTED));
            return;
        };
        let query = app.ui_state.project_search.to_lowercase();
        let rows: Vec<(u32, String)> = model
            .projects
            .iter()
            .filter(|p| {
                query.is_empty()
                    || p.name.to_lowercase().contains(&query)
                    || format!("{:03}", p.id).contains(&query)
            })
            .map(|p| (p.id, p.name.clone()))
            .collect();
        if model.projects.is_empty() {
            ui.label(
                RichText::new("Project slots appear after a sync with file-protocol support.")
                    .color(theme::TEXT_MUTED),
            );
        }
        let mut select = None;
        for (id, name) in &rows {
            let selected = app.ui_state.project == ProjectSelection::Slot(*id);
            let clicked = widgets::list_row(ui, selected, theme::LIST_ROW_HEIGHT, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("{id:03}"))
                            .color(theme::TEXT_MUTED)
                            .monospace(),
                    );
                    ui.label(
                        RichText::new(names::project_display_name(name, *id).text())
                            .color(theme::TEXT_PRIMARY)
                            .strong(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new("Not indexed")
                                .color(theme::TEXT_MUTED)
                                .size(theme::FONT_META),
                        );
                    });
                });
            });
            if clicked {
                select = Some(ProjectSelection::Slot(*id));
            }
        }
        if let Some(selection) = select {
            app.ui_state.project = selection;
        }
    });
}

/// The pinned row for the synced active project.
fn active_project_row(app: &mut RytmdashApp, ui: &mut egui::Ui) {
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
    });
    if clicked {
        app.ui_state.project = ProjectSelection::Active;
    }
}
