//! The egui dashboard shell.

mod app;
mod dashboard;
mod execute;
mod import;
mod sync;
mod theme;

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
            .with_inner_size([720.0, 480.0])
            .with_title("rytmdash"),
        ..Default::default()
    };
    eframe::run_native(
        "rytmdash",
        options,
        Box::new(|cc| Ok(Box::new(RytmdashApp::new(cc)))),
    )
}
