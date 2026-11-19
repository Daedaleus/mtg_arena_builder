//! Zeichnet das Begleitfenster.

use super::*;

/// Zeichnet das Begleitfenster.
pub fn draw(ui: &mut egui::Ui, view: &CompanionView) {
    let green = egui::Color32::from_rgb(110, 200, 120);
    let yellow = egui::Color32::from_rgb(235, 195, 80);
    ui.heading(&view.status);
    if let Some((me, opp)) = view.life {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(tr!("live_draw.du", me = me))
                    .strong()
                    .size(18.0),
            );
            ui.label(egui::RichText::new(tr!("live_draw.gegner", opp = opp)).size(18.0));
        });
    }
    if !view.board.is_empty() {
        ui.label(egui::RichText::new(&view.board).weak());
    }
    ui.separator();
    for (level, text) in &view.hints {
        let rich = egui::RichText::new(text);
        ui.label(match level {
            Level::Good => rich.color(green).strong(),
            Level::Warn => rich.color(yellow).strong(),
            Level::Info => rich,
        });
    }
    if !view.opponent.is_empty() {
        ui.separator();
        ui.label(egui::RichText::new(tr!("live_draw.gegner_2", opponent = view.opponent)).strong());
        if !view.opp_cards.is_empty() {
            ui.label(egui::RichText::new(view.opp_cards.join(", ")).weak());
        }
    }
}
