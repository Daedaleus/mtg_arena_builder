//! Replacement suggestions: similar cards from the own collection.
//!
//! This is a heuristic (type, mana value, color, shared abilities) – it finds
//! functionally related cards, but doesn't replace game understanding.

use std::collections::{HashMap, HashSet};

use crate::carddb::{Card, CardDb, ROLE_COUNTER, ROLE_DRAW, ROLE_REMOVAL};

#[derive(Clone, Debug)]
pub struct Suggestion {
    pub key: String,
    pub name: String,
    pub owned: u32,
    pub score: f32,
    pub info: String,
}

fn jaccard<T: PartialEq>(a: &[T], b: &[T]) -> f32 {
    if a.is_empty() && b.is_empty() {
        return 0.0;
    }
    let inter = a.iter().filter(|x| b.contains(x)).count() as f32;
    let union = a.len() as f32 + b.iter().filter(|x| !a.contains(x)).count() as f32;
    inter / union
}

fn score(target: &Card, cand: &Card) -> f32 {
    let cmc_diff = target.cmc.abs_diff(cand.cmc) as f32;
    let mut s = 4.0 * jaccard(&target.abilities, &cand.abilities)
        + 2.0 * jaccard(&target.types, &cand.types)
        + 1.5 * jaccard(&target.subtypes, &cand.subtypes)
        + 1.5 / (1.0 + cmc_diff);
    if target.colors == cand.colors {
        s += 1.0;
    }
    if !target.power.is_empty() && target.power == cand.power && target.toughness == cand.toughness
    {
        s += 0.5;
    }
    // The same job (removal, countering, card draw) counts a lot: a removal
    // spell should be replaced by removal, not by a combat trick.
    for role in [ROLE_REMOVAL, ROLE_COUNTER, ROLE_DRAW] {
        match (target.has_role(role), cand.has_role(role)) {
            (true, true) => s += 2.5,
            (true, false) => s -= 2.5,
            _ => {}
        }
    }
    s
}

/// Searches up to `limit` replacement cards for `target_key` in the collection.
///
/// * `deck_colors` – color identity of the deck; the replacement must fit into it.
/// * `in_deck` – cards already in the deck (not suggested).
/// * `legal` – if known: card names legal in the format.
pub fn suggest(
    db: &CardDb,
    owned: &HashMap<String, u32>,
    legal: Option<&HashSet<String>>,
    deck_colors: u8,
    in_deck: &HashSet<String>,
    target_key: &str,
    limit: usize,
) -> Vec<Suggestion> {
    let Some(target) = db.representative(target_key) else {
        return Vec::new();
    };
    let mut out: Vec<Suggestion> = owned
        .iter()
        .filter(|(key, n)| **n > 0 && key.as_str() != target_key && !in_deck.contains(*key))
        .filter(|(key, _)| legal.is_none_or(|l| l.contains(*key)))
        .filter_map(|(key, n)| {
            let cand = db.representative(key)?;
            let fits = !cand.basic
                && !cand.back_face
                && cand.is_land() == target.is_land()
                && cand.types.iter().any(|t| target.types.contains(t))
                && cand.colors & !deck_colors == 0
                && cand.cmc.abs_diff(target.cmc) <= 1;
            fits.then(|| Suggestion {
                key: key.clone(),
                name: cand.name.clone(),
                owned: *n,
                score: score(target, cand),
                info: if cand.mana_cost.is_empty() {
                    String::new()
                } else {
                    cand.mana_cost.clone()
                },
            })
        })
        .collect();
    out.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.name.cmp(&b.name))
    });
    out.truncate(limit);
    out
}

impl Suggestion {
    /// Suggestion for a specific own card (e.g. chosen by the budget plan).
    pub fn for_card(db: &CardDb, owned: &HashMap<String, u32>, key: &str) -> Option<Suggestion> {
        let c = db.representative(key)?;
        Some(Suggestion {
            key: key.to_string(),
            name: c.name.clone(),
            owned: owned.get(key).copied().unwrap_or(0),
            score: 0.0,
            info: if c.basic {
                tr!("state.basic_land")
            } else {
                c.mana_cost.clone()
            },
        })
    }
}

#[cfg(test)]
#[path = "tests/similar.rs"]
mod tests;
