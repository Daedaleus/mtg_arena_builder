//! Design tokens: all design values of the interface in one place – colors
//! (for dark and light mode), spacing and image sizes. The building blocks in
//! `atoms` and all views take their values only from here.

use eframe::egui::{self, Color32};

use crate::carddb::Rarity;

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
