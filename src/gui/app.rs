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
use crate::gui::execute::ExecutePanel;
use crate::gui::import::ImportPanel;
use crate::gui::sync::SyncController;
use crate::gui::transfer_ctl::{CaptureController, Captured};
use crate::gui::views::activity::{ActivityEntry, ActivityKind};
use crate::gui::{sidebar, theme, top_bar, views};
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
}

/// When the last sync finished and how complete it was.
#[derive(Clone, Copy)]
pub struct SyncInfo {
    pub at: Instant,
    pub missed: usize,
}

pub struct RytmdashApp {
    pub(crate) watcher: DeviceWatcher,
    pub(crate) sync: SyncController,
    pub(crate) import: ImportPanel,
    pub(crate) execute: ExecutePanel,
    pub(crate) model: Option<ProjectModel>,
    pub(crate) local_meta: LocalMeta,
    pub(crate) backup: BackupController,
    pub(crate) manifest: ProjectManifest,
    pub(crate) capture: CaptureController,
    pub(crate) clipboard: Option<PatternClipboard>,
    pub(crate) kit_clipboard: Option<KitClipboard>,
    /// Parsed project snapshots from raw backups (`None` = unparseable),
    /// loaded lazily per slot.
    pub(crate) raw_snapshots: HashMap<u32, Option<RawProjectSnapshot>>,
    pub(crate) ui_state: UiState,
    pub(crate) last_sync: Option<SyncInfo>,
    /// Objects the last sync could not read (for Diagnostics).
    pub(crate) sync_misses: Vec<MissedObject>,
    pub(crate) activity: Vec<ActivityEntry>,
    toasts: Vec<(String, Instant)>,
}

impl RytmdashApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);
        let ctx = cc.egui_ctx.clone();
        Self {
            watcher: DeviceWatcher::spawn(move || ctx.request_repaint()),
            sync: SyncController::default(),
            import: ImportPanel::default(),
            execute: ExecutePanel::default(),
            model: cache::load(),
            local_meta: localmeta::load(),
            backup: BackupController::default(),
            manifest: cache::load_project_manifest(&cache::default_dir()),
            capture: CaptureController::default(),
            clipboard: transfer::load(),
            kit_clipboard: transfer::load_kit(),
            raw_snapshots: HashMap::new(),
            ui_state: UiState::default(),
            last_sync: None,
            sync_misses: Vec::new(),
            activity: Vec::new(),
            toasts: Vec::new(),
        }
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
            self.import.add_dropped_files(dropped);
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
                    self.toast(format!("Captured {hint} for transfer"));
                    self.log_activity(
                        ActivityKind::Success,
                        format!("Captured {hint} for cross-project transfer"),
                        None,
                    );
                }
                Ok(Captured::Kit(clip)) => {
                    let hint = clip.source_hint.clone();
                    self.kit_clipboard = Some(clip);
                    self.toast(format!("Captured {hint} for transfer"));
                    self.log_activity(
                        ActivityKind::Success,
                        format!("Captured {hint} for cross-project transfer"),
                        None,
                    );
                }
                Err(e) => {
                    self.toast(format!("Capture failed: {e}"));
                    self.log_activity(ActivityKind::Error, format!("Capture failed: {e}"), None);
                }
            }
        }

        // Esc leaves the sample-trace state (PRD §16).
        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.ui_state.kits.traced_slot = None;
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
            Nav::Drive => views::drive::show(ui, self.model.as_ref()),
            Nav::Import => {
                let in_port = self.connected_in_port();
                views::import_page::show(ui, &mut self.import, in_port.as_deref(), ctx);
            }
            Nav::Activity => {
                views::activity::show(ui, &self.activity, &self.sync, &self.import, &self.backup)
            }
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

impl eframe::App for RytmdashApp {
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
