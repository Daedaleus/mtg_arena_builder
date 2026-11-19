//! Wiederverwendbare Anzeige-Bausteine: Probehände und Wildcard-Plan.

use super::*;

/// Ergebnis der Probehände: Empfehlungen und Kennzahlen.
pub(super) fn sim_view(ui: &mut egui::Ui, t: &sim::Trial) {
    let r = &t.fast;
    egui::CollapsingHeader::new(RichText::new(tr!("widgets.probehaende")).strong())
        .id_salt("sim_view")
        .default_open(true)
        .show(ui, |ui| {
            ui.label(
                RichText::new(tr!("widgets.simulierte_partien_bis_zug", games = r.games, turns = r.turns, size = r.size, lands = r.lands))
                .weak(),
            );
            for (i, a) in r.advice.iter().enumerate() {
                let color = if r.smooth { GREEN } else { YELLOW };
                let text = RichText::new(a).color(color);
                ui.label(if i == 0 { text.strong() } else { text });
            }
            let pct = |x: f32| format!("{:.0} %", x * 100.0);
            ui.label(tr!("widgets.land_rechtzeitig_mind_so", v = pct(r.on_curve[0]), v2 = pct(r.on_curve[1]), v3 = pct(r.on_curve[2]), v4 = pct(r.on_curve[3])));
            ui.label(tr!("widgets.mulligan_farbe_fehlte_laenderflut", v = pct(r.mulligan), v2 = pct(r.color_screw), v3 = pct(r.flood)));
            let p = &r.pressure;
            ui.label(tr!("widgets.gegen_einen_schnellen_gegner", turns = r.turns, v = pct(p.ahead), v2 = pct(p.won), v3 = pct(p.dead), life = p.life[r.turns - 1] : {:.0}, opp_life = p.opp_life[r.turns - 1] : {:.0}, kills = p.kills : {:.1}))
            .on_hover_text(
                tr!("widgets.gegner_spielt_zuegen_1"),
            );
            personal_view(ui, t);
            let mana: Vec<String> = (0..r.turns)
                .map(|t| format!("Z{} {:.1}/{:.1}", t + 1, r.mana_used[t], r.mana_avail[t]))
                .collect();
            ui.label(RichText::new(tr!("widgets.mana_genutzt_verfuegbar", v = mana.join(" · "))).weak());
        });
}

/// Probehände gegen die eigenen Gegner: mit Zugvorschlag und so, wie man spielt.
fn personal_view(ui: &mut egui::Ui, t: &sim::Trial) {
    let pct = |x: f32| format!("{:.0} %", x * 100.0);
    let Some(best) = &t.mine else {
        ui.label(
            RichText::new(tr!(
                "widgets.einen_vergleich_deinen_echten",
                min_profile_games = sim::MIN_PROFILE_GAMES
            ))
            .weak(),
        );
        return;
    };
    ui.add_space(4.0);
    ui.label(
        RichText::new(tr!(
            "widgets.gegen_aus_deiner_partien",
            opponent = best.opponent,
            profile_games = t.profile_games
        ))
        .strong(),
    );
    if let Some(o) = &t.opponent {
        ui.label(
            RichText::new(tr!(
                "widgets.sie_entfernen_ihrer_zuege",
                v = pct(o.removal),
                v2 = pct(o.tricks)
            ))
            .weak(),
        );
    }
    ui.label(tr!("widgets.zugvorschlag_nach_zug_vorne", turns = best.turns, v = pct(best.pressure.ahead), v2 = pct(best.pressure.dead), life = best.pressure.life[best.turns - 1] : {:.0}, opp_life = best.pressure.opp_life[best.turns - 1] : {:.0}));
    match (&t.mine_habits, &t.habits) {
        (Some(yours), Some(h)) => {
            let gap = (best.pressure.ahead - yours.pressure.ahead) * 100.0;
            ui.label(tr!(
                "widgets.so_wie_spielst_vorne",
                v = pct(yours.pressure.ahead),
                v2 = pct(yours.pressure.dead),
                turns = h.turns,
                v3 = pct(h.weaker_turn),
                v4 = pct(h.skip_attack)
            ));
            let text = if gap >= 1.0 {
                tr!("widgets.zugvorschlag_begleitfensters_liegst_mehr", gap = gap : {:.0})
            } else {
                tr!("widgets.spielst_dieses_deck_schon")
            };
            ui.label(RichText::new(text).color(if gap >= 1.0 { YELLOW } else { GREEN }));
        }
        _ => {
            ui.label(
                RichText::new(tr!(
                    "widgets.so_wie_spielst_braucht",
                    min_habit_turns = sim::MIN_HABIT_TURNS
                ))
                .weak(),
            );
        }
    }
    ui.label(RichText::new(tr!("widgets.simulation_endet_nach_zug", turns = best.turns)).weak());
}

/// Fehlende Wildcards als Text, z. B. „2 Rare, 1 Mythic“.
pub(super) fn wildcards_text(short: &[u32; 4]) -> String {
    let parts: Vec<String> = short
        .iter()
        .enumerate()
        .filter(|(_, n)| **n > 0)
        .map(|(i, n)| format!("{n} {}", Rarity::WILDCARDS[i].label()))
        .collect();
    if parts.is_empty() {
        tr!("common.keine")
    } else {
        parts.join(", ")
    }
}

/// Wildcard-Plan: jetzt herstellbar und die Reihenfolge für den Rest.
pub(super) fn wildcard_plan_view(ui: &mut egui::Ui, plan: &budget::WildcardPlan, db: &CardDb) {
    egui::CollapsingHeader::new(RichText::new(tr!("widgets.wildcard_plan")).strong())
        .id_salt("wildcard_plan")
        .default_open(false)
        .show(ui, |ui| {
            ui.label(tr!("widgets.schon_vorhanden_decks", complete = plan.complete : {:.0}));
            if !plan.now.is_empty() {
                ui.label(RichText::new(tr!("widgets.jetzt_deinen_wildcards_herstellen")).color(GREEN).strong());
                for s in &plan.now {
                    ui.label(format!("• {}× {} ({})", s.qty, db.display_name(&s.name), Rarity::WILDCARDS[s.slot].label()));
                }
                if let Some(last) = plan.now.last() {
                    ui.label(RichText::new(tr!("widgets.danach_deck_komplett", complete = last.complete : {:.0})).weak());
                }
            }
            if !plan.later.is_empty() {
                ui.label(RichText::new(tr!("widgets.danach_dieser_reihenfolge")).strong());
                for s in &plan.later {
                    ui.label(tr!("widgets.dafuer_fehlen_insgesamt_noch", qty = s.qty, name = db.display_name(&s.name), rarity_names = Rarity::WILDCARDS[s.slot].label(), v = wildcards_text(&s.short), complete = s.complete : {:.0}));
                }
                ui.label(
                    RichText::new(
                        tr!("widgets.bis_dahin_ersetzt_budget"),
                    )
                    .weak(),
                );
            }
            if !plan.unavailable.is_empty() {
                ui.label(
                    RichText::new(tr!("widgets.gibt_arena_nicht", v = plan.unavailable.join(", "))).color(YELLOW),
                );
            }
        });
}
