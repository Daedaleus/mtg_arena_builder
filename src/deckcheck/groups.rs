//! Fasst die Partien nach Deckliste zusammen.

use super::*;

use crate::games::arena::event;

pub(super) fn event_label(name: &str) -> String {
    match name {
        event::LADDER | event::LADDER_BO3 => tr!("deckcheck_groups.rangliste"),
        event::PLAY | event::PLAY_BO3 => tr!("deckcheck_groups.spielen"),
        event::STARTER_DECKS => tr!("deckcheck_groups.starterdeck"),
        e if e.starts_with(event::WELCOME_DUELS_PREFIX) => {
            tr!("deckcheck_groups.willkommens_duell")
        }
        e if e.starts_with(event::COLOR_CHALLENGE_PREFIX) => tr!("deckcheck_groups.farb_challenge"),
        _ => tr!("deckcheck_groups.sonstiges"),
    }
}

pub(super) fn is_fixed_event(name: &str) -> bool {
    event::fixed_deck(name)
}

/// Fasst die Partien nach Deckliste zusammen, zuletzt gespieltes Deck zuerst.
pub fn group_decks(games: &[GameRecord], db: &CardDb) -> Vec<DeckGroup> {
    let mut groups: Vec<DeckGroup> = Vec::new();
    // Gleiche Karte in anderem Druck = gleiches Deck: Schlüssel über die Namen.
    let key_of = |deck: &[GrpId]| crate::games::deck_key(deck, db);
    let mut keys: Vec<Vec<String>> = Vec::new();
    for (i, g) in games.iter().enumerate() {
        if g.deck.is_empty() {
            continue;
        }
        let key = key_of(&g.deck);
        let won = (g.won == Some(true)) as usize;
        match keys.iter().position(|k| *k == key) {
            Some(pos) => {
                groups[pos].games.push(i);
                groups[pos].wins += won;
                groups[pos].fixed &= is_fixed_event(&g.event);
            }
            None => {
                keys.push(key);
                groups.push(DeckGroup {
                    label: String::new(),
                    deck: g.deck.clone(),
                    games: vec![i],
                    wins: won,
                    fixed: is_fixed_event(&g.event),
                });
            }
        }
    }
    for grp in &mut groups {
        let first = &games[grp.games[0]];
        let colors = crate::games::deck_colors(&grp.deck, db);
        // Bekannteste Karte als Erkennungszeichen: seltenste Nichtland-Karte.
        let signature = grp
            .deck
            .iter()
            .filter_map(|g| db.cards.get(g))
            .filter(|c| !c.is_land())
            .max_by_key(|c| (c.rarity, c.cmc))
            .map(|c| c.name.clone())
            .unwrap_or_default();
        grp.label = tr!(
            "deckcheck_groups.karten_partien",
            v = if colors.is_empty() {
                "?"
            } else {
                colors.as_str()
            },
            v2 = event_label(&first.event),
            signature = signature,
            deck_n = grp.deck.len(),
            games_n = grp.games.len()
        );
    }
    groups
}
