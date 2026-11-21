//! Computes the companion's display from the game state.

use super::*;

pub(super) fn phase_label(phase: Phase) -> String {
    match phase {
        Phase::Beginning => tr!("live_view.beginning"),
        Phase::Main1 => tr!("live_view.main_phase_1"),
        Phase::Combat => tr!("live_view.combat"),
        Phase::Main2 => tr!("live_view.main_phase_2"),
        Phase::Ending => tr!("live_view.end"),
        Phase::Unknown => String::new(),
    }
}

pub(super) fn names(db: &CardDb, grps: &[GrpId], limit: usize) -> String {
    let mut list: Vec<String> = Vec::new();
    for g in grps {
        if let Some(c) = db.cards.get(g)
            && !list.iter().any(|n| n == c.display_name())
        {
            list.push(c.display_name().to_string());
        }
    }
    let more = list.len().saturating_sub(limit);
    list.truncate(limit);
    let mut text = list.join(", ");
    if more > 0 {
        text.push_str(&tr!("live_view.text", more = more));
    }
    text
}

/// Recognises the opposing deck from the cards seen so far.
pub(super) fn guess_opponent<'a>(
    cards: &[GrpId],
    db: &CardDb,
    meta: &'a [ArchShare],
) -> (String, Option<&'a ArchShare>) {
    let seen = crate::opponents::seen_keys(cards, db);
    match crate::opponents::match_archetype(&seen, meta) {
        Some(a) => (tr!("live_view.probably", name = a.name), Some(a)),
        None => {
            let ids: Vec<GrpId> = cards.to_vec();
            let colors = crate::games::deck_colors(&ids, db);
            let label = if colors.is_empty() {
                tr!("live_view.still_unknown")
            } else {
                crate::carddb::color::label(&colors)
            };
            (label, None)
        }
    }
}

/// Shows a finished game for this long.
const SHOW_LAST_GAME: Duration = Duration::from_secs(180);

pub fn build_view(
    live: Option<&LiveState>,
    last: Option<&(GameRecord, Instant)>,
    db: &CardDb,
    meta: &[ArchShare],
) -> CompanionView {
    let Some(ls) = live else {
        return idle_view(
            last.filter(|(_, at)| at.elapsed() < SHOW_LAST_GAME)
                .map(|(g, _)| g),
        );
    };
    let mut v = CompanionView {
        life: Some((ls.life_me, ls.life_opp)),
        board: tr!(
            "live_view.lands_free_creatures_opponent",
            lands_me = ls.lands_me,
            lands_untapped = ls.lands_untapped,
            creatures_me = ls.creatures_me,
            creatures_opp = ls.creatures_opp
        ),
        ..Default::default()
    };
    let (opponent, arch) = guess_opponent(&ls.opp_cards, db, meta);
    let hand: Vec<&Card> = ls.hand.iter().filter_map(|g| db.cards.get(g)).collect();
    if ls.mulligan {
        v.status = tr!("live_view.opening_hand");
        mulligan_hint(&mut v, &hand);
    } else {
        v.status = turn_status(ls);
        let lethal = lethal_hint(&mut v, ls, db, &hand);
        if !lethal && ls.life_opp <= 5 {
            v.hints.push((
                Level::Good,
                tr!("live_view.opponent_only_life_left", life_opp = ls.life_opp),
            ));
        }
        if ls.my_turn {
            my_turn_hints(&mut v, ls, db);
        } else if ls.opp_attackers > 0 {
            defense_hints(&mut v, ls, db);
        }
        trick_warning(&mut v, ls, db, arch);
        if v.hints.is_empty() {
            v.hints
                .push((Level::Info, tr!("live_view.nothing_watch_out_right")));
        }
    }
    v.opponent = opponent;
    v.opp_cards = opp_card_names(ls, db);
    v
}

/// No running game: result of the last one or waiting.
fn idle_view(last: Option<&GameRecord>) -> CompanionView {
    let mut v = CompanionView::default();
    match last {
        Some(g) => {
            v.status = match g.won {
                Some(true) => tr!("live_view.game_over_won"),
                Some(false) => tr!("live_view.game_over_lost"),
                None => tr!("live_view.game_over"),
            };
            v.hints.push((
                Level::Info,
                tr!("live_view.analysis_available_shortly_under"),
            ));
        }
        None => {
            v.status = tr!("live_view.waiting_game");
            v.hints
                .push((Level::Info, tr!("live_view.tip_play_arena_windowed")));
        }
    }
    v
}

/// Keep the opening hand or mulligan?
fn mulligan_hint(v: &mut CompanionView, hand: &[&Card]) {
    if hand.len() < 6 {
        return;
    }
    let n = hand.iter().filter(|c| c.is_land()).count();
    if crate::games::GOOD_KEEP_LANDS.contains(&n) {
        v.hints.push((
            Level::Good,
            tr!("live_view.lands_cards_keep", n = n, hand_n = hand.len()),
        ));
    } else {
        v.hints.push((
            Level::Warn,
            tr!(
                "live_view.lands_cards_mulligan_recommended",
                n = n,
                hand_n = hand.len()
            ),
        ));
    }
}

