//! The application shell: top app bar, left navigation sidebar, workspace
//! router, toasts and the plan-execution modal (PRD v2 Part II).

use std::time::Instant;

use eframe::egui::{self, Align2, CornerRadius, Frame, Margin, RichText, Stroke};

use std::collections::HashMap;

use crate::cache::{self, ProjectManifest};
use crate::device::detect::DeviceState;
use crate::device::objects::MissedObject;
use crate::device::projfile::{self, RawProjectSnapshot};
use crate::device::watcher::DeviceWatcher;
use crate::gui::backup::BackupController;
use crate::gui::drive_ctl::DriveBrowser;
use crate::gui::execute::ExecutePanel;
use crate::gui::fileops::{FileOpController, FileOpOutcome};
use crate::gui::import::ImportPanel;
use crate::gui::sync::SyncController;
use crate::gui::transfer_ctl::{CaptureController, Captured};
use crate::gui::views::activity::{ActivityEntry, ActivityKind};
use crate::gui::{project_modals, sidebar, theme, top_bar, views};
use crate::localmeta::{self, LocalMeta};
use crate::model::ProjectModel;
use crate::transfer::{self, KitClipboard, PatternClipboard};

/// Top-level navigation destinations (PRD §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Nav {
    #[default]
    Projects,
    Drive,
    Import,
    Activity,
    Diagnostics,
}

/// Tabs inside the project workspace (PRD §9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProjectTab {
    #[default]
    Kits,
    Samples,
    Patterns,
    SoundPool,
    Overview,
}

/// What the sidebar project browser points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProjectSelection {
    /// The synced active project (the only one with indexed contents today).
    #[default]
    Active,
    /// A +Drive project slot; contents are not indexed yet (WP11).
    Slot(u32),
}

/// Kit list filter (PRD §11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KitFilter {
    #[default]
    InUse,
    All,
    Unused,
}

/// UI state for the Kits workspace.
#[derive(Default)]
pub struct KitsState {
    pub filter: KitFilter,
    pub search: String,
    pub selected_kit: Option<usize>,
    /// Track selected in the pad grid — opens the inspector.
    pub selected_track: Option<usize>,
    /// Sample slot being traced — pads using it get a purple ring.
    pub traced_slot: Option<u8>,
    /// Hides the sample-slot rail when no track inspector is open.
    pub rail_hidden: bool,
    /// The in-progress kit edits ("Edit kit" mode). `None` = read-only.
    pub edit_draft: Option<crate::model::edit::KitEditDraft>,
    /// The save-plan preview modal for the current draft.
    pub save_modal_open: bool,
    /// The confirm dialog for discarding a non-empty draft.
    pub discard_confirm_open: bool,
    /// A draft whose save plan is executing; applied to the local model when
    /// the plan's read-back verification succeeds, restored to the editor
    /// when it fails.
    pub pending_save: Option<crate::model::edit::KitEditDraft>,
}

/// All per-frame-persistent UI state, separate from device/model state.
#[derive(Default)]
pub struct UiState {
    pub nav: Nav,
    pub project: ProjectSelection,
    pub project_search: String,
    pub tab: ProjectTab,
    pub kits: KitsState,
    pub selected_sample_slot: Option<u8>,
    pub selected_pattern: Option<usize>,
    /// Collapses the whole left sidebar (toggled from the top bar).
    pub sidebar_hidden: bool,
    /// In-progress local-label edit text and which slot it belongs to.
    pub alias_draft: String,
    pub alias_draft_slot: Option<u32>,
    /// The paste-preview modal for a cross-project pattern transfer.
    pub paste_modal_open: bool,
    /// The paste-preview modal for a cross-project kit transfer.
    pub kit_paste_modal_open: bool,
    /// Rename-on-device modal: the slot being renamed + the draft name.
    pub rename_modal_slot: Option<u32>,
    pub rename_draft: String,
    /// Copy-project modal: (source slot, chosen destination slot).
    pub copy_modal: Option<(u32, u32)>,
    /// Load-sample modal: the +Drive path + the chosen project slot.
    pub load_sample_modal: Option<String>,
    pub load_sample_slot: u8,
    /// Current +Drive browser directory.
    pub drive_path: String,
    /// Pad waiting for a +Drive sample: `(kit_index, track)`. Set from the
    /// pad's context menu; consumed after the loaded sample is assigned.
    pub pending_pad_assign: Option<(usize, usize)>,
    /// Sync-would-discard-edits confirmation: the number of unsaved edits.
    pub sync_confirm: Option<usize>,
    /// Sidebar "Rearrange" mode: shows empty slots and enables project drag.
    pub rearrange: bool,
}

