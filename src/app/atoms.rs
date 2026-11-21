//! Atoms: the smallest building blocks of the interface (Atomic Design). They
//! only know text and meaning; their styling comes from `theme`. Larger
//! building blocks (molecules, organisms) live in `widgets`; the tabs only
//! put them together.

use eframe::egui::{self, RichText};

use super::theme::{self, Tone};
use crate::carddb::Rarity;

/// Muted hint or explanation text.
pub(super) fn note(ui: &mut egui::Ui, text: impl Into<String>) -> egui::Response {
    ui.label(RichText::new(text).weak())
}

/// Text in the color of a meaning (good, attention, bad).
pub(super) fn toned(ui: &mut egui::Ui, tone: Tone, text: impl Into<String>) -> egui::Response {
    let color = theme::tone(ui.ctx(), tone);
    ui.label(RichText::new(text).color(color))
}

/// Key figure tile: small label, big value (optionally in the color of a
/// meaning) and a muted line below.
pub(super) fn stat_tile(
    ui: &mut egui::Ui,
    label: impl Into<String>,
    value: impl Into<String>,
    tone: Option<Tone>,
    sub: impl Into<String>,
) -> egui::Response {
    egui::Frame::none()
        .fill(theme::tile_fill(ui.ctx()))
        .rounding(theme::CARD_ROUNDING)
        .inner_margin(theme::TILE_PADDING)
        .show(ui, |ui| {
            ui.set_min_width(theme::TILE_WIDTH);
            ui.vertical(|ui| {
                ui.label(RichText::new(label).weak());
                let mut value = RichText::new(value).size(theme::TILE_VALUE).strong();
                if let Some(t) = tone {
                    value = value.color(theme::tone(ui.ctx(), t));
                }
                ui.label(value);
                ui.label(RichText::new(sub).weak().small());
            });
        })
        .response
}

/// Rarity as a word in its color.
pub(super) fn rarity_label(ui: &mut egui::Ui, r: Rarity) -> egui::Response {
    let color = theme::rarity(ui.ctx(), r);
    ui.label(RichText::new(r.label()).color(color))
}
