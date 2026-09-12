//! Patterns page (PRD §26): the 8-bank × 16-pattern map with kit links, the
//! cross-project transfer clipboard, and the paste-plan modal. Pattern
//! sequence editing stays out of scope.

use eframe::egui::{self, CornerRadius, Frame, Margin, RichText, ScrollArea, Sense, Stroke};

use crate::gui::app::{KitsmithApp, ProjectTab};
use crate::gui::{theme, widgets};
use crate::model::names;
use crate::model::{pattern_display_name, ProjectModel};
use crate::ops::Plan;
use crate::transfer;

pub fn show(app: &mut KitsmithApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    let connected = app.connected_in_port();
    let capturing = app.capture.is_running();
    let mut jump_kit = None;
    let mut request_capture: Option<usize> = None;
    let mut open_paste = false;
    let mut clear_clipboard = false;

    let browsing = app.is_browsing_snapshot();
    let project_sel = app.ui_state.project;
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
        let clipboard = &app.clipboard;
        let selected = &mut app.ui_state.selected_pattern;

        egui::Panel::right("pattern-inspector")
            .default_size(theme::INSPECTOR_WIDTH)
            .resizable(false)
            .frame(Frame::new().inner_margin(Margin::symmetric(10, 4)))
            .show(ui, |ui| {
                if let Some(index) = *selected {
                    if let Some(kit) = pattern_inspector(ui, model, index, selected) {
                        jump_kit = Some(kit);
                    }
                } else {
                    widgets::section_label(ui, "PATTERN DETAILS");
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(
                            "Select a pattern to see which kit it uses. Right-click one to copy \
                             it into another project.",
                        )
                        .color(theme::TEXT_MUTED),
                    );
                }
            });

        egui::CentralPanel::default()
            .frame(Frame::new().inner_margin(Margin::symmetric(0, 4)))
            .show(ui, |ui| {
                if !browsing {
                    if let Some(clip) = clipboard {
                        clipboard_banner(ui, clip, &mut open_paste, &mut clear_clipboard);
                        ui.add_space(theme::GAP_S);
                    }
                }
                ui.label(
                    RichText::new(format!("{} patterns link to a kit", model.patterns.len()))
                        .color(theme::TEXT_SECONDARY),
                );
                ui.add_space(theme::GAP_S);
                if browsing {
                    ui.label(
                        RichText::new(
                            "Snapshot view — copying patterns out of backups is not supported \
                             yet (kits are: use the Kits tab).",
                        )
                        .color(theme::TEXT_MUTED)
                        .size(theme::FONT_META),
                    );
                } else {
                    ui.label(
                        RichText::new(
                            "Right-click a pattern to copy it — then paste it into any project.",
                        )
                        .color(theme::TEXT_MUTED)
                        .size(theme::FONT_META),
                    );
                }
                ScrollArea::vertical().show(ui, |ui| {
                    pattern_map(
                        ui,
                        model,
                        selected,
                        !browsing && connected.is_some() && !capturing,
                        &mut request_capture,
                    );
                });
            });
    }

    if let Some(kit_index) = jump_kit {
        app.ui_state.tab = ProjectTab::Kits;
        app.ui_state.kits.selected_kit = Some(kit_index);
        app.ui_state.kits.selected_track = None;
    }
    if let Some(index) = request_capture {
        if let (Some(port), Some(model)) = (connected.clone(), app.model.clone()) {
            let repaint = ctx.clone();
            app.capture.start(
                port,
                model,
                crate::gui::transfer_ctl::CaptureJob::Pattern(index),
                move || repaint.request_repaint(),
            );
        }
    }
    if open_paste {
        app.ui_state.paste_modal_open = true;
    }
    if clear_clipboard {
        app.clipboard = None;
        transfer::clear_saved();
    }
    paste_modal(app, &ctx);
}