/// When the last sync finished and how complete it was.
#[derive(Clone, Copy)]
pub struct SyncInfo {
    pub at: Instant,
    pub missed: usize,
}

pub struct KitsmithApp {
    pub(crate) watcher: DeviceWatcher,
    pub(crate) sync: SyncController,
    pub(crate) import: ImportPanel,
    pub(crate) execute: ExecutePanel,
    pub(crate) model: Option<ProjectModel>,
    pub(crate) local_meta: LocalMeta,
    pub(crate) backup: BackupController,
    pub(crate) manifest: ProjectManifest,
    pub(crate) capture: CaptureController,
    pub(crate) fileops: FileOpController,
    pub(crate) drive: DriveBrowser,
    pub(crate) clipboard: Option<PatternClipboard>,
    pub(crate) kit_clipboard: Option<KitClipboard>,
    /// Parsed project snapshots from raw backups (`None` = unparseable),
    /// loaded lazily per slot.
    pub(crate) raw_snapshots: HashMap<u32, Option<RawProjectSnapshot>>,
    /// Full dashboard models decoded from raw backups, lazily per slot.
    pub(crate) raw_models: HashMap<u32, Option<ProjectModel>>,
    pub(crate) ui_state: UiState,
    pub(crate) last_sync: Option<SyncInfo>,
    /// Objects the last sync could not read (for Diagnostics).
    pub(crate) sync_misses: Vec<MissedObject>,
    /// Which backed-up slot the active project most likely is (heuristic
    /// match — the device does not report the loaded project).
    pub(crate) active_guess: Option<u32>,
    pub(crate) activity: Vec<ActivityEntry>,
    toasts: Vec<(String, Instant)>,
    /// The device port the last frame saw, to notice reconnects.
    seen_in_port: Option<String>,
}

