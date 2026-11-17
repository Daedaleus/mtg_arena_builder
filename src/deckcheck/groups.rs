//! Fasst die Partien nach Deckliste zusammen.

use super::*;

pub(super) fn event_label(event: &str) -> &str {
    match event {
        "Ladder" | "Traditional_Ladder" => "Rangliste",
        "Play" | "Traditional_Play" => "Spielen",
        "DualColorPrecons" => "Starterdeck",
        e if e.starts_with("WelcomeDeckDuels") => "Willkommens-Duell",
        e if e.starts_with("ColorChallenge") => "Farb-Challenge",
        _ => "Sonstiges",
    }
}

pub(super) fn is_fixed_event(event: &str) -> bool {
    event.starts_with("WelcomeDeckDuels")
        || event.starts_with("ColorChallenge")
        || event == "DualColorPrecons"
}

/// Fasst die Partien nach Deckliste zusammen, zuletzt gespieltes Deck zuerst.
pub fn group_decks(games: &[GameRecord], db: &CardDb) -> Vec<DeckGroup> {
    let mut groups: Vec<DeckGroup> = Vec::new();
    // Gleiche Karte in anderem Druck = gleiches Deck: Schlüssel über die Namen.
    let key_of = |deck: &[u32]| -> Vec<String> {
        let mut k: Vec<String> = deck
            .iter()
            .map(|g| {
                db.cards
                    .get(g)
                    .map(|c| name_key(&c.name))
                    .unwrap_or_else(|| g.to_string())
            })
            .collect();
        k.sort();
        k
    };
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
        grp.label = format!(
            "{} {} – {} ({} Karten, {} Partien)",
            if colors.is_empty() {
                "?"
            } else {
                colors.as_str()
            },
            event_label(&first.event),
            signature,
            grp.deck.len(),
            grp.games.len()
        );
    }
    groups
}