/// The transfer clipboard banner: what is captured, paste and clear actions.
fn clipboard_banner(
    ui: &mut egui::Ui,
    clip: &transfer::PatternClipboard,
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
                widgets::badge(ui, "CLIPBOARD", theme::SAMPLE_PURPLE);
                ui.label(
                    RichText::new(&clip.source_hint)
                        .color(theme::TEXT_PRIMARY)
                        .strong(),
                );
                ui.label(
                    RichText::new(format!(
                        "captured {}",
                        clip_age_label(clip.captured_at_unix)
                    ))
                    .color(theme::TEXT_MUTED)
                    .size(theme::FONT_META),
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

fn clip_age_label(captured_at_unix: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let secs = now.saturating_sub(captured_at_unix);
    match secs {
        0..=59 => "just now".to_owned(),
        60..=3599 => format!("{}m ago", secs / 60),
        3600..=86_399 => format!("{}h ago", secs / 3600),
        _ => format!("{}d ago", secs / 86_400),
    }
}

/// The paste-plan modal (PRD §30–§31): remaps, missing samples, overwrite
/// warnings — then one named action through the backup-first pipeline.
fn paste_modal(app: &mut KitsmithApp, ctx: &egui::Context) {
    if !app.ui_state.paste_modal_open {
        return;
    }
    let (Some(clip), Some(model)) = (&app.clipboard, app.model.as_ref()) else {
        app.ui_state.paste_modal_open = false;
        return;
    };
    let preview = transfer::plan_paste(clip, model);
    let connected = app.connected_in_port();
    let mut close = false;
    let mut confirm = false;
    let mut launch: Option<Plan> = None;

    egui::Window::new(format!(
        "Copy {} into this project?",
        preview.overwrite_pattern
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
            "Pattern {} and {} will be replaced in this project.",
            preview.overwrite_pattern, preview.overwrite_kit
        ));
        if !preview.kit_also_used_by.is_empty() {
            ui.label(
                RichText::new(format!(
                    "Warning: {} also use kit {:02} — replacing it changes them too.",
                    preview.kit_also_used_by.join(" · "),
                    clip.kit_index + 1
                ))
                .color(theme::WARNING),
            );
        }
        if !preview.overwrite_pool.is_empty() {
            ui.label(format!(
                "Sound Pool slot(s) {} will be replaced.",
                preview
                    .overwrite_pool
                    .iter()
                    .map(|i| format!("{:03}", i + 1))
                    .collect::<Vec<_>>()
                    .join(" · ")
            ));
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
                "Copy {} into this project",
                preview.overwrite_pattern
            ));
            if ui
                .add_enabled(connected.is_some(), button)
                .on_disabled_hover_text("Connect the Rytm first")
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
        // rytm-rs can panic on unsupported object contents; surface that as
        // an error instead of crashing the app.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            transfer::materialize(clip, &preview)
        }))
        .unwrap_or_else(|_| Err("internal error while re-encoding the captured objects".into()));
        match result {
            Ok(paste) => launch = Some(Plan::paste_pattern(&paste)),
            Err(e) => {
                app.ui_state.paste_modal_open = false;
                app.toast(format!("Paste failed before writing anything: {e}"));
                return;
            }
        }
    }

    if let (Some(plan), Some(port)) = (launch, connected) {
        app.ui_state.paste_modal_open = false;
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
        app.ui_state.paste_modal_open = false;
    }
}

/// 8 banks (A–H) × 16 patterns; cells with a synced link show their kit.
/// Right-click a cell for transfer actions (PRD §36).
fn pattern_map(
    ui: &mut egui::Ui,
    model: &ProjectModel,
    selected: &mut Option<usize>,
    can_capture: bool,
    request_capture: &mut Option<usize>,
) {
    for bank in 0..8 {
        widgets::section_label(ui, &format!("BANK {}", (b'A' + bank as u8) as char));
        ui.add_space(2.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
            for pos in 0..16 {
                let index = bank * 16 + pos;
                pattern_cell(ui, model, index, selected, can_capture, request_capture);
            }
        });
        ui.add_space(theme::GAP_M);
    }
}

