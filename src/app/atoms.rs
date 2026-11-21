//! Atome: die kleinsten Bausteine der Oberfläche (Atomic Design). Sie kennen
//! nur Text und Bedeutung, ihre Gestaltung kommt aus `theme`. Größere
//! Bausteine (Moleküle, Organismen) stehen in `widgets`, die Reiter setzen sie
//! nur zusammen.

use eframe::egui::{self, RichText};

use super::theme::{self, Tone};
use crate::carddb::Rarity;

/// Gedämpfter Hinweis- oder Erklärtext.
pub(super) fn note(ui: &mut egui::Ui, text: impl Into<String>) -> egui::Response {
    ui.label(RichText::new(text).weak())
}

/// Text in der Farbe einer Bedeutung (gut, Achtung, schlecht).
pub(super) fn toned(ui: &mut egui::Ui, tone: Tone, text: impl Into<String>) -> egui::Response {
    let color = theme::tone(ui.ctx(), tone);
    ui.label(RichText::new(text).color(color))
}

/// Seltenheit als Wort in ihrer Farbe.
pub(super) fn rarity_label(ui: &mut egui::Ui, r: Rarity) -> egui::Response {
    let color = theme::rarity(ui.ctx(), r);
    ui.label(RichText::new(r.label()).color(color))
}
