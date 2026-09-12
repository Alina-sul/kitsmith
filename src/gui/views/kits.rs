//! Kits workspace (PRD Part IV): kit list pane, responsive 12-pad grid with
//! the new card anatomy, and the right inspector (track details or the
//! draggable sample rail). Pads are drop zones; a landed drag becomes a
//! [`PendingAssign`] for the plan modal — this view never mutates the device.

use eframe::egui::{
    self, CornerRadius, Frame, Id, Margin, RichText, ScrollArea, Sense, Stroke, Vec2,
};

use crate::gui::app::{KitFilter, RytmdashApp};
use crate::gui::execute::PendingAssign;
use crate::gui::{theme, widgets};
use crate::model::names::{self, ResolvedName};
use crate::model::{pattern_display_name, KitModel, ProjectModel};

use super::SlotDrag;

pub fn show(app: &mut RytmdashApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    let connected = app.connected_in_port();
    let browsing = app.is_browsing_snapshot();
    let project_sel = app.ui_state.project;
    let can_capture = connected.is_some() && !app.device_busy();
    let mut request_capture_kit: Option<usize> = None;
    let mut request_pad_assign: Option<(usize, usize)> = None;
    let mut open_kit_paste = false;
    let mut clear_kit_clipboard = false;
    let mut pending = None;

    {
        let model = match project_sel {
            crate::gui::app::ProjectSelection::Active => app.model.as_ref(),
            crate::gui::app::ProjectSelection::Slot(id) => {
                app.raw_models.get(&id).and_then(Option::as_ref)
            }
        };
        let Some(model) = model else {
            return;
        };
        let kit_clipboard = &app.kit_clipboard;
        let state = &mut app.ui_state.kits;

        let linked = model.linked_kit_indices();
        // Keep a valid kit selected (first linked kit by default).
        let selected_kit = state
            .selected_kit
            .filter(|k| model.kits.get(*k).is_some())
            .or_else(|| linked.first().copied())
            .unwrap_or(0);
        state.selected_kit = Some(selected_kit);

        // Copying a kit needs the device only when the source is the live
        // active project; snapshot kits copy straight from the backup file.
        let can_copy_kit = browsing || can_capture;
        egui::Panel::left("kit-list")
            .default_size(theme::KIT_LIST_WIDTH)
            .resizable(false)
            .frame(Frame::new().inner_margin(Margin::symmetric(0, 4)))
            .show(ui, |ui| {
                kit_list(
                    ui,
                    model,
                    &linked,
                    state,
                    can_copy_kit,
                    &mut request_capture_kit,
                );
            });

        if state.selected_track.is_some() || !state.rail_hidden {
            egui::Panel::right("kit-inspector")
                .default_size(theme::INSPECTOR_WIDTH)
                .resizable(true)
                .frame(Frame::new().inner_margin(Margin::symmetric(10, 4)))
                .show(ui, |ui| {
                    if state.selected_track.is_some() {
                        track_inspector(ui, model, state, browsing, &mut request_pad_assign);
                    } else {
                        sample_rail(ui, model, state);
                    }
                });
        }

        egui::CentralPanel::default()
            .frame(Frame::new().inner_margin(Margin::symmetric(12, 4)))
            .show(ui, |ui| {
                if !browsing {
                    if let Some(clip) = kit_clipboard {
                        kit_clipboard_banner(
                            ui,
                            clip,
                            &mut open_kit_paste,
                            &mut clear_kit_clipboard,
                        );
                        ui.add_space(theme::GAP_S);
                    }
                }
                if let Some(kit) = model.kits.get(selected_kit) {
                    kit_header(ui, model, kit);
                    ui.add_space(theme::GAP_S);
                    mode_switch(ui, state);
                    ui.add_space(theme::GAP_M);
                    egui::ScrollArea::both()
                        .id_salt("pad-grid-scroll")
                        .show(ui, |ui| {
                            if let Some(assign) =
                                pad_grid(ui, model, kit, state, browsing, &mut request_pad_assign)
                            {
                                pending = Some(assign);
                            }
                        });
                }
            });
    }

    // Snapshot browsing is read-only: drops must not open the assign flow.
    if pending.is_some() && !browsing {
        app.execute.pending = pending;
    }
    if let Some((kit_index, track)) = request_pad_assign {
        app.ui_state.pending_pad_assign = Some((kit_index, track));
        app.ui_state.nav = crate::gui::app::Nav::Drive;
    }
    if let Some(index) = request_capture_kit {
        if let crate::gui::app::ProjectSelection::Slot(id) = project_sel {
            // Copy straight out of the backup — no device I/O needed.
            app.copy_kit_from_backup(id, index);
        } else if let (Some(port), Some(model)) = (connected.clone(), app.model.clone()) {
            let repaint = ctx.clone();
            app.capture.start(
                port,
                model,
                crate::gui::transfer_ctl::CaptureJob::Kit(index),
                move || repaint.request_repaint(),
            );
        }
    }
    if open_kit_paste {
        app.ui_state.kit_paste_modal_open = true;
    }
    if clear_kit_clipboard {
        app.kit_clipboard = None;
        crate::transfer::clear_saved_kit();
    }
    kit_paste_modal(app, &ctx);
}