impl KitsmithApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);
        let ctx = cc.egui_ctx.clone();
        let mut app = Self {
            watcher: DeviceWatcher::spawn(move || ctx.request_repaint()),
            sync: SyncController::default(),
            import: ImportPanel::default(),
            execute: ExecutePanel::default(),
            model: cache::load(),
            local_meta: localmeta::load(),
            backup: BackupController::default(),
            manifest: cache::load_project_manifest(&cache::default_dir()),
            capture: CaptureController::default(),
            fileops: FileOpController::default(),
            drive: DriveBrowser::default(),
            clipboard: transfer::load(),
            kit_clipboard: transfer::load_kit(),
            raw_snapshots: HashMap::new(),
            raw_models: HashMap::new(),
            ui_state: UiState::default(),
            last_sync: None,
            sync_misses: Vec::new(),
            active_guess: None,
            activity: Vec::new(),
            toasts: Vec::new(),
            seen_in_port: None,
        };
        app.identify_active_project();
        app
    }

    pub(crate) fn log_activity(
        &mut self,
        kind: ActivityKind,
        title: String,
        detail: Option<String>,
    ) {
        self.activity.push(ActivityEntry {
            kind,
            title,
            detail,
            at: Instant::now(),
        });
    }

    pub(crate) fn toast(&mut self, text: String) {
        self.toasts.push((text, Instant::now()));
    }

    /// Drops any open kit-edit draft, telling the user when real work is
    /// lost — a draft must never vanish silently (PRD P8).
    fn discard_kit_draft(&mut self, reason: &str) {
        let had_edits = self
            .ui_state
            .kits
            .edit_draft
            .as_ref()
            .is_some_and(|d| !d.is_empty());
        self.ui_state.kits.edit_draft = None;
        self.ui_state.kits.save_modal_open = false;
        self.ui_state.kits.discard_confirm_open = false;
        if had_edits {
            self.toast(reason.to_owned());
        }
    }

    /// Persists the local metadata store, reporting failure as a toast.
    pub(crate) fn save_local_meta(&mut self) {
        if let Err(e) = localmeta::save(&self.local_meta) {
            self.toast(format!("Could not save local labels: {e}"));
        }
    }

    /// Ensures the parsed snapshot for a backed-up project slot is cached
    /// (reads + parses the backup file on first access).
    pub(crate) fn ensure_raw_snapshot(&mut self, slot: u32) {
        if self.raw_snapshots.contains_key(&slot) {
            return;
        }
        let snapshot = cache::load_project_backup(&cache::default_dir(), slot)
            .and_then(|bytes| projfile::parse_raw_project(&bytes));
        self.raw_snapshots.insert(slot, snapshot);
    }

    /// Ensures the full dashboard model for a backed-up slot is decoded and
    /// cached (kit + pattern records through libanalogrytm).
    pub(crate) fn ensure_raw_model(&mut self, slot: u32) {
        if self.raw_models.contains_key(&slot) {
            return;
        }
        let known: Vec<crate::model::SampleSlotInfo> = self
            .model
            .as_ref()
            .map(|m| m.samples.clone())
            .unwrap_or_default();
        let model = cache::load_project_backup(&cache::default_dir(), slot).and_then(|bytes| {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                projfile::to_project_model(&bytes, &known).ok()
            }))
            .unwrap_or(None)
        });
        self.raw_models.insert(slot, model);
    }

    /// True when the workspace is browsing a backed-up snapshot rather than
    /// the live/cached active project.
    pub(crate) fn is_browsing_snapshot(&self) -> bool {
        !matches!(self.ui_state.project, ProjectSelection::Active)
    }

    /// Builds a kit clipboard from a backed-up project (no device I/O) and
    /// stores it for pasting.
    pub(crate) fn copy_kit_from_backup(&mut self, slot: u32, kit_index: usize) {
        self.ensure_raw_snapshot(slot);
        let Some(snapshot) = self
            .raw_snapshots
            .get(&slot)
            .and_then(Option::as_ref)
            .cloned()
        else {
            self.toast("This project's backup is not indexed — index it first.".to_owned());
            return;
        };
        let Some(bytes) = cache::load_project_backup(&cache::default_dir(), slot) else {
            self.toast("Backup file missing — re-index this project.".to_owned());
            return;
        };
        let label = crate::model::names::project_display_name(&snapshot.name, slot)
            .text()
            .to_owned();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            transfer::capture_kit_from_backup(&bytes, &snapshot, &label, kit_index)
        }))
        .unwrap_or_else(|_| Err("internal error while converting the kit".into()));
        match result {
            Ok(clip) => {
                let hint = clip.source_hint.clone();
                let _ = transfer::save_kit(&clip);
                self.kit_clipboard = Some(clip);
                self.toast(format!(
                    "Copied {hint} — paste it from the target project's Kits tab"
                ));
            }
            Err(e) => self.toast(format!("Copy failed: {e}")),
        }
    }

    /// Guesses which backed-up slot the active project is, by matching kit
    /// names and sample-slot content hashes against every raw backup. The
    /// device gives no direct way to know; this is a clearly-labeled
    /// heuristic, recomputed after syncs and backups.
    pub(crate) fn identify_active_project(&mut self) {
        self.active_guess = None;
        if self.model.is_none() {
            return;
        }
        let slots: Vec<u32> = self
            .manifest
            .iter()
            .filter(|(_, info)| info.raw)
            .map(|(slot, _)| *slot)
            .collect();
        let mut best: Option<(u32, usize)> = None;
        let mut tie = false;
        for slot in slots {
            self.ensure_raw_snapshot(slot);
            let (Some(model), Some(Some(snapshot))) =
                (self.model.as_ref(), self.raw_snapshots.get(&slot))
            else {
                continue;
            };
            let mut score = 0usize;
            for (i, kit) in model.kits.iter().enumerate() {
                let name = kit.name.trim();
                if !name.is_empty() && snapshot.kits.get(i).is_some_and(|k| k.name.trim() == name) {
                    score += 2;
                }
            }
            for sample in &model.samples {
                if sample.hash != 0
                    && snapshot
                        .samples
                        .iter()
                        .any(|s| s.slot == sample.slot && s.hash == sample.hash)
                {
                    score += 3;
                }
            }
            match &mut best {
                Some((_, top)) if score == *top => tie = true,
                Some((slot_ref, top)) if score > *top => {
                    *slot_ref = slot;
                    *top = score;
                    tie = false;
                }
                None => best = Some((slot, score)),
                _ => {}
            }
        }
        if let Some((slot, score)) = best {
            if score >= 6 && !tie {
                self.active_guess = Some(slot);
            }
        }
    }

    /// Pulls finished background work into app state.
    fn absorb_events(&mut self, ctx: &egui::Context) {
        if let Some((model, missed)) = self.sync.take_finished() {
            self.last_sync = Some(SyncInfo {
                at: Instant::now(),
                missed: missed.len(),
            });
            let (kind, title, detail) = if missed.is_empty() {
                (ActivityKind::Success, "Sync complete".to_owned(), None)
            } else {
                let detail = missed
                    .iter()
                    .map(MissedObject::describe)
                    .collect::<Vec<_>>()
                    .join("\n");
                (
                    ActivityKind::Warning,
                    format!("Partial sync · {} items unavailable", missed.len()),
                    Some(detail),
                )
            };
            self.sync_misses = missed;
            self.log_activity(kind, title, detail);
            self.model = Some(model);
            self.identify_active_project();
            // A sync re-reads whatever project the hardware has loaded now,
            // which may not be the one the session backup covers.
            self.execute.invalidate_backup();
            // The model under an open kit-edit draft just changed; stale
            // "current" values would make the draft lie. Start over.
            self.discard_kit_draft("Kit edits discarded — device state was re-read");
        }

        // A reconnect (or a different Rytm) invalidates the session backup for
        // the same reason: the active project may have changed while we were
        // not looking. An open kit-edit draft was confirmed against a device
        // state that may no longer exist — it goes too.
        let in_port = self.connected_in_port();
        if in_port != self.seen_in_port {
            self.seen_in_port = in_port;
            self.execute.invalidate_backup();
            self.discard_kit_draft("Kit edits discarded — the device connection changed");
        }

        // Files dropped anywhere on the window enter the import queue.
        let dropped: Vec<std::path::PathBuf> = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        if !dropped.is_empty() {
            let count = dropped.len();
            self.import.add_dropped_files(dropped, ctx);
            self.toast(format!(
                "{count} file{} added to Import",
                if count == 1 { "" } else { "s" }
            ));
            self.log_activity(
                ActivityKind::Info,
                format!("{count} file(s) added to the import queue"),
                None,
            );
        }

        if let Some((ok, failed)) = self.backup.take_finished() {
            self.manifest = cache::load_project_manifest(&cache::default_dir());
            self.raw_snapshots.clear();
            self.raw_models.clear();
            self.identify_active_project();
            let (kind, title) = if failed == 0 {
                (
                    ActivityKind::Success,
                    format!("Backed up {ok} projects to this computer"),
                )
            } else {
                (
                    ActivityKind::Warning,
                    format!("Backed up {ok} projects · {failed} failed"),
                )
            };
            self.toast(title.clone());
            self.log_activity(kind, title, None);
        }

        if let Some(result) = self.capture.take_finished() {
            match result {
                Ok(Captured::Pattern(clip)) => {
                    let hint = clip.source_hint.clone();
                    self.clipboard = Some(clip);
                    self.toast(format!("Copied {hint} — paste it in any project"));
                    self.log_activity(
                        ActivityKind::Success,
                        format!("Copied {hint} to the clipboard"),
                        None,
                    );
                }
                Ok(Captured::Kit(clip)) => {
                    let hint = clip.source_hint.clone();
                    self.kit_clipboard = Some(clip);
                    self.toast(format!("Copied {hint} — paste it in any project"));
                    self.log_activity(
                        ActivityKind::Success,
                        format!("Copied {hint} to the clipboard"),
                        None,
                    );
                }
                Err(e) => {
                    self.toast(format!("Capture failed: {e}"));
                    self.log_activity(ActivityKind::Error, format!("Capture failed: {e}"), None);
                }
            }
        }

        if let Some(result) = self.fileops.take_finished() {
            match result {
                Ok((outcome, summary)) => {
                    self.apply_file_op_outcome(outcome);
                    self.toast(summary.clone());
                    self.log_activity(ActivityKind::Success, summary, None);
                }
                Err(e) => {
                    self.toast(format!("Operation failed: {e}"));
                    self.log_activity(ActivityKind::Error, format!("Operation failed: {e}"), None);
                }
            }
        }

        // Esc leaves the sample-trace state (PRD §16).
        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.ui_state.kits.traced_slot = None;
        }
    }

    /// Folds a completed file operation into the local model and caches.
    fn apply_file_op_outcome(&mut self, outcome: FileOpOutcome) {
        match outcome {
            FileOpOutcome::Renamed { slot, name } => {
                if let Some(model) = self.model.as_mut() {
                    if let Some(p) = model.projects.iter_mut().find(|p| p.id == slot) {
                        p.name = name;
                    }
                    let _ = cache::save(model);
                }
                self.manifest = cache::load_project_manifest(&cache::default_dir());
                self.raw_snapshots.remove(&slot);
                self.raw_models.remove(&slot);
            }
            FileOpOutcome::Copied { dst, name, .. } => {
                if let Some(model) = self.model.as_mut() {
                    model.projects.retain(|p| p.id != dst);
                    model
                        .projects
                        .push(crate::model::ProjectInfo { id: dst, name });
                    model.projects.sort_by_key(|p| p.id);
                    let _ = cache::save(model);
                }
                self.manifest = cache::load_project_manifest(&cache::default_dir());
                self.raw_snapshots.remove(&dst);
                self.raw_models.remove(&dst);
            }
            FileOpOutcome::SampleLoaded(info) => {
                let slot = info.slot;
                if let Some(model) = self.model.as_mut() {
                    model.samples.retain(|s| s.slot != slot);
                    model.samples.push(info);
                    model.samples.sort_by_key(|s| s.slot);
                    let _ = cache::save(model);
                }
                // A pad was waiting for this sample: hand over to the
                // assign-confirm flow (object write, backup-first).
                if let Some((kit_index, track)) = self.ui_state.pending_pad_assign.take() {
                    self.ui_state.nav = Nav::Projects;
                    self.ui_state.tab = ProjectTab::Kits;
                    self.ui_state.project = ProjectSelection::Active;
                    self.ui_state.kits.selected_kit = Some(kit_index);
                    self.execute.pending = Some(crate::gui::execute::PendingAssign {
                        kit_index,
                        track,
                        slot,
                    });
                }
            }
        }
    }

    fn show_toasts(&mut self, ctx: &egui::Context) {
        const TOAST_SECONDS: f32 = 4.0;
        self.toasts
            .retain(|(_, at)| at.elapsed().as_secs_f32() < TOAST_SECONDS);
        if self.toasts.is_empty() {
            return;
        }
        egui::Area::new(egui::Id::new("toasts"))
            .anchor(Align2::RIGHT_BOTTOM, egui::vec2(-16.0, -16.0))
            .show(ctx, |ui| {
                for (text, _) in &self.toasts {
                    Frame::new()
                        .fill(theme::SURFACE_ELEVATED)
                        .stroke(Stroke::new(1.0, theme::BORDER))
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(Margin::symmetric(12, 8))
                        .show(ui, |ui| {
                            ui.label(text);
                        });
                }
            });
        ctx.request_repaint_after(std::time::Duration::from_millis(250));
    }

    fn workspace(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        match self.ui_state.nav {
            Nav::Projects => views::project::show(self, ui),
            Nav::Drive => views::drive::show(self, ui),
            Nav::Import => {
                let in_port = self.connected_in_port();
                views::import_page::show(ui, &mut self.import, in_port.as_deref(), ctx);
            }
            Nav::Activity => views::activity::show(
                ui,
                &self.activity,
                &self.sync,
                &self.import,
                &self.backup,
                self.fileops.running_description(),
            ),
            Nav::Diagnostics => views::diagnostics::show(
                ui,
                &self.watcher,
                self.model.as_ref(),
                self.last_sync,
                &self.sync_misses,
            ),
        }
    }

    pub(crate) fn connected_in_port(&self) -> Option<String> {
        match &self.watcher.snapshot().device {
            DeviceState::Connected(info) => Some(info.port_name.clone()),
            DeviceState::Searching => None,
        }
    }
}

