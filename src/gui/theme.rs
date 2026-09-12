//! Design tokens (PRD v2 §39–§41) and the explicit dark theme.
//!
//! Kitsmith owns its appearance: [`apply`] forces the dark palette and the
//! typography scale on startup regardless of the OS appearance. All colors,
//! sizes and gaps used by the GUI live here — nowhere else.

use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle};

// ---- Surfaces -------------------------------------------------------------

/// App background.
pub const BG_APP: Color32 = Color32::from_rgb(0x0D, 0x0F, 0x12);
/// Sidebar / secondary background.
pub const BG_SIDEBAR: Color32 = Color32::from_rgb(0x12, 0x15, 0x1A);
/// Card / panel surface.
pub const SURFACE: Color32 = Color32::from_rgb(0x18, 0x1C, 0x22);
/// Elevated surface (selected rows, modals).
pub const SURFACE_ELEVATED: Color32 = Color32::from_rgb(0x1E, 0x23, 0x2B);
/// Hover fill.
pub const HOVER: Color32 = Color32::from_rgb(0x25, 0x2B, 0x35);
/// Hairline borders.
pub const BORDER: Color32 = Color32::from_rgb(0x30, 0x37, 0x43);

// ---- Text -----------------------------------------------------------------

pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(0xF3, 0xF5, 0xF7);
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(0xA6, 0xAF, 0xBA);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x74, 0x7E, 0x8A);

// ---- Semantics ------------------------------------------------------------

/// Purple: everything sample/slot related (raised luminance for dark bg).
pub const SAMPLE_PURPLE: Color32 = Color32::from_rgb(0xB0, 0x7C, 0xD8);
// Scene blue (#4DA3FF) and Performance green (#38C977) join this palette
// with the Scenes/Performance visualization milestone (PRD §18–§19).
/// Success / connected / active-on-device.
pub const SUCCESS: Color32 = Color32::from_rgb(0x2E, 0xCC, 0x71);
/// Warnings, partial sync.
pub const WARNING: Color32 = Color32::from_rgb(0xF2, 0xB8, 0x4B);
/// Failures and destructive actions.
pub const DANGER: Color32 = Color32::from_rgb(0xE1, 0x64, 0x64);

/// Identity color per track (warm lows → cool highs), tuned for dark bg.
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

// ---- Typography (px equivalents; nothing user-facing below 12) ------------

pub const FONT_PAGE_TITLE: f32 = 22.0;
pub const FONT_SECTION: f32 = 15.0;
pub const FONT_CARD_PRIMARY: f32 = 15.0;
pub const FONT_BODY: f32 = 13.5;
pub const FONT_META: f32 = 12.0;

// ---- Layout ---------------------------------------------------------------

pub const APP_BAR_HEIGHT: f32 = 52.0;
pub const SIDEBAR_WIDTH: f32 = 260.0;
pub const INSPECTOR_WIDTH: f32 = 320.0;
pub const KIT_LIST_WIDTH: f32 = 220.0;
pub const LIST_ROW_HEIGHT: f32 = 40.0;
pub const KIT_ROW_HEIGHT: f32 = 52.0;
/// Content gap scale.
pub const GAP_S: f32 = 8.0;
pub const GAP_M: f32 = 12.0;
pub const GAP_L: f32 = 16.0;
pub const GAP_XL: f32 = 24.0;

/// Pad card bounds (PRD §14.1); actual width flexes with the grid.
pub const PAD_MIN_WIDTH: f32 = 120.0;
pub const PAD_MAX_WIDTH: f32 = 220.0;
pub const PAD_HEIGHT: f32 = 112.0;
pub const PAD_GAP: f32 = 10.0;

/// Applies the Kitsmith dark theme and typography scale to the context.
pub fn apply(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);

    let mut visuals = egui::Visuals::dark();
    // Buttons and selectable labels show a pointing hand on hover; custom
    // clickable widgets add the same via `on_hover_cursor`.
    visuals.interact_cursor = Some(egui::CursorIcon::PointingHand);
    visuals.override_text_color = Some(TEXT_PRIMARY);
    visuals.panel_fill = BG_APP;
    visuals.window_fill = SURFACE_ELEVATED;
    visuals.window_stroke = Stroke::new(1.0, BORDER);
    visuals.extreme_bg_color = SURFACE;
    visuals.faint_bg_color = SURFACE;
    visuals.widgets.noninteractive.bg_fill = BG_APP;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    visuals.widgets.inactive.bg_fill = SURFACE_ELEVATED;
    visuals.widgets.inactive.weak_bg_fill = SURFACE_ELEVATED;
    visuals.widgets.hovered.bg_fill = HOVER;
    visuals.widgets.hovered.weak_bg_fill = HOVER;
    visuals.widgets.active.bg_fill = HOVER;
    visuals.widgets.active.weak_bg_fill = HOVER;
    visuals.selection.bg_fill = HOVER;
    visuals.selection.stroke = Stroke::new(1.0, TEXT_SECONDARY);
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(6);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(6);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(6);
    visuals.widgets.active.corner_radius = CornerRadius::same(6);
    ctx.set_visuals(visuals);

    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (
                TextStyle::Heading,
                FontId::new(FONT_PAGE_TITLE, FontFamily::Proportional),
            ),
            (
                TextStyle::Body,
                FontId::new(FONT_BODY, FontFamily::Proportional),
            ),
            (
                TextStyle::Button,
                FontId::new(FONT_BODY, FontFamily::Proportional),
            ),
            (
                TextStyle::Small,
                FontId::new(FONT_META, FontFamily::Proportional),
            ),
            (
                TextStyle::Monospace,
                FontId::new(FONT_BODY, FontFamily::Monospace),
            ),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(GAP_S, 6.0);
        style.spacing.button_padding = egui::vec2(12.0, 7.0);
        style.spacing.interact_size.y = 30.0;
    });
}