/// The kit transfer clipboard banner.
fn kit_clipboard_banner(
    ui: &mut egui::Ui,
    clip: &crate::transfer::KitClipboard,
    open_paste: &mut bool,
    clear: &mut bool,
) {
    Frame::new()
        .fill(theme::SURFACE)
        .stroke(Stroke::new(1.0, theme::BORDER))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                widgets::badge(ui, "KIT CLIPBOARD", theme::SAMPLE_PURPLE);
                ui.label(
                    RichText::new(&clip.source_hint)
                        .color(theme::TEXT_PRIMARY)
                        .strong(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Clear").clicked() {
                        *clear = true;
                    }
                    if ui.button("Paste into this project…").clicked() {
                        *open_paste = true;
                    }
                });
            });
            ui.label(
                RichText::new(
                    "To paste into another project: load that project on the Rytm, press Sync, \
                     then paste. (The device only lets us write into the active project.)",
                )
                .color(theme::TEXT_MUTED)
                .size(theme::FONT_META),
            );
        });
}

/// The kit paste-plan modal (PRD §30–§31, kit-only slice).
fn kit_paste_modal(app: &mut RytmdashApp, ctx: &egui::Context) {
    if !app.ui_state.kit_paste_modal_open {
        return;
    }
    let (Some(clip), Some(model)) = (&app.kit_clipboard, app.model.as_ref()) else {
        app.ui_state.kit_paste_modal_open = false;
        return;
    };
    let preview = crate::transfer::plan_paste_kit(clip, model);
    let connected = app.connected_in_port();
    let busy = app.device_busy();
    let mut close = false;
    let mut confirm = false;
    let mut launch: Option<crate::ops::Plan> = None;

    egui::Window::new(format!(
        "Copy kit {:02} into this project?",
        clip.kit_index + 1
    ))
    .collapsible(false)
    .resizable(false)
    .show(ctx, |ui| {
        ui.label(
            RichText::new(
                "Pasting writes into whatever project is ACTIVE on the device. Make sure \
                     the target project is loaded and synced.",
            )
            .color(theme::WARNING),
        );
        ui.add_space(theme::GAP_S);
        ui.label(format!(
            "{} will be replaced in this project.",
            preview.overwrite_kit
        ));
        if !preview.kit_used_by.is_empty() {
            ui.label(
                RichText::new(format!(
                    "Warning: {} use this kit slot — replacing it changes them too.",
                    preview.kit_used_by.join(" · ")
                ))
                .color(theme::WARNING),
            );
        }
        ui.add_space(theme::GAP_S);
        widgets::section_label(ui, "SAMPLE MAPPING");
        if preview.slot_remap.is_empty() && preview.missing.is_empty() {
            ui.label(RichText::new("No samples referenced.").color(theme::TEXT_MUTED));
        }
        for (src, dst) in &preview.slot_remap {
            let name = clip
                .samples
                .iter()
                .find(|d| d.source_slot == *src)
                .map_or("", |d| d.name.as_str());
            ui.label(format!(
                "{name} · slot {src:02} → slot {dst:02} (already loaded here)"
            ));
        }
        for dep in &preview.missing {
            ui.label(
                RichText::new(format!(
                    "{} · not loaded in this project — reference will be cleared",
                    dep.name
                ))
                .color(theme::DANGER),
            );
        }
        ui.add_space(theme::GAP_S);
        ui.label(
            RichText::new("A backup of this project is pulled before anything is written.")
                .color(theme::TEXT_SECONDARY)
                .size(theme::FONT_META),
        );
        ui.add_space(theme::GAP_S);
        ui.horizontal(|ui| {
            let button = egui::Button::new(format!(
                "Copy kit {:02} into this project",
                clip.kit_index + 1
            ));
            if ui
                .add_enabled(connected.is_some() && !busy, button)
                .on_disabled_hover_text("Connect the Rytm and let any running device task finish")
                .clicked()
            {
                confirm = true;
            }
            if ui.button("Cancel").clicked() {
                close = true;
            }
        });
    });

    if confirm {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::transfer::materialize_kit(clip, &preview)
        }))
        .unwrap_or_else(|_| Err("internal error while re-encoding the captured kit".into()));
        match result {
            Ok(paste) => launch = Some(crate::ops::Plan::paste_kit(&paste)),
            Err(e) => {
                app.ui_state.kit_paste_modal_open = false;
                app.toast(format!("Paste failed before writing anything: {e}"));
                return;
            }
        }
    }

    if let (Some(plan), Some(port)) = (launch, connected) {
        app.ui_state.kit_paste_modal_open = false;
        app.execute.launch(
            plan,
            port,
            ctx.clone(),
            Some(
                "Now save the project on the device (PROJECT → SAVE) to persist the copy."
                    .to_owned(),
            ),
        );
    } else if close {
        app.ui_state.kit_paste_modal_open = false;
    }
}

