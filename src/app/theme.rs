//! Design-Tokens: alle Gestaltungswerte der Oberfläche an einer Stelle –
//! Farben (für dunkles und helles Design), Abstände und Bildgrößen. Die
//! Bausteine in `atoms` und alle Ansichten holen ihre Werte nur von hier.

use eframe::egui::{self, Color32};

use crate::carddb::Rarity;

/// Bedeutung einer Farbe; die Farbe selbst hängt vom Design ab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Tone {
    /// Gut, erledigt, gewonnen.
    Good,
    /// Achtung, herstellbar, knapp.
    Warn,
    /// Fehler, fehlt, verloren.
    Bad,
    /// Unwichtig oder unbekannt.
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

/// Helle Töne auf dunklem Grund.
const DARK: Palette = Palette {
    good: Color32::from_rgb(110, 200, 120),
    warn: Color32::from_rgb(235, 195, 80),
    bad: Color32::from_rgb(235, 115, 105),
    muted: Color32::from_rgb(150, 150, 150),
    uncommon: Color32::from_rgb(170, 200, 220),
    rare: Color32::from_rgb(230, 195, 100),
    mythic: Color32::from_rgb(240, 130, 60),
};

/// Dunklere Töne auf hellem Grund, damit der Kontrast reicht.
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

/// Farbe einer Bedeutung im aktuellen Design.
pub(super) fn tone(ctx: &egui::Context, tone: Tone) -> Color32 {
    let p = palette(ctx);
    match tone {
        Tone::Good => p.good,
        Tone::Warn => p.warn,
        Tone::Bad => p.bad,
        Tone::Muted => p.muted,
    }
}

/// Farbe einer Seltenheit im aktuellen Design.
pub(super) fn rarity(ctx: &egui::Context, r: Rarity) -> Color32 {
    let p = palette(ctx);
    match r {
        Rarity::Uncommon => p.uncommon,
        Rarity::Rare => p.rare,
        Rarity::Mythic => p.mythic,
        _ => p.muted,
    }
}

/// Abstand zwischen Elementen und Innenabstand von Knöpfen.
const ITEM_SPACING: egui::Vec2 = egui::vec2(8.0, 6.0);
const BUTTON_PADDING: egui::Vec2 = egui::vec2(8.0, 4.0);
/// Größe eines Kartenbilds beim Überfahren.
pub(super) const CARD_IMAGE: egui::Vec2 = egui::vec2(244.0, 340.0);

/// Abstände und Textgröße für die ganze Oberfläche setzen.
pub(super) fn apply(ctx: &egui::Context, zoom: f32) {
    ctx.style_mut(|style| {
        style.spacing.item_spacing = ITEM_SPACING;
        style.spacing.button_padding = BUTTON_PADDING;
    });
    ctx.set_zoom_factor(zoom);
}