/// "Turn 5 · your turn · combat"
fn turn_status(ls: &LiveState) -> String {
    let whose = if ls.my_turn {
        tr!("live_view.turn_3")
    } else {
        tr!("live_view.opponents_turn")
    };
    let phase = phase_label(ls.phase);
    if phase.is_empty() {
        tr!("live_view.turn", turn = ls.turn, whose = whose)
    } else {
        tr!(
            "live_view.turn_2",
            turn = ls.turn,
            whose = whose,
            phase = phase
        )
    }
}

/// Chances to win: can I defeat the opponent now? Returns `true` if a lethal
/// attack is possible.
fn lethal_hint(v: &mut CompanionView, ls: &LiveState, db: &CardDb, hand: &[&Card]) -> bool {
    if !ls.my_turn || ls.phase == Phase::Ending || ls.life_opp <= 0 {
        return false;
    }
    let extra = crate::turnplan::sources(db, &[], &ls.mana_permanents).len() as u32;
    let land_to_play = !ls.land_played && hand.iter().any(|c| c.is_land());
    let mana = ls.lands_untapped + extra + land_to_play as u32;
    let combat = unblocked_damage(&ls.attackers, &ls.blockers);
    let (burn, burn_cards) = best_burn(hand, mana);
    let total = combat + burn as i32;
    let burn_text = if burn > 0 {
        tr!("live_view.burn", burn = burn, v = burn_cards.join(", "))
    } else {
        String::new()
    };
    if total >= ls.life_opp {
        v.hints.push((
            Level::Good,
            tr!(
                "live_view.lethal_possible_attacking_everything",
                combat = combat,
                burn_text = burn_text,
                life_opp = ls.life_opp
            ),
        ));
        return true;
    }
    if total > 0 && ls.life_opp <= total + 4 {
        v.hints.push((
            Level::Info,
            tr!(
                "live_view.opponent_almost_beaten_life",
                life_opp = ls.life_opp,
                total = total,
                combat = combat,
                burn_text = burn_text
            ),
        ));
    }
    false
}

/// Own turn: safe attackers and, in the main phases, the turn suggestion.
fn my_turn_hints(v: &mut CompanionView, ls: &LiveState, db: &CardDb) {
    let attackers = worth_attacking(ls, db);
    if !attackers.is_empty() {
        v.hints.push((
            Level::Good,
            tr!("live_view.attack_safely", v = names(db, &attackers, 4)),
        ));
    }
    if matches!(
        ls.phase,
        Phase::Beginning | Phase::Main1 | Phase::Main2 | Phase::Unknown
    ) {
        turn_hints(v, ls, db);
    }
}

/// The opponent attacks: instant removal and block suggestion.
fn defense_hints(v: &mut CompanionView, ls: &LiveState, db: &CardDb) {
    let removal =
        crate::turnplan::instant_removal(db, &ls.hand, &ls.untapped_lands, &ls.mana_permanents);
    if !removal.is_empty() {
        v.hints.push((
            Level::Good,
            tr!(
                "live_view.castable_instant_speed_now",
                v = names(db, &removal, 2)
            ),
        ));
    }
    block_hints(v, ls, db);
}

/// The opponent's open mana before combat: typical tricks of their deck.
fn trick_warning(v: &mut CompanionView, ls: &LiveState, db: &CardDb, arch: Option<&ArchShare>) {
    let combat_ahead = if ls.my_turn {
        matches!(ls.phase, Phase::Main1 | Phase::Combat)
    } else {
        ls.opp_attackers > 0
    };
    if !combat_ahead || ls.opp_untapped_lands == 0 {
        return;
    }
    let tricks = crate::combat::likely_tricks(db, arch, &ls.opp_cards, ls.opp_untapped_lands);
    if tricks.is_empty() {
        return;
    }
    let list: Vec<String> = tricks
        .iter()
        .take(3)
        .map(|(n, share)| match share {
            Some(s) => format!("{n} ({:.0} %)", s * 100.0),
            None => tr!("live_view.already_seen", n = n),
        })
        .collect();
    v.hints.push((
        Level::Warn,
        tr!(
            "live_view.careful_opponent_mana_open",
            opp_untapped_lands = ls.opp_untapped_lands,
            v = list.join(", ")
        ),
    ));
}

/// The opponent's non-land cards seen, alphabetically.
fn opp_card_names(ls: &LiveState, db: &CardDb) -> Vec<String> {
    let mut seen: Vec<String> = ls
        .opp_cards
        .iter()
        .filter_map(|g| db.cards.get(g))
        .filter(|c| !c.is_land())
        .map(|c| c.display_name().to_string())
        .collect();
    seen.sort();
    seen.dedup();
    seen
}