// ---- Kit list -------------------------------------------------------------

/// Kit list pane with In use / All / Unused filters and search (PRD §11).
/// Right-click a kit for transfer actions (PRD §36).
fn kit_list(
    ui: &mut egui::Ui,
    model: &ProjectModel,
    linked: &[usize],
    state: &mut crate::gui::app::KitsState,
    can_capture: bool,
    request_capture: &mut Option<usize>,
) {
    widgets::section_label(ui, "KITS");
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        for (filter, label) in [
            (KitFilter::InUse, "In use".to_owned()),
            (KitFilter::All, format!("All {}", model.kits.len())),
            (KitFilter::Unused, "Unused".to_owned()),
        ] {
            if ui.selectable_label(state.filter == filter, label).clicked() {
                state.filter = filter;
            }
        }
    });
    ui.add(
        egui::TextEdit::singleline(&mut state.search)
            .hint_text("Search kits…")
            .desired_width(f32::INFINITY),
    );
    ui.add_space(theme::GAP_S);

    let query = state.search.to_lowercase();
    let rows: Vec<&KitModel> = model
        .kits
        .iter()
        .filter(|kit| match state.filter {
            KitFilter::InUse => linked.contains(&kit.index),
            KitFilter::All => true,
            KitFilter::Unused => !linked.contains(&kit.index),
        })
        .filter(|kit| kit_matches(kit, model, &query))
        .collect();

    ScrollArea::vertical()
        .id_salt("kit-list-scroll")
        .show(ui, |ui| {
            if rows.is_empty() {
                ui.label(RichText::new("No kits match.").color(theme::TEXT_MUTED));
            }
            for kit in rows {
                let selected = state.selected_kit == Some(kit.index);
                let patterns = kit_pattern_names(model, kit.index);
                let row = widgets::list_row(ui, selected, theme::KIT_ROW_HEIGHT, |ui| {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("{:02}", kit.index + 1))
                                    .color(theme::TEXT_MUTED)
                                    .monospace(),
                            );
                            ui.add(
                                egui::Label::new(
                                    RichText::new(
                                        names::kit_display_name(&kit.name, kit.index).text(),
                                    )
                                    .color(theme::TEXT_PRIMARY)
                                    .strong(),
                                )
                                .truncate(),
                            );
                        });
                        if !patterns.is_empty() {
                            ui.label(
                                RichText::new(pattern_summary(&patterns, 4))
                                    .color(theme::TEXT_MUTED)
                                    .size(theme::FONT_META),
                            );
                        }
                    });
                });
                row.context_menu(|ui| {
                    if ui
                        .add_enabled(can_capture, egui::Button::new("Copy kit"))
                        .on_hover_text(
                            "Copies this kit (with its sample references) to the rytmdash \
                             clipboard — paste it into any project.",
                        )
                        .on_disabled_hover_text("Connect the Rytm first")
                        .clicked()
                    {
                        *request_capture = Some(kit.index);
                        ui.close();
                    }
                });
                if row.clicked() {
                    state.selected_kit = Some(kit.index);
                    state.selected_track = None;
                }
            }
        });
}

