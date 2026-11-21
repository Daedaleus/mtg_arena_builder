//! Design tokens: all design values of the interface in one place – colors
//! (for dark and light mode), spacing and image sizes. The building blocks in
//! `atoms` and all views take their values only from here.

use eframe::egui::{self, Color32};

use crate::carddb::Rarity;

/// Text size of the heading of a collapsible section.
pub(super) const FOLD_TITLE: f32 = 17.0;
/// Space between sections of a page.
pub(super) const SECTION_GAP: f32 = 10.0;
/// Key figure tiles: minimum width and inner padding.
pub(super) const TILE_WIDTH: f32 = 150.0;
pub(super) const TILE_PADDING: f32 = 10.0;
/// Text size of the value in a key figure tile.
pub(super) const TILE_VALUE: f32 = 22.0;
/// Cards: inner padding and corner rounding.
pub(super) const CARD_PADDING: f32 = 12.0;
pub(super) const CARD_ROUNDING: f32 = 8.0;

/// Background of key figure tiles.
pub(super) fn tile_fill(ctx: &egui::Context) -> Color32 {
    ctx.style().visuals.faint_bg_color
}

/// Frame line of cards.
pub(super) fn card_stroke(ctx: &egui::Context) -> egui::Stroke {
    ctx.style().visuals.widgets.noninteractive.bg_stroke
}

/// Text size of the main areas in the navigation.
pub(super) const AREA_TEXT: f32 = 16.0;
/// Corner rounding and inner padding of the game mode box.
pub(super) const NAV_ROUNDING: f32 = 6.0;
pub(super) const NAV_PADDING: f32 = 4.0;

/// Background that highlights the frame of everything: the game mode.
pub(super) fn accent_fill(ctx: &egui::Context) -> Color32 {
    ctx.style().visuals.selection.bg_fill.gamma_multiply(0.35)
}

/// Meaning of a color; the color itself depends on the design.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Tone {
    /// Good, done, won.
    Good,
    /// Attention, craftable, close.
    Warn,
    /// Error, missing, lost.
    Bad,
    /// Unimportant or unknown.
    Muted,
}

struct Palette {
    good: Color32,
    warn: Color32,
    bad: Color32,
    muted: Color32,
    uncommon: Color32,
    rare: Color32,
    mythic: Color32,
}

/// Light tones on a dark background.
const DARK: Palette = Palette {
    good: Color32::from_rgb(110, 200, 120),
    warn: Color32::from_rgb(235, 195, 80),
    bad: Color32::from_rgb(235, 115, 105),
    muted: Color32::from_rgb(150, 150, 150),
    uncommon: Color32::from_rgb(170, 200, 220),
    rare: Color32::from_rgb(230, 195, 100),
    mythic: Color32::from_rgb(240, 130, 60),
};

/// Darker tones on a light background, so the contrast is enough.
const LIGHT: Palette = Palette {
    good: Color32::from_rgb(25, 120, 45),
    warn: Color32::from_rgb(150, 100, 0),
    bad: Color32::from_rgb(185, 35, 35),
    muted: Color32::from_rgb(105, 105, 105),
    uncommon: Color32::from_rgb(40, 90, 130),
    rare: Color32::from_rgb(140, 100, 0),
    mythic: Color32::from_rgb(180, 70, 0),
};

fn palette(ctx: &egui::Context) -> &'static Palette {
    if ctx.style().visuals.dark_mode {
        &DARK
    } else {
        &LIGHT
    }
}

/// Color of a meaning in the current design.
pub(super) fn tone(ctx: &egui::Context, tone: Tone) -> Color32 {
    let p = palette(ctx);
    match tone {
        Tone::Good => p.good,
        Tone::Warn => p.warn,
        Tone::Bad => p.bad,
        Tone::Muted => p.muted,
    }
}

/// Color of a rarity in the current design.
pub(super) fn rarity(ctx: &egui::Context, r: Rarity) -> Color32 {
    let p = palette(ctx);
    match r {
        Rarity::Uncommon => p.uncommon,
        Rarity::Rare => p.rare,
        Rarity::Mythic => p.mythic,
        _ => p.muted,
    }
}

/// Spacing between elements and inner padding of buttons.
const ITEM_SPACING: egui::Vec2 = egui::vec2(8.0, 6.0);
const BUTTON_PADDING: egui::Vec2 = egui::vec2(8.0, 4.0);
/// Size of a card image on hover.
pub(super) const CARD_IMAGE: egui::Vec2 = egui::vec2(244.0, 340.0);

/// Set spacing and text size for the whole interface.
pub(super) fn apply(ctx: &egui::Context, zoom: f32) {
    ctx.style_mut(|style| {
        style.spacing.item_spacing = ITEM_SPACING;
        style.spacing.button_padding = BUTTON_PADDING;
    });
    ctx.set_zoom_factor(zoom);
}
