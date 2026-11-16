//! Ersatzvorschläge: ähnliche Karten aus der eigenen Sammlung.
//!
//! Das ist eine Heuristik (Typ, Manawert, Farbe, gemeinsame Fähigkeiten) –
//! sie findet funktional verwandte Karten, ersetzt aber kein Spielverständnis.

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
    // Gleiche Aufgabe (Removal, Neutralisieren, Kartenziehen) zählt viel:
    // Ein Removal-Zauber soll durch Removal ersetzt werden, nicht durch einen Kampftrick.
    for role in [ROLE_REMOVAL, ROLE_COUNTER, ROLE_DRAW] {
        match (target.has_role(role), cand.has_role(role)) {
            (true, true) => s += 2.5,
            (true, false) => s -= 2.5,
            _ => {}
        }
    }
    s
}

/// Sucht bis zu `limit` Ersatzkarten für `target_key` in der Sammlung.
///
/// * `deck_colors` – Farbidentität des Decks; Ersatz muss hineinpassen.
/// * `in_deck` – Karten, die schon im Deck sind (werden nicht vorgeschlagen).
/// * `legal` – falls bekannt: im Format legale Kartennamen.
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
