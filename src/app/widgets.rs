//! Reusable display building blocks: sample hands and wildcard plan.

use super::*;

/// Result of the sample hands: recommendations and key figures.
pub(super) fn sim_view(ui: &mut egui::Ui, t: &sim::Trial) {
    let r = &t.fast;
    egui::CollapsingHeader::new(RichText::new(tr!("widgets.sample_hands")).strong())
        .id_salt("sim_view")
        .default_open(true)
        .show(ui, |ui| {
            ui.label(
                RichText::new(tr!("widgets.simulated_games_up_turn", games = r.games, turns = r.turns, size = r.size, lands = r.lands))
                .weak(),
            );
            for (i, a) in r.advice.iter().enumerate() {
                let color = if r.smooth { theme::tone(ui.ctx(), Tone::Good) } else { theme::tone(ui.ctx(), Tone::Warn) };
                let text = RichText::new(a).color(color);
                ui.label(if i == 0 { text.strong() } else { text });
            }
            let pct = |x: f32| format!("{:.0} %", x * 100.0);
            ui.label(tr!("widgets.land_time_least_many", v = pct(r.on_curve[0]), v2 = pct(r.on_curve[1]), v3 = pct(r.on_curve[2]), v4 = pct(r.on_curve[3])));
            ui.label(tr!("widgets.mulligan_color_missing_flood", v = pct(r.mulligan), v2 = pct(r.color_screw), v3 = pct(r.flood)));
            let p = &r.pressure;
            ui.label(tr!("widgets.against_fast_opponent_up", turns = r.turns, v = pct(p.ahead), v2 = pct(p.won), v3 = pct(p.dead), life = p.life[r.turns - 1] : {:.0}, opp_life = p.opp_life[r.turns - 1] : {:.0}, kills = p.kills : {:.1}))
            .on_hover_text(
                tr!("widgets.turns_1_6_opponent"),
            );
            personal_view(ui, t);
            let mana: Vec<String> = (0..r.turns)
                .map(|t| format!("Z{} {:.1}/{:.1}", t + 1, r.mana_used[t], r.mana_avail[t]))
                .collect();
            note(ui, tr!("widgets.mana_used_available", v = mana.join(" · ")));
        });
}

/// Sample hands against the own opponents: with the turn suggestion and the way you play.
fn personal_view(ui: &mut egui::Ui, t: &sim::Trial) {
    let pct = |x: f32| format!("{:.0} %", x * 100.0);
    let Some(best) = &t.mine else {
        ui.label(
            RichText::new(tr!(
                "widgets.comparison_real_opponents_needs",
                min_profile_games = sim::MIN_PROFILE_GAMES
            ))
            .weak(),
        );
        return;
    };
    ui.add_space(4.0);
    ui.label(
        RichText::new(tr!(
            "widgets.against_games",
            opponent = best.opponent,
            profile_games = t.profile_games
        ))
        .strong(),
    );
    if let Some(o) = &t.opponent {
        ui.label(
            RichText::new(tr!(
                "widgets.they_remove_one_creatures",
                v = pct(o.removal),
                v2 = pct(o.tricks)
            ))
            .weak(),
        );
    }
    ui.label(tr!("widgets.turn_suggestion_after_turn", turns = best.turns, v = pct(best.pressure.ahead), v2 = pct(best.pressure.dead), life = best.pressure.life[best.turns - 1] : {:.0}, opp_life = best.pressure.opp_life[best.turns - 1] : {:.0}));
    match (&t.mine_habits, &t.habits) {
        (Some(yours), Some(h)) => {
            let gap = (best.pressure.ahead - yours.pressure.ahead) * 100.0;
            ui.label(tr!(
                "widgets.way_play_ahead_lost",
                v = pct(yours.pressure.ahead),
                v2 = pct(yours.pressure.dead),
                turns = h.turns,
                v3 = pct(h.weaker_turn),
                v4 = pct(h.skip_attack)
            ));
            let text = if gap >= 1.0 {
                tr!("widgets.companions_turn_suggestion_ahead", gap = gap : {:.0})
            } else {
                tr!("widgets.already_play_deck_almost")
            };
            ui.label(RichText::new(text).color(if gap >= 1.0 {
                theme::tone(ui.ctx(), Tone::Warn)
            } else {
                theme::tone(ui.ctx(), Tone::Good)
            }));
        }
        _ => {
            ui.label(
                RichText::new(tr!(
                    "widgets.way_play_needs_least",
                    min_habit_turns = sim::MIN_HABIT_TURNS
                ))
                .weak(),
            );
        }
    }
    ui.label(
        RichText::new(tr!(
            "widgets.simulation_ends_after_turn",
            turns = best.turns
        ))
        .weak(),
    );
}

/// Missing wildcards as text, e.g. "2 Rare, 1 Mythic".
pub(super) fn wildcards_text(short: &[u32; 4]) -> String {
    let parts: Vec<String> = short
        .iter()
        .enumerate()
        .filter(|(_, n)| **n > 0)
        .map(|(i, n)| format!("{n} {}", Rarity::WILDCARDS[i].label()))
        .collect();
    if parts.is_empty() {
        tr!("common.none")
    } else {
        parts.join(", ")
    }
}

/// Wildcard plan: craftable now and the order for the rest.
pub(super) fn wildcard_plan_view(ui: &mut egui::Ui, plan: &budget::WildcardPlan, db: &CardDb) {
    egui::CollapsingHeader::new(RichText::new(tr!("widgets.wildcard_plan")).strong())
        .id_salt("wildcard_plan")
        .default_open(false)
        .show(ui, |ui| {
            ui.label(tr!("widgets.already_owned_deck", complete = plan.complete : {:.0}));
            if !plan.now.is_empty() {
                ui.label(RichText::new(tr!("widgets.craft_now_wildcards")).color(theme::tone(ui.ctx(), Tone::Good)).strong());
                for s in &plan.now {
                    ui.label(format!("• {}× {} ({})", s.qty, db.display_name(&s.name), Rarity::WILDCARDS[s.slot].label()));
                }
                if let Some(last) = plan.now.last() {
                    note(ui, tr!("widgets.after_deck_complete", complete = last.complete : {:.0}));
                }
            }
            if !plan.later.is_empty() {
                ui.label(RichText::new(tr!("widgets.after_order")).strong());
                for s in &plan.later {
                    ui.label(tr!("widgets.still_missing_total", qty = s.qty, name = db.display_name(&s.name), rarity_names = Rarity::WILDCARDS[s.slot].label(), v = wildcards_text(&s.short), complete = s.complete : {:.0}));
                }
                ui.label(
                    RichText::new(
                        tr!("widgets.until_build_budget_version"),
                    )
                    .weak(),
                );
            }
            if !plan.unavailable.is_empty() {
                ui.label(
                    RichText::new(tr!("widgets.available_arena", v = plan.unavailable.join(", "))).color(theme::tone(ui.ctx(), Tone::Warn)),
                );
            }
        });
}