/// Search matches kit name, slot number, or a linked pattern name like `B03`.
fn kit_matches(kit: &KitModel, model: &ProjectModel, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    kit.name.to_lowercase().contains(query)
        || format!("{:02}", kit.index + 1).contains(query)
        || kit_pattern_names(model, kit.index)
            .iter()
            .any(|p| p.to_lowercase().contains(query))
}

fn kit_pattern_names(model: &ProjectModel, kit_index: usize) -> Vec<String> {
    model
        .patterns
        .iter()
        .filter(|p| p.kit_index == kit_index)
        .map(|p| pattern_display_name(p.index))
        .collect()
}

/// `B01 · B02 · B03 +2` — first few pattern badges, then a count.
fn pattern_summary(patterns: &[String], max: usize) -> String {
    let shown = patterns.iter().take(max).cloned().collect::<Vec<_>>();
    let mut text = shown.join(" · ");
    if patterns.len() > max {
        text.push_str(&format!(" +{}", patterns.len() - max));
    }
    text
}

// ---- Kit header + mode switch --------------------------------------------

/// `Kit 02 · DEUIL B` with its linked-pattern summary (PRD §12).
fn kit_header(ui: &mut egui::Ui, model: &ProjectModel, kit: &KitModel) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("Kit {:02}", kit.index + 1))
                .color(theme::TEXT_MUTED)
                .size(theme::FONT_SECTION),
        );
        ui.label(
            RichText::new(names::kit_display_name(&kit.name, kit.index).text())
                .size(theme::FONT_SECTION + 2.0)
                .color(theme::TEXT_PRIMARY)
                .strong(),
        );
    });
    let patterns = kit_pattern_names(model, kit.index);
    ui.label(
        RichText::new(if patterns.is_empty() {
            "Not used by any pattern".to_owned()
        } else {
            format!("Used by {}", pattern_summary(&patterns, 6))
        })
        .color(theme::TEXT_SECONDARY)
        .size(theme::FONT_META),
    );
}

/// Tracks / Scenes / Performance segmented control (PRD §13). Scenes and
/// Performance read support arrives with a later milestone; the segments are
/// disabled with an explanation rather than hidden, for discoverability.
fn mode_switch(ui: &mut egui::Ui, state: &mut crate::gui::app::KitsState) {
    ui.horizontal(|ui| {
        let _ = ui.selectable_label(true, "Tracks");
        ui.add_enabled(false, egui::Button::selectable(false, "Scenes"))
            .on_disabled_hover_text("Scene visualization arrives with a later milestone.");
        ui.add_enabled(false, egui::Button::selectable(false, "Performance"))
            .on_disabled_hover_text("Performance visualization arrives with a later milestone.");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if state.selected_track.is_none() {
                let label = if state.rail_hidden {
                    "Show sample slots"
                } else {
                    "Hide sample slots"
                };
                if ui.button(label).clicked() {
                    state.rail_hidden = !state.rail_hidden;
                }
            }
        });
    });
}

// ---- Pad grid -------------------------------------------------------------

