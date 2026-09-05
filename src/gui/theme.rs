//! Visual constants: track identity colors and shared accents.
//!
//! The twelve drum tracks get stable identity colors (used as pad stripes and
//! highlights) so a track reads the same everywhere in the app.

use eframe::egui::Color32;

/// Purple used for everything sample/slot related.
pub const SLOT_COLOR: Color32 = Color32::from_rgb(0x9B, 0x59, 0xB6);

/// Green used for "connected / ok" states.
pub const OK_COLOR: Color32 = Color32::from_rgb(0x2E, 0xCC, 0x71);

/// The device's track order: pads left→right, bottom row first.
pub const TRACK_LABELS: [&str; 12] = [
    "BD", "SD", "RS", "CP", "BT", "LT", "MT", "HT", "CH", "OH", "CY", "CB",
];

/// Identity color per track (warm lows → cool highs).
pub const TRACK_COLORS: [Color32; 12] = [
    Color32::from_rgb(0xE7, 0x4C, 0x3C), // BD
    Color32::from_rgb(0xE6, 0x7E, 0x22), // SD
    Color32::from_rgb(0xF1, 0xC4, 0x0F), // RS
    Color32::from_rgb(0xF3, 0x9C, 0x12), // CP
    Color32::from_rgb(0x27, 0xAE, 0x60), // BT
    Color32::from_rgb(0x2E, 0xCC, 0x71), // LT
    Color32::from_rgb(0x1A, 0xBC, 0x9C), // MT
    Color32::from_rgb(0x16, 0xA0, 0x85), // HT
    Color32::from_rgb(0x34, 0x98, 0xDB), // CH
    Color32::from_rgb(0x5D, 0xAD, 0xE2), // OH
    Color32::from_rgb(0x8E, 0x7C, 0xC3), // CY
    Color32::from_rgb(0xAF, 0x7A, 0xC5), // CB
];
