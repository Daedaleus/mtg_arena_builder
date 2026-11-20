//! Einzelne Züge bewerten: Schlüsselmomente (der Zugvorschlag hätte mehr aus
//! dem Mana gemacht) und Karten, die lange bezahlbar auf der Hand lagen.

use super::*;

/// Ab so vielen eigenen Zügen gilt eine bezahlbare Karte als „lange auf der Hand“.
pub const HELD_TURNS: u32 = 3;

/// Eine Karte, die lange bezahlbar auf der Hand lag.
#[derive(Clone, Debug)]
pub struct HeldCard<'a> {
    pub card: &'a Card,
    /// Eigene Züge, in denen sie bezahlbar war.
    pub turns: u32,
    /// Letzter dieser Züge.
    pub last_turn: u32,
}

/// Karten, die mindestens [`HELD_TURNS`] eigene Züge bezahlbar auf der Hand
/// lagen (nach Kartenname). Spontanzauber (z. B. Schutz-Tricks) hält man oft
/// bewusst zurück – die zählen nicht. Bezahlbar heißt: Manawert höchstens die
/// Zahl der eigenen Länder (ohne Farben).
pub fn held_cards<'a>(g: &GameRecord, db: &'a CardDb) -> Vec<HeldCard<'a>> {
    let mut held: Vec<HeldCard> = Vec::new();
    // Der letzte Zug zählt nicht: Die Partie endete darin.
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

/// Mindestens so viel Mana mehr hätte der Zugvorschlag nutzen müssen,
/// damit ein Zug als „besserer Zug möglich“ gilt.
pub(super) const KEY_MOMENT_MANA: u32 = 2;

/// Vergleicht einen eigenen Zug mit dem Zugvorschlag: Hand zu Zugbeginn
/// (Hand am Ende plus gewirkte Karten) und Länder am Zugende.
pub fn key_moment(t: &TurnStat, db: &CardDb) -> Option<String> {
    if !t.mine || t.lands.is_empty() {
        return None;
    }
    // Hand zu Zugbeginn; legendäre Karten, die schon im Spiel sind, fallen weg.
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
    Some(tr!(
        "games_moments.mana_genutzt_moeglich_mana",
        used = used,
        mana_total = plan.mana_total,
        v = names.join(&tr!("common.dann")),
        mana_used = plan.mana_used
    ))
}