impl eframe::App for KitsmithApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.absorb_events(&ctx);

        egui::Panel::top("app-bar")
            .exact_size(theme::APP_BAR_HEIGHT)
            .frame(
                Frame::new()
                    .fill(theme::BG_SIDEBAR)
                    .inner_margin(Margin::symmetric(12, 0)),
            )
            .show(ui, |ui| {
                top_bar::show(self, ui, &ctx);
            });

        if !self.ui_state.sidebar_hidden {
            egui::Panel::left("sidebar")
                .default_size(theme::SIDEBAR_WIDTH)
                .resizable(true)
                .min_size(200.0)
                .max_size(320.0)
                .frame(
                    Frame::new()
                        .fill(theme::BG_SIDEBAR)
                        .inner_margin(Margin::same(10)),
                )
                .show(ui, |ui| {
                    sidebar::show(self, ui);
                });
        }

        egui::CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(theme::BG_APP)
                    .inner_margin(Margin::same(16)),
            )
            .show(ui, |ui| {
                self.workspace(ui, &ctx);
            });

        project_modals::show(self, &ctx);

        // Plan preview / execution modal, if a drag landed.
        let in_port = self.connected_in_port();
        if let Some(report) = self
            .execute
            .show(&ctx, in_port.as_deref(), self.model.as_ref())
        {
            let (kind, title) = if report.succeeded() {
                (ActivityKind::Success, "Plan completed".to_owned())
            } else {
                (ActivityKind::Error, "Plan stopped".to_owned())
            };
            let detail = report.completed.join("\n");
            self.log_activity(kind, title, Some(detail));
            if let Some(draft) = self.ui_state.kits.pending_save.take() {
                let kit_label = self
                    .model
                    .as_ref()
                    .and_then(|m| m.kits.get(draft.kit_index))
                    .map_or_else(
                        || format!("Kit {:02}", draft.kit_index + 1),
                        |k| {
                            crate::model::names::kit_display_name(&k.name, k.index)
                                .text()
                                .to_owned()
                        },
                    );
                if report.succeeded() {
                    // The read-back verified these values on the device;
                    // mirror them locally instead of forcing a re-sync.
                    if let Some(model) = &mut self.model {
                        crate::gui::kit_editor::apply_draft_to_model(model, &draft);
                        if let Err(e) = cache::save(model) {
                            self.toast(format!("cache write failed: {e}"));
                        }
                    }
                    self.toast(format!("{kit_label} saved and verified"));
                } else if self.ui_state.kits.edit_draft.is_none() {
                    // The write did not complete — hand the edits back to
                    // the editor so nothing the user typed is lost. (The
                    // write may still have landed before verification
                    // failed, so the dashboard can trail the device.)
                    self.ui_state.kits.edit_draft = Some(draft);
                    self.toast(format!(
                        "Save failed — your edits are back in the kit editor. \
                         Re-sync to see {kit_label}'s actual device state."
                    ));
                } else {
                    // A newer draft exists; never silently overwrite it.
                    self.toast(format!(
                        "Save of {kit_label} failed; its edits were dropped in \
                         favor of the draft you started since. Re-sync to see \
                         the device's actual state."
                    ));
                }
            }
        }

        self.show_toasts(&ctx);
    }
}

/// Formats how long ago an instant was, for status lines.
pub fn ago(at: Instant) -> String {
    let secs = at.elapsed().as_secs();
    match secs {
        0..=9 => "just now".to_owned(),
        10..=59 => format!("{secs}s ago"),
        60..=3599 => format!("{}m ago", secs / 60),
        _ => format!("{}h ago", secs / 3600),
    }
}

/// Reusable page title.
pub fn page_title(ui: &mut egui::Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .size(theme::FONT_PAGE_TITLE)
            .color(theme::TEXT_PRIMARY)
            .strong(),
    );
}
