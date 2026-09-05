//! The egui dashboard shell.

mod app;

use app::RytmdashApp;

/// Opens the rytmdash window. Blocks until it closes.
pub fn run() -> eframe::Result {
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
