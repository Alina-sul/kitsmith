//! The project workspace: header, state badge, tab row, tab routing
//! (PRD §8–§9). Selecting a project in the app never touches the device.

use eframe::egui::{self, RichText};

use crate::gui::app::{ago, page_title, ProjectSelection, ProjectTab, RytmdashApp};
use crate::gui::views::{kits, overview, patterns, samples};
use crate::gui::{theme, widgets};
use crate::model::names;

pub fn show(app: &mut RytmdashApp, ui: &mut egui::Ui) {
    match app.ui_state.project {
        ProjectSelection::Active => active_project(app, ui),
        ProjectSelection::Slot(id) => unindexed_project(app, ui, id),
    }
}

/// A +Drive project slot whose contents are not indexed yet (WP11). The UI is
/// honest about that instead of faking contents (PRD §27), and hosts the
/// local-label editor — the rename Rytmdash can offer today.
fn unindexed_project(app: &mut RytmdashApp, ui: &mut egui::Ui, id: u32) {
    let native_name = app
        .model
        .as_ref()
        .and_then(|m| m.projects.iter().find(|p| p.id == id))
        .map(|p| p.name.clone())
        .unwrap_or_default();
    let native_display = names::project_display_name(&native_name, id)
        .text()
        .to_owned();
    let alias = app
        .local_meta
        .project_alias(id, &native_name)
        .map(str::to_owned);

    ui.label(
        RichText::new(format!("Project {id:03}"))
            .color(theme::TEXT_MUTED)
            .size(theme::FONT_META),
    );
    match &alias {
        Some(alias) => {
            ui.label(
                RichText::new(alias)
                    .size(theme::FONT_PAGE_TITLE)
                    .color(theme::TEXT_PRIMARY)
                    .strong()
                    .italics(),
            )
            .on_hover_text("Local label — the device still shows the native name.");
            ui.label(
                RichText::new(format!("Native name: {native_display}"))
                    .color(theme::TEXT_SECONDARY)
                    .size(theme::FONT_META),
            );
        }
        None => page_title(ui, &native_display),
    }
    ui.add_space(theme::GAP_L);

    alias_editor(app, ui, id, &native_name, alias.as_deref());

    widgets::inspector_group(ui, "NOT INDEXED", |ui| {
        ui.label(
            "Rytmdash can list this +Drive project slot but has not indexed its contents yet.",
        );
        ui.add_space(4.0);
        ui.label(
            RichText::new(
                "Browsing kits and samples of non-active projects arrives with the project \
                 indexer milestone. Until then, load the project on the device and press Sync \
                 to inspect it.",
            )
            .color(theme::TEXT_SECONDARY),
        );
    });
}

/// The local-label editor (PRD §32): a rename that lives in Rytmdash only.
/// No spec'd protocol operation renames a +Drive project natively, and the
/// UI says so instead of pretending to write the device.
fn alias_editor(
    app: &mut RytmdashApp,
    ui: &mut egui::Ui,
    id: u32,
    native_name: &str,
    current_alias: Option<&str>,
) {
    // (Re)seed the draft when the editor targets a different slot.
    if app.ui_state.alias_draft_slot != Some(id) {
        app.ui_state.alias_draft = current_alias.unwrap_or_default().to_owned();
        app.ui_state.alias_draft_slot = Some(id);
    }

    widgets::inspector_group(ui, "RYTMDASH LABEL (LOCAL ONLY)", |ui| {
        ui.label(
            RichText::new(
                "Shown instead of the native name inside Rytmdash. Nothing is written to the \
                 device — no verified protocol operation renames a project natively yet.",
            )
            .color(theme::TEXT_SECONDARY)
            .size(theme::FONT_META),
        );
        ui.add_space(4.0);
        let submitted = ui
            .add(
                egui::TextEdit::singleline(&mut app.ui_state.alias_draft)
                    .hint_text("e.g. Live set A")
                    .desired_width(280.0),
            )
            .lost_focus()
            && ui.input(|i| i.key_pressed(egui::Key::Enter));
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let draft = app.ui_state.alias_draft.trim().to_owned();
            let unchanged = current_alias.unwrap_or_default() == draft;
            if (ui
                .add_enabled(!unchanged, egui::Button::new("Save label"))
                .clicked()
                || (submitted && !unchanged))
                && !draft.is_empty()
            {
                app.local_meta.set_project_alias(id, &draft, native_name);
                app.save_local_meta();
                app.toast(format!("Local label saved for project {id:03}"));
            }
            if current_alias.is_some() && ui.button("Remove label").clicked() {
                app.local_meta.clear_project_alias(id);
                app.save_local_meta();
                app.ui_state.alias_draft.clear();
                app.toast(format!("Local label removed from project {id:03}"));
            }
        });
    });
}