/// The 12 pads, laid out like the hardware: 3 rows of 4, BD row at the bottom.
fn pad_grid(
    ui: &mut egui::Ui,
    model: &ProjectModel,
    kit: &KitModel,
    state: &mut crate::gui::app::KitsState,
    browsing: bool,
    request_pad_assign: &mut Option<(usize, usize)>,
) -> Option<PendingAssign> {
    let mut dropped = None;
    let card_width = ((ui.available_width() - 3.0 * theme::PAD_GAP) / 4.0)
        .clamp(theme::PAD_MIN_WIDTH, theme::PAD_MAX_WIDTH);
    // Display rows top→bottom = CH-row, BT-row, BD-row (mirrors the device).
    for row in [[8, 9, 10, 11], [4, 5, 6, 7], [0, 1, 2, 3]] {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = theme::PAD_GAP;
            for track in row {
                if let Some(assign) = pad_card(
                    ui,
                    model,
                    kit,
                    track,
                    card_width,
                    state,
                    browsing,
                    request_pad_assign,
                ) {
                    dropped = Some(assign);
                }
            }
        });
        ui.add_space(theme::PAD_GAP);
    }
    if state.traced_slot.is_some() {
        ui.label(
            RichText::new("Tracing sample usage — press Esc or click the pill again to exit.")
                .color(theme::SAMPLE_PURPLE)
                .size(theme::FONT_META),
        );
    }
    dropped
}

/// One pad card (PRD §14.2): track label, Sound name, machine, sample row.
/// Click selects the track; a slot drop returns a pending assignment;
/// right-click offers assigning a new +Drive sample.
#[allow(clippy::too_many_arguments)]
fn pad_card(
    ui: &mut egui::Ui,
    model: &ProjectModel,
    kit: &KitModel,
    track: usize,
    width: f32,
    state: &mut crate::gui::app::KitsState,
    browsing: bool,
    request_pad_assign: &mut Option<(usize, usize)>,
) -> Option<PendingAssign> {
    let sound = kit.sounds.get(track)?;
    let color = theme::TRACK_COLORS[track];
    let traced = state.traced_slot.is_some() && sound.sample_slot == state.traced_slot;
    let selected = state.selected_track == Some(track);

    let stroke = if traced {
        Stroke::new(2.0, theme::SAMPLE_PURPLE)
    } else if selected {
        Stroke::new(1.5, theme::TEXT_SECONDARY)
    } else {
        Stroke::new(1.0, theme::BORDER)
    };
    let frame = Frame::new()
        .fill(if selected {
            theme::SURFACE_ELEVATED
        } else {
            theme::SURFACE
        })
        .stroke(stroke)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(10));

    let mut toggle_trace = None;
    let (zone_response, payload) = ui.dnd_drop_zone::<SlotDrag, ()>(frame, |ui| {
        ui.set_min_size(Vec2::new(width, theme::PAD_HEIGHT));
        ui.set_max_width(width);
        ui.vertical(|ui| {
            ui.label(
                RichText::new(names::track_label(track))
                    .color(color)
                    .size(theme::FONT_BODY)
                    .strong(),
            );
            ui.add_space(2.0);
            match names::sound_display_name(&sound.name, track) {
                ResolvedName::Native(name) => {
                    ui.label(
                        RichText::new(name)
                            .size(theme::FONT_CARD_PRIMARY)
                            .color(theme::TEXT_PRIMARY)
                            .strong(),
                    );
                }
                fallback => {
                    ui.label(
                        RichText::new(fallback.text())
                            .size(theme::FONT_CARD_PRIMARY)
                            .color(theme::TEXT_SECONDARY),
                    );
                }
            }
            ui.label(
                RichText::new(&sound.machine)
                    .color(theme::TEXT_SECONDARY)
                    .size(theme::FONT_META),
            );
            ui.add_space(4.0);
            match sound.sample_slot {
                Some(slot) => {
                    let label = match names::sample_display_name(model.sample_name(slot)) {
                        ResolvedName::Native(name) => format!("{slot:02} · {name}"),
                        _ => format!("{slot:02} · name unavailable"),
                    };
                    let pill = widgets::sample_pill(ui, &label, traced)
                        .on_hover_text("Project sample slot — click to trace usage");
                    if pill.clicked() {
                        toggle_trace = Some(slot);
                    }
                }
                None => {
                    ui.label(
                        RichText::new("No sample")
                            .color(theme::TEXT_MUTED)
                            .size(theme::FONT_META),
                    );
                }
            }
        });
    });

    let card = zone_response
        .response
        .interact(Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    card.context_menu(|ui| {
        if ui
            .add_enabled(!browsing, egui::Button::new("Assign sample from +Drive…"))
            .on_hover_text(
                "Opens the +Drive browser; the chosen sample is loaded into a free project \
                 slot and then assigned to this pad.",
            )
            .on_disabled_hover_text(
                "Snapshots are read-only — load this project on the device to edit it",
            )
            .clicked()
        {
            *request_pad_assign = Some((kit.index, track));
            ui.close();
        }
    });
    if let Some(slot) = toggle_trace {
        state.traced_slot = (state.traced_slot != Some(slot)).then_some(slot);
    } else if card.clicked() {
        state.selected_track = (state.selected_track != Some(track)).then_some(track);
    }

    payload.map(|drag| PendingAssign {
        kit_index: kit.index,
        track,
        slot: drag.slot,
    })
}

