//! Wiederverwendbare Anzeige-Bausteine: Probehände und Wildcard-Plan.

use super::*;

/// Ergebnis der Probehände: Empfehlungen und Kennzahlen.
pub(super) fn sim_view(ui: &mut egui::Ui, r: &sim::SimReport) {
    egui::CollapsingHeader::new(RichText::new("Probehände").strong())
        .id_salt("sim_view")
        .default_open(true)
        .show(ui, |ui| {
            ui.label(
                RichText::new(format!(
                    "{} simulierte Partien bis Zug {} · {} Karten, davon {} Länder",
                    r.games,
                    sim::TURNS,
                    r.size,
                    r.lands
                ))
                .weak(),
            );
            for (i, a) in r.advice.iter().enumerate() {
                let color = if a.contains("rund") { GREEN } else { YELLOW };
                let text = RichText::new(a).color(color);
                ui.label(if i == 0 { text.strong() } else { text });
            }
            let pct = |x: f32| format!("{:.0} %", x * 100.0);
            ui.label(format!(
                "Land rechtzeitig (mind. so viele Länder wie der Zug): Zug 1 {} · Zug 2 {} · Zug 3 {} · Zug 4 {}",
                pct(r.on_curve[0]),
                pct(r.on_curve[1]),
                pct(r.on_curve[2]),
                pct(r.on_curve[3])
            ));
            ui.label(format!(
                "Mulligan {} · Farbe fehlte {} · Länderflut {}",
                pct(r.mulligan),
                pct(r.color_screw),
                pct(r.flood)
            ));
            let mana: Vec<String> = (0..sim::TURNS)
                .map(|t| format!("Z{} {:.1}/{:.1}", t + 1, r.mana_used[t], r.mana_avail[t]))
                .collect();
            ui.label(RichText::new(format!("Mana genutzt/verfügbar: {}", mana.join(" · "))).weak());
        });
}

/// Fehlende Wildcards als Text, z. B. „2 Rare, 1 Mythic“.
pub(super) fn wildcards_text(short: &[u32; 4]) -> String {
    let parts: Vec<String> = short
        .iter()
        .enumerate()
        .filter(|(_, n)| **n > 0)
        .map(|(i, n)| format!("{n} {}", RARITY_NAMES[i]))
        .collect();
    if parts.is_empty() {
        "keine".into()
    } else {
        parts.join(", ")
    }
}

/// Wildcard-Plan: jetzt herstellbar und die Reihenfolge für den Rest.
pub(super) fn wildcard_plan_view(ui: &mut egui::Ui, plan: &budget::WildcardPlan) {
    egui::CollapsingHeader::new(RichText::new("Wildcard-Plan").strong())
        .id_salt("wildcard_plan")
        .default_open(false)
        .show(ui, |ui| {
            ui.label(format!("Schon vorhanden: {:.0} % des Decks.", plan.complete));
            if !plan.now.is_empty() {
                ui.label(RichText::new("Jetzt mit deinen Wildcards herstellen:").color(GREEN).strong());
                for s in &plan.now {
                    ui.label(format!("• {}× {} ({})", s.qty, s.name, RARITY_NAMES[s.slot]));
                }
                if let Some(last) = plan.now.last() {
                    ui.label(RichText::new(format!("Danach ist das Deck zu {:.0} % komplett.", last.complete)).weak());
                }
            }
            if !plan.later.is_empty() {
                ui.label(RichText::new("Danach, in dieser Reihenfolge:").strong());
                for s in &plan.later {
                    ui.label(format!(
                        "• {}× {} ({}) – dafür fehlen insgesamt noch: {} → {:.0} %",
                        s.qty,
                        s.name,
                        RARITY_NAMES[s.slot],
                        wildcards_text(&s.short),
                        s.complete
                    ));
                }
                ui.label(
                    RichText::new(
                        "Bis dahin ersetzt „Budget-Version bauen“ die fehlenden Karten durch ähnliche aus deiner Sammlung.",
                    )
                    .weak(),
                );
            }
            if !plan.unavailable.is_empty() {
                ui.label(
                    RichText::new(format!("Gibt es in Arena nicht: {}", plan.unavailable.join(", "))).color(YELLOW),
                );
            }
        });
}
