//! Brawl decks from the collection: for every owned commander (legendary
//! creature or planeswalker) a one-copy deck in its colour identity, the
//! best commander per colour combination.

use std::collections::{HashMap, HashSet};

use super::{Built, Pool, Shape, build_one, owned_cards};
use crate::carddb::{CardDb, color};
use crate::deck::DeckEntry;
use crate::games::GameFormat;

/// Historic Brawl: 99 cards besides the commander, 38 of them lands.
const BRAWL: Shape = Shape {
    spells: 61,
    lands: 38,
    max_copies: 1,
    caps: [[17, 24, 14, 7, 3, 0], [20, 27, 17, 12, 7, 3]],
    max_noncreature: 24,
    max_other: 8,
    min_creatures: 20,
    max_duals: 15,
};

/// Standard Brawl: 59 cards besides the commander, 24 of them lands.
const STANDARD_BRAWL: Shape = Shape {
    spells: 35,
    lands: 24,
    max_copies: 1,
    caps: [[10, 14, 8, 4, 2, 0], [12, 16, 10, 7, 4, 2]],
    max_noncreature: 14,
    max_other: 4,
    min_creatures: 12,
    max_duals: 8,
};

/// At most this many suggestions.
const MAX_DECKS: usize = 12;
/// Score deduction per color beyond two: more colors give more cards to
/// choose from, but a mana base of mostly basic lands can't pay them.
const COLOR_PENALTY: f32 = 0.2;

/// Brawl decks for `format` (Brawl or Standard Brawl), best first; one per
/// colour combination with its best commander. Empty for other formats.
pub fn build_brawl(
    db: &CardDb,
    owned: &HashMap<String, u32>,
    legal: Option<&HashSet<String>>,
    format: GameFormat,
) -> Vec<Built> {
    let shape = match format {
        GameFormat::Brawl => &BRAWL,
        GameFormat::StandardBrawl => &STANDARD_BRAWL,
        _ => return Vec::new(),
    };
    let meta_use = HashMap::new();
    let pool = Pool {
        db,
        owned,
        legal,
        meta_use: &meta_use,
    };
    let mut best: HashMap<u8, Built> = HashMap::new();
    let commanders = owned_cards(db, owned, legal, 1)
        .map(|(_, c, _)| c)
        .filter(|c| c.can_be_commander() && !c.back_face && c.colors != 0);
    for commander in commanders {
        let Some(mut built) = build_one(&pool, commander.colors, shape, Some(&commander.name))
        else {
            continue;
        };
        built.deck.main.insert(
            0,
            DeckEntry {
                qty: 1,
                name: commander.name.clone(),
            },
        );
        built.deck.commander = vec![commander.name.clone()];
        built.score -= COLOR_PENALTY * commander.colors.count_ones().saturating_sub(2) as f32;
        built.label = format!(
            "{} ({})",
            commander.display_name(),
            color::label(&color::letters(commander.colors))
        );
        let better = best
            .get(&commander.colors)
            .is_none_or(|b| built.score > b.score);
        if better {
            best.insert(commander.colors, built);
        }
    }
    let mut out: Vec<Built> = best.into_values().collect();
    out.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.label.cmp(&b.label))
    });
    out.truncate(MAX_DECKS);
    out
}