fn active_project(app: &mut RytmdashApp, ui: &mut egui::Ui) {
    header(app, ui);
    ui.add_space(theme::GAP_S);

    if app.model.is_none() {
        empty_state(app, ui);
        return;
    }

    tab_row(app, ui);
    ui.add_space(theme::GAP_M);

    match app.ui_state.tab {
        ProjectTab::Kits => kits::show(app, ui),
        ProjectTab::Samples => samples::show(app, ui),
        ProjectTab::Patterns => patterns::show(app, ui),
        ProjectTab::Overview => overview::show(app, ui),
    }
}

/// Project header (PRD §8): name, live/cached/partial state, sync timestamp.
fn header(app: &mut RytmdashApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        page_title(ui, "Active project");
        ui.add_space(theme::GAP_S);
        let connected = app.connected_in_port().is_some();
        match (&app.model, app.last_sync) {
            (Some(_), Some(info)) if info.missed > 0 => {
                widgets::badge(ui, "PARTIAL", theme::WARNING);
                ui.label(
                    RichText::new(format!("{} items unavailable", info.missed))
                        .color(theme::TEXT_MUTED)
                        .size(theme::FONT_META),
                );
            }
            (Some(_), _) if connected => widgets::badge(ui, "LIVE", theme::SUCCESS),
            (Some(_), _) => {
                widgets::badge(ui, "CACHED", theme::TEXT_MUTED);
                ui.label(
                    RichText::new("device disconnected")
                        .color(theme::TEXT_MUTED)
                        .size(theme::FONT_META),
                );
            }
            (None, _) => {}
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if let Some(info) = app.last_sync {
                ui.label(
                    RichText::new(format!("synced {}", ago(info.at)))
                        .color(theme::TEXT_MUTED)
                        .size(theme::FONT_META),
                );
            }
        });
    });
    ui.label(
        RichText::new(
            "The project currently loaded on the device — the only one with indexed contents.",
        )
        .color(theme::TEXT_MUTED)
        .size(theme::FONT_META),
    );
}

/// Stable tab row under the header (PRD §9).
fn tab_row(app: &mut RytmdashApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        for (tab, label) in [
            (ProjectTab::Kits, "Kits"),
            (ProjectTab::Samples, "Samples"),
            (ProjectTab::Patterns, "Patterns"),
            (ProjectTab::Overview, "Overview"),
        ] {
            let selected = app.ui_state.tab == tab;
            if ui.selectable_label(selected, label).clicked() {
                app.ui_state.tab = tab;
                app.ui_state.kits.selected_track = None;
            }
        }
    });
}

fn empty_state(app: &RytmdashApp, ui: &mut egui::Ui) {
    let connected = app.connected_in_port().is_some();
    ui.add_space(48.0);
    ui.vertical_centered(|ui| {
        ui.label(
            RichText::new("No project data yet")
                .size(theme::FONT_PAGE_TITLE)
                .color(theme::TEXT_SECONDARY),
        );
        ui.add_space(theme::GAP_S);
        if connected {
            ui.label("Press ⟳ Sync in the top bar to pull the active project.");
        } else {
            ui.label("1 · Connect the Analog Rytm over USB");
            ui.label("2 · On the device: USB CONFIG → USB-MIDI, PORT CONFIG → USB");
            ui.label("3 · Press ⟳ Sync in the top bar");
        }
        ui.add_space(theme::GAP_S);
        ui.label(
            RichText::new(
                "You can drop .wav files anywhere in this window to queue them for import.",
            )
            .color(theme::TEXT_MUTED),
        );
    });
}