fn pattern_cell(
    ui: &mut egui::Ui,
    model: &ProjectModel,
    index: usize,
    selected: &mut Option<usize>,
    can_capture: bool,
    request_capture: &mut Option<usize>,
) {
    let link = model.patterns.iter().find(|p| p.index == index);
    let is_selected = *selected == Some(index);
    let (fill, stroke) = if is_selected {
        (
            theme::SURFACE_ELEVATED,
            Stroke::new(1.5, theme::TEXT_SECONDARY),
        )
    } else if link.is_some() {
        (theme::SURFACE, Stroke::new(1.0, theme::BORDER))
    } else {
        (theme::BG_APP, Stroke::new(1.0, theme::BORDER))
    };
    let response = Frame::new()
        .fill(fill)
        .stroke(stroke)
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.set_min_width(64.0);
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(pattern_display_name(index))
                        .color(if link.is_some() {
                            theme::TEXT_PRIMARY
                        } else {
                            theme::TEXT_MUTED
                        })
                        .strong(),
                );
                match link {
                    Some(p) => {
                        ui.label(
                            RichText::new(format!("Kit {:02}", p.kit_index + 1))
                                .color(theme::TEXT_SECONDARY)
                                .size(theme::FONT_META),
                        );
                    }
                    None => {
                        ui.label(
                            RichText::new("—")
                                .color(theme::TEXT_MUTED)
                                .size(theme::FONT_META),
                        );
                    }
                }
            });
        })
        .response
        .interact(Sense::click());
    let response = if link.is_some() {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    };
    if link.is_some() {
        response.context_menu(|ui| {
            if ui
                .add_enabled(can_capture, egui::Button::new("Copy pattern"))
                .on_hover_text(
                    "Copies this pattern, its kit and Sound-locked pool sounds to the kitsmith \
                     clipboard — paste it into any project.",
                )
                .on_disabled_hover_text("Connect the Rytm first")
                .clicked()
            {
                *request_capture = Some(index);
                ui.close();
            }
        });
    }
    if response.clicked() && link.is_some() {
        *selected = (!is_selected).then_some(index);
    }
}

/// Inspector for one pattern. Returns the kit index when "Show in Kits" is
/// clicked.
fn pattern_inspector(
    ui: &mut egui::Ui,
    model: &ProjectModel,
    index: usize,
    selected: &mut Option<usize>,
) -> Option<usize> {
    let mut jump = None;
    ui.horizontal(|ui| {
        widgets::section_label(ui, &format!("PATTERN {}", pattern_display_name(index)));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("×").on_hover_text("Close").clicked() {
                *selected = None;
            }
        });
    });
    ui.add_space(theme::GAP_S);

    widgets::inspector_group(ui, "LINKED KIT", |ui| {
        match model
            .patterns
            .iter()
            .find(|p| p.index == index)
            .and_then(|p| model.kits.get(p.kit_index))
        {
            Some(kit) => {
                widgets::detail_line(ui, "Kit", &format!("{:02}", kit.index + 1));
                widgets::detail_line(
                    ui,
                    "Name",
                    names::kit_display_name(&kit.name, kit.index).text(),
                );
                if ui.link("Show in Kits").clicked() {
                    jump = Some(kit.index);
                }
            }
            None => {
                ui.label(RichText::new("Kit data unavailable").color(theme::TEXT_MUTED));
            }
        }
    });

    widgets::inspector_group(ui, "DEPENDENCIES", |ui| {
        let Some(pattern) = model.patterns.iter().find(|p| p.index == index) else {
            ui.label(RichText::new("Pattern data unavailable").color(theme::TEXT_MUTED));
            return;
        };
        if !model.dependency_scan {
            ui.label(
                RichText::new("Old cache — re-sync to index Sound Locks and sample plocks.")
                    .color(theme::TEXT_MUTED)
                    .size(theme::FONT_META),
            );
            return;
        }
        if pattern.sound_locks.is_empty() && pattern.plocked_sample_slots.is_empty() {
            ui.label(RichText::new("No Sound Locks or sample plocks.").color(theme::TEXT_MUTED));
        }
        for pool_index in &pattern.sound_locks {
            let name = model
                .pool_sounds
                .get(usize::from(*pool_index))
                .map(|s| s.name.trim())
                .filter(|n| !n.is_empty())
                .map_or_else(
                    || format!("Pool Sound {:03}", pool_index + 1),
                    str::to_owned,
                );
            ui.label(format!(
                "Sound-locks pool sound {:03} {name}",
                pool_index + 1
            ));
        }
        for slot in &pattern.plocked_sample_slots {
            let name = match crate::model::names::sample_display_name(model.sample_name(*slot)) {
                crate::model::names::ResolvedName::Native(name) => name,
                _ => "name unavailable".to_owned(),
            };
            ui.label(format!("Sample plock on slot {slot:02} · {name}"));
        }
    });
    jump
}