// ---- Inspector ------------------------------------------------------------

/// Track inspector (PRD §15): sample, sound and usage groups.
fn track_inspector(
    ui: &mut egui::Ui,
    model: &ProjectModel,
    state: &mut crate::gui::app::KitsState,
    browsing: bool,
    request_pad_assign: &mut Option<(usize, usize)>,
) {
    let Some(kit) = state.selected_kit.and_then(|k| model.kits.get(k)) else {
        return;
    };
    let Some(track) = state.selected_track else {
        return;
    };
    let Some(sound) = kit.sounds.get(track) else {
        return;
    };

    ui.horizontal(|ui| {
        ui.label(
            RichText::new(names::track_label(track))
                .color(theme::TRACK_COLORS[track])
                .size(theme::FONT_SECTION)
                .strong(),
        );
        ui.label(
            RichText::new(names::sound_display_name(&sound.name, track).text())
                .size(theme::FONT_SECTION)
                .color(theme::TEXT_PRIMARY)
                .strong(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("×").on_hover_text("Close inspector").clicked() {
                state.selected_track = None;
            }
        });
    });
    ui.add_space(theme::GAP_M);

    widgets::inspector_group(ui, "SAMPLE", |ui| match sound.sample_slot {
        Some(slot) => {
            widgets::detail_line(ui, "Slot", &format!("{slot:02}"));
            let info = model.samples.iter().find(|s| s.slot == slot);
            match info {
                Some(info) => {
                    widgets::detail_line(ui, "File", &info.name);
                    widgets::detail_line(ui, "Size", &format!("{} KB", info.size / 1024));
                    ui.label(
                        RichText::new(&info.path)
                            .color(theme::TEXT_MUTED)
                            .size(theme::FONT_META),
                    )
                    .on_hover_text("+Drive path");
                }
                None => {
                    widgets::detail_line(ui, "File", "Name unavailable");
                }
            }
            let uses = model.slot_usage().get(&slot).map_or(0, Vec::len);
            widgets::detail_line(
                ui,
                "Usage",
                &format!("{uses} kit-track use{}", if uses == 1 { "" } else { "s" }),
            );
            let tracing = state.traced_slot == Some(slot);
            if ui
                .selectable_label(tracing, if tracing { "Tracing…" } else { "Trace usage" })
                .clicked()
            {
                state.traced_slot = (!tracing).then_some(slot);
            }
            ui.add_space(4.0);
            if ui
                .add_enabled(!browsing, egui::Button::new("Replace from +Drive…"))
                .on_hover_text(
                    "Pick any +Drive sample: it loads into a free project slot and this track \
                     is re-pointed at it (the current slot stays loaded).",
                )
                .on_disabled_hover_text("Snapshots are read-only")
                .clicked()
            {
                *request_pad_assign = Some((kit.index, track));
            }
        }
        None => {
            ui.label(RichText::new("No sample assigned").color(theme::TEXT_MUTED));
            ui.add_space(4.0);
            if ui
                .add_enabled(!browsing, egui::Button::new("Assign sample from +Drive…"))
                .on_hover_text(
                    "Pick any +Drive sample: it loads into a free project slot and gets \
                     assigned to this track.",
                )
                .on_disabled_hover_text("Snapshots are read-only")
                .clicked()
            {
                *request_pad_assign = Some((kit.index, track));
            }
        }
    });

    widgets::inspector_group(ui, "SOUND", |ui| {
        widgets::detail_line(ui, "Machine", &sound.machine);
        match names::sound_display_name(&sound.name, track) {
            ResolvedName::Native(name) => widgets::detail_line(ui, "Name", &name),
            _ => widgets::detail_line(ui, "Name", "Unnamed"),
        }
        widgets::detail_line(ui, "Source", "Kit track");
    });

    if let Some(slot) = sound.sample_slot {
        widgets::inspector_group(ui, "USED BY", |ui| {
            usage_list(ui, model, slot);
        });
    }
}

