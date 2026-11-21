//! Judging single turns: key moments (the turn suggestion would have made
//! more of the mana) and cards that stayed affordable in hand for a long time.

use super::*;

/// From this many own turns on, an affordable card counts as "long in hand".
pub const HELD_TURNS: u32 = 3;

/// A card that stayed affordable in hand for a long time.
#[derive(Clone, Debug)]
pub struct HeldCard<'a> {
    pub card: &'a Card,
    /// Own turns in which it was affordable.
    pub turns: u32,
    /// Last of these turns.
    pub last_turn: u32,
}

/// Cards that were affordable in hand for at least [`HELD_TURNS`] own turns
/// (by card name). Instants (e.g. protection tricks) are often held back on
/// purpose – they don't count. Affordable means: mana value at most the
/// number of own lands (without colors).
pub fn held_cards<'a>(g: &GameRecord, db: &'a CardDb) -> Vec<HeldCard<'a>> {
    let mut held: Vec<HeldCard> = Vec::new();
    // The last turn doesn't count: the game ended in it.
    let done = g.turns.len().saturating_sub(1);
    for t in g.turns[..done].iter().filter(|t| t.mine) {
        let mut counted = HashSet::new();
        let playable = crate::turnplan::without_legend_duplicates(db, &t.hand, &t.board);
        for c in playable.iter().filter_map(|grp| card(db, *grp)) {
            let castable = !c.is_land() && !c.is_instant() && c.cmc > 0 && c.cmc <= t.lands_me;
            if !castable || !counted.insert(&c.name) {
                continue;
            }
            match held.iter_mut().find(|h| h.card.name == c.name) {
                Some(h) => {
                    h.turns += 1;
                    h.last_turn = t.turn;
                }
                None => held.push(HeldCard {
                    card: c,
                    turns: 1,
                    last_turn: t.turn,
                }),
            }
        }
    }
    held.retain(|h| h.turns >= HELD_TURNS);
    held
}

/// The turn suggestion must have used at least this much more mana for a
/// turn to count as "better play possible".
pub(super) const KEY_MOMENT_MANA: u32 = 2;

/// A turn in which the turn suggestion would have used clearly more mana.
#[derive(Clone, Debug, PartialEq)]
pub struct KeyMoment {
    pub text: String,
    /// The suggested cards.
    pub cards: Vec<GrpId>,
}

/// Compares an own turn with the turn suggestion: hand at the start of the
/// turn (hand at the end plus cast cards) and lands at the end of the turn.
pub fn key_moment(t: &TurnStat, db: &CardDb) -> Option<KeyMoment> {
    if !t.mine || t.lands.is_empty() {
        return None;
    }
    // Hand at the start of the turn; legendary cards already in play drop out.
    let mut hand = crate::turnplan::without_legend_duplicates(db, &t.hand, &t.board);
    hand.extend(&t.cast);
    let plan = crate::turnplan::plan(
        db,
        &hand,
        &t.lands,
        &[],
        true,
        &t.opp_creatures,
        t.creatures_at_start,
    );
    let used: u32 = t
        .cast
        .iter()
        .filter_map(|g| card(db, *g))
        .map(|c| c.cmc)
        .sum();
    if plan.spells.is_empty() || plan.mana_used < used + KEY_MOMENT_MANA {
        return None;
    }
    let names: Vec<String> = plan
        .spells
        .iter()
        .filter_map(|g| card(db, *g))
        .map(|c| c.display_name().to_string())
        .collect();
    let text = tr!(
        "games_moments.mana_used_possible_mana",
        used = used,
        mana_total = plan.mana_total,
        v = names.join(&tr!("common.then")),
        mana_used = plan.mana_used
    );
    Some(KeyMoment {
        text,
        cards: plan.spells,
    })
}
