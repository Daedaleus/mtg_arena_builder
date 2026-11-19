//! Gegner-Analyse: Gegen welche Decks gewinne bzw. verliere ich?
//!
//! Das gegnerische Deck wird aus den gesehenen Karten erkannt: Passen sie zu
//! einem geladenen Turnier-Archetyp, wird dessen Name verwendet, sonst die
//! Farben des Gegners.

use std::collections::{HashMap, HashSet};

use crate::carddb::{CardDb, color, name_key};
use crate::deckcheck::MetaArchetype;
use crate::games::{self, GameRecord};

#[derive(Clone, Debug, Default)]
pub struct OppGroup {
    pub name: String,
    /// Indizes der Partien.
    pub games: Vec<usize>,
    pub wins: usize,
    pub avg_turns: f32,
    /// Gefährlichste Karten dieses Gegnertyps: (Name, Schaden an mir).
    pub threats: Vec<(String, u32)>,
}

impl OppGroup {
    pub fn losses(&self) -> usize {
        self.games.len() - self.wins
    }
}

/// Anteil der Turnierlisten eines Archetyps, die eine Karte spielen.
#[derive(Clone, Debug, Default)]
pub struct ArchShare {
    pub name: String,
    /// Kartenschlüssel -> Anteil der Listen (0..=1).
    pub share: HashMap<String, f32>,
}

/// Anteile je Karte für alle Archetypen mit geladenen Turnierlisten
/// (Haupt- und Nebendeck; jede Karte zählt pro Liste einmal).
pub fn arch_shares(archetypes: &[MetaArchetype], db: &CardDb) -> Vec<ArchShare> {
    archetypes
        .iter()
        .filter(|a| !a.lists.is_empty())
        .map(|a| {
            let n = a.lists.len() as f32;
            let mut share: HashMap<String, f32> = HashMap::new();
            for list in &a.lists {
                let keys: HashSet<String> = list
                    .main
                    .iter()
                    .chain(&list.side)
                    .filter_map(|e| db.resolve(&e.name))
                    .collect();
                for k in keys {
                    *share.entry(k).or_insert(0.0) += 1.0 / n;
                }
            }
            ArchShare {
                name: a.name.clone(),
                share,
            }
        })
        .collect()
}

/// Schlüssel der gesehenen Nichtland-Karten.
pub fn seen_keys<'a>(cards: impl IntoIterator<Item = &'a u32>, db: &CardDb) -> HashSet<String> {
    cards
        .into_iter()
        .filter_map(|g| db.cards.get(g))
        .filter(|c| !c.is_land())
        .map(|c| name_key(&c.name))
        .collect()
}

/// Archetyp, zu dem die gesehenen Karten am besten passen. Mindestens drei
/// typische Karten bzw. die Hälfte der gesehenen Karten (wenigstens zwei)
/// müssen passen.
pub fn match_archetype<'a>(
    seen: &HashSet<String>,
    shares: &'a [ArchShare],
) -> Option<&'a ArchShare> {
    let (arch, hits, _) = shares
        .iter()
        .map(|a| {
            let hits: Vec<f32> = seen
                .iter()
                .filter_map(|k| a.share.get(k).copied())
                .collect();
            (a, hits.len(), hits.iter().sum::<f32>())
        })
        .max_by(|a, b| a.2.total_cmp(&b.2).then(b.0.name.cmp(&a.0.name)))?;
    (hits >= 3 || (seen.len() >= 2 && hits * 2 >= seen.len() && hits >= 2)).then_some(arch)
}

/// Ab diesem durchschnittlichen Anteil gilt eine Deckliste als Archetyp.
const MIN_DECK_MATCH: f32 = 0.35;

/// Bezeichnung für eine vollständige Deckliste ohne Archetyp-Namen (z. B. von
/// magic.gg): passender Turnier-Archetyp, wenn die Nichtland-Karten im Schnitt
/// in mindestens 35 % seiner Listen vorkommen, sonst Farben und seltenste Karte.
pub fn deck_label(deck: &crate::deck::Deck, db: &CardDb, shares: &[ArchShare]) -> String {
    let cards: Vec<&crate::carddb::Card> = deck
        .main
        .iter()
        .filter_map(|e| db.resolve(&e.name).and_then(|k| db.representative(&k)))
        .filter(|c| !c.is_land())
        .collect();
    let keys: HashSet<String> = cards.iter().map(|c| name_key(&c.name)).collect();
    if !keys.is_empty() {
        let best = shares
            .iter()
            .map(|a| {
                let sum: f32 = keys.iter().filter_map(|k| a.share.get(k)).sum();
                (a, sum / keys.len() as f32)
            })
            .max_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((arch, score)) = best
            && score >= MIN_DECK_MATCH
        {
            return arch.name.clone();
        }
    }
    let ids: Vec<u32> = cards.iter().map(|c| c.grp_id).collect();
    let colors = color::label(&games::deck_colors(&ids, db));
    match cards.iter().max_by_key(|c| (c.rarity, c.cmc)) {
        Some(c) => tr!("opponents.text", colors = colors, name = c.name),
        None => colors,
    }
}

/// Erkennt das gegnerische Deck einer Partie.
pub fn classify(g: &GameRecord, db: &CardDb, shares: &[ArchShare]) -> String {
    let seen = seen_keys(g.opp_cards.keys(), db);
    match match_archetype(&seen, shares) {
        Some(arch) => tr!("opponents.erkannt", name = arch.name),
        None => color::label(&games::opp_colors(g, db)),
    }
}

/// Fasst die Partien nach Gegnertyp zusammen, meiste Niederlagen zuerst.
pub fn analyze(games: &[GameRecord], db: &CardDb, shares: &[ArchShare]) -> Vec<OppGroup> {
    let mut groups: Vec<OppGroup> = Vec::new();
    let mut damage: Vec<HashMap<String, u32>> = Vec::new();
    for (i, g) in games.iter().enumerate() {
        if g.won.is_none() {
            continue;
        }
        let name = classify(g, db, shares);
        let pos = match groups.iter().position(|x| x.name == name) {
            Some(p) => p,
            None => {
                groups.push(OppGroup {
                    name,
                    ..Default::default()
                });
                damage.push(HashMap::new());
                groups.len() - 1
            }
        };
        groups[pos].games.push(i);
        groups[pos].wins += (g.won == Some(true)) as usize;
        for (grp, dmg) in &g.damage_taken {
            if let Some(c) = db.cards.get(grp) {
                *damage[pos].entry(c.name.clone()).or_insert(0) += dmg;
            }
        }
    }
    for (grp, dmg) in groups.iter_mut().zip(damage) {
        let turns: u32 = grp.games.iter().map(|i| games[*i].last_turn()).sum();
        grp.avg_turns = turns as f32 / grp.games.len().max(1) as f32;
        let mut threats: Vec<(String, u32)> = dmg.into_iter().collect();
        threats.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        threats.truncate(3);
        grp.threats = threats;
    }
    groups.sort_by(|a, b| {
        b.losses()
            .cmp(&a.losses())
            .then(b.games.len().cmp(&a.games.len()))
            .then_with(|| a.name.cmp(&b.name))
    });
    groups
}

#[cfg(test)]
#[path = "tests/opponents.rs"]
mod tests;
