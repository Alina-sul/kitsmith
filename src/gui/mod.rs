//! The egui desktop shell (PRD v2 Part II): app bar, sidebar navigation,
//! workspace views, inspectors and the plan modal.

mod app;
mod backup;
mod drive_ctl;
mod execute;
mod fileops;
mod import;
mod project_modals;
mod sidebar;
mod sync;
mod theme;
mod top_bar;
mod transfer_ctl;
mod views;
mod widgets;

use app::RytmdashApp;

/// Opens the rytmdash window. Blocks until it closes.
pub fn run() -> eframe::Result {
    // CoreMIDI delivers setup-change notifications to the runloop of the
    // thread that makes the process's FIRST CoreMIDI call. Make that call
    // here on the main thread (whose event loop runs under eframe/winit) —
    // otherwise the port list freezes at its startup state and devices
    // plugged in later are never seen.
    let _ = crate::midi::ports::snapshot();

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([1024.0, 640.0])
            .with_title("rytmdash"),
        ..Default::default()
    };
    eframe::run_native(
        "rytmdash",
        options,
        Box::new(|cc| Ok(Box::new(RytmdashApp::new(cc)))),
    )
}