/// All known references to a sample slot, in plain language: kit tracks,
/// Sound Pool sounds (via Sound Locks), and trig sample plocks (PRD §3.6).
fn usage_list(ui: &mut egui::Ui, model: &ProjectModel, slot: u8) {
    let refs = model.slot_references(slot);
    if refs.is_empty() {
        ui.label(RichText::new("No known references").color(theme::TEXT_MUTED));
    }
    for u in &refs.kit_uses {
        ui.label(format!(
            "Kit {:02} {} · {}",
            u.kit_index + 1,
            names::kit_display_name(&u.kit_name, u.kit_index).text(),
            names::track_label(u.track),
        ));
    }
    for p in &refs.pool_uses {
        ui.label(format!(
            "Pool sound {:03} {}",
            p.pool_index + 1,
            p.sound_name.trim()
        ));
    }
    for index in &refs.plock_patterns {
        ui.label(format!(
            "Pattern {} · trig sample plock",
            pattern_display_name(*index)
        ));
    }
    if !model.dependency_scan {
        ui.add_space(4.0);
        ui.label(
            RichText::new("Old cache — re-sync to index Sound Lock and plock uses.")
                .color(theme::TEXT_MUTED)
                .size(theme::FONT_META),
        );
    }
}

/// Default inspector content: every used sample slot as a draggable chip
/// (drag onto a pad to re-point that track's Sound).
fn sample_rail(ui: &mut egui::Ui, model: &ProjectModel, state: &mut crate::gui::app::KitsState) {
    widgets::section_label(ui, "SAMPLE SLOTS");
    let usage = model.slot_usage();
    ui.label(
        RichText::new(format!(
            "{} in use · click to trace, drag onto a pad to assign",
            usage.len()
        ))
        .color(theme::TEXT_MUTED)
        .size(theme::FONT_META),
    );
    ui.add_space(theme::GAP_S);

    ScrollArea::vertical()
        .id_salt("sample-rail-scroll")
        .show(ui, |ui| {
            if usage.is_empty() {
                ui.label(
                    RichText::new("No kit references any sample slot yet.")
                        .color(theme::TEXT_MUTED),
                );
            }
            for (slot, uses) in &usage {
                let traced = state.traced_slot == Some(*slot);
                ui.horizontal(|ui| {
                    widgets::drag_handle(ui);
                    let label = match names::sample_display_name(model.sample_name(*slot)) {
                        ResolvedName::Native(name) => format!("{slot:02} · {name}"),
                        _ => format!("{slot:02} · name unavailable"),
                    };
                    let id = Id::new(("rail-slot", *slot));
                    let response = ui
                        .dnd_drag_source(id, SlotDrag { slot: *slot }, |ui| {
                            let _ = widgets::sample_pill(ui, &label, traced);
                        })
                        .response
                        .on_hover_cursor(egui::CursorIcon::Grab)
                        .on_hover_text("Drag onto a pad to assign this slot");
                    if response.clicked() {
                        state.traced_slot = (!traced).then_some(*slot);
                    }
                    ui.label(
                        RichText::new(format!(
                            "{} use{}",
                            uses.len(),
                            if uses.len() == 1 { "" } else { "s" }
                        ))
                        .color(theme::TEXT_MUTED)
                        .size(theme::FONT_META),
                    );
                });
            }
        });
}
