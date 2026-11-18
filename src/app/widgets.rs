//! Wiederverwendbare Anzeige-Bausteine: Probehände und Wildcard-Plan.

use super::*;

/// Ergebnis der Probehände: Empfehlungen und Kennzahlen.
pub(super) fn sim_view(ui: &mut egui::Ui, t: &sim::Trial) {
    let r = &t.fast;
    egui::CollapsingHeader::new(RichText::new("Probehände").strong())
        .id_salt("sim_view")
        .default_open(true)
        .show(ui, |ui| {
            ui.label(
                RichText::new(format!(
                    "{} simulierte Partien bis Zug {} · {} Karten, davon {} Länder",
                    r.games,
                    r.turns,
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
            let p = &r.pressure;
            ui.label(format!(
                "Gegen einen schnellen Gegner bis Zug {}: vorne in {} (davon gewonnen {}) · verloren in {} · \
                 im Schnitt {:.0} Leben bei dir, {:.0} beim Gegner · {:.1} gegnerische Kreaturen ausgeschaltet",
                r.turns,
                pct(p.ahead),
                pct(p.won),
                pct(p.dead),
                p.life[r.turns - 1],
                p.opp_life[r.turns - 1],
                p.kills
            ))
            .on_hover_text(
                "Der Gegner spielt in den Zügen 1–6 je eine Kreatur (2/1, 2/2, 3/2, 3/3, 4/3, 4/4), danach 2/2en, \
                 und greift mit allem an. \
                 Du blockst wie im Begleitfenster vorgeschlagen, setzt Entfernung auf die stärkste Kreatur und \
                 greifst an, wenn es tödlich ist oder gefahrlos und du den Gegenschlag überlebst.",
            );
            personal_view(ui, t);
            let mana: Vec<String> = (0..r.turns)
                .map(|t| format!("Z{} {:.1}/{:.1}", t + 1, r.mana_used[t], r.mana_avail[t]))
                .collect();
            ui.label(RichText::new(format!("Mana genutzt/verfügbar: {}", mana.join(" · "))).weak());
        });
}

/// Probehände gegen die eigenen Gegner: mit Zugvorschlag und so, wie man spielt.
fn personal_view(ui: &mut egui::Ui, t: &sim::Trial) {
    let pct = |x: f32| format!("{:.0} %", x * 100.0);
    let Some(best) = &t.mine else {
        ui.label(
            RichText::new(format!(
                "Für einen Vergleich mit deinen echten Gegnern braucht es mindestens {} vollständig \
                 mitgeschriebene Partien.",
                sim::MIN_PROFILE_GAMES
            ))
            .weak(),
        );
        return;
    };
    ui.add_space(4.0);
    ui.label(
        RichText::new(format!(
            "Gegen {} – aus {} deiner Partien:",
            best.opponent, t.profile_games
        ))
        .strong(),
    );
    ui.label(format!(
        "Mit Zugvorschlag: nach Zug {} vorne in {} · verloren in {} · {:.0} Leben bei dir, {:.0} beim Gegner",
        best.turns,
        pct(best.pressure.ahead),
        pct(best.pressure.dead),
        best.pressure.life[best.turns - 1],
        best.pressure.opp_life[best.turns - 1]
    ));
    match (&t.mine_habits, &t.habits) {
        (Some(yours), Some(h)) => {
            let gap = (best.pressure.ahead - yours.pressure.ahead) * 100.0;
            ui.label(format!(
                "So wie du spielst: vorne in {} · verloren in {} – aus {} deiner Züge: besserer Zug möglich in {}, \
                 gefahrlosen Angriff ausgelassen in {}",
                pct(yours.pressure.ahead),
                pct(yours.pressure.dead),
                h.turns,
                pct(h.weaker_turn),
                pct(h.skip_attack)
            ));
            let text = if gap >= 1.0 {
                format!(
                    "Mit dem Zugvorschlag des Begleitfensters liegst du in {gap:.0} % mehr Partien vorne – \
                     so viel holst du an diesem Deck allein durchs Spiel heraus."
                )
            } else {
                "Du spielst dieses Deck schon fast so gut wie der Zugvorschlag.".into()
            };
            ui.label(RichText::new(text).color(if gap >= 1.0 { YELLOW } else { GREEN }));
        }
        _ => {
            ui.label(
                RichText::new(format!(
                    "Für „so wie du spielst“ braucht es mindestens {} ausgewertete eigene Züge.",
                    sim::MIN_HABIT_TURNS
                ))
                .weak(),
            );
        }
    }
    ui.label(
        RichText::new(format!(
            "Die Simulation endet nach Zug {} – längere Partien entscheiden sich manchmal noch später.",
            best.turns
        ))
            .weak(),
    );
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
