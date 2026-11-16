//! Gegner-Analyse: Gegen welche Decks gewinne bzw. verliere ich?
//!
//! Das gegnerische Deck wird aus den gesehenen Karten erkannt: Passen sie zu
//! einem geladenen Turnier-Archetyp, wird dessen Name verwendet, sonst die
//! Farben des Gegners.

use std::collections::{HashMap, HashSet};

use crate::carddb::{CardDb, name_key};
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

pub fn color_label(letters: &str) -> String {
    let name = |c: char| match c {
        'W' => "Weiß",
        'U' => "Blau",
        'B' => "Schwarz",
        'R' => "Rot",
        'G' => "Grün",
        _ => "?",
    };
    match letters.len() {
        0 => "Unbekannt".into(),
        1 => format!("Mono-{}", name(letters.chars().next().unwrap())),
        _ => letters.chars().map(name).collect::<Vec<_>>().join("-"),
    }
}

/// Erkennt das gegnerische Deck einer Partie.
pub fn classify(g: &GameRecord, db: &CardDb, meta: &[MetaArchetype]) -> String {
    let seen: HashSet<String> = g
        .opp_cards
        .keys()
        .filter_map(|grp| db.cards.get(grp))
        .filter(|c| !c.is_land())
        .map(|c| name_key(&c.name))
        .collect();
    let mut best: Option<(&str, usize, f32)> = None;
    for arch in meta.iter().filter(|a| !a.lists.is_empty()) {
        let n = arch.lists.len() as f32;
        let mut share: HashMap<String, f32> = HashMap::new();
        for list in &arch.lists {
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
        let hits: Vec<f32> = seen.iter().filter_map(|k| share.get(k).copied()).collect();
        let score: f32 = hits.iter().sum();
        if best.is_none_or(|b| score > b.2) {
            best = Some((&arch.name, hits.len(), score));
        }
    }
    // Mindestens drei typische Karten bzw. die Hälfte der gesehenen Karten müssen passen.
    match best {
        Some((name, hits, _))
            if hits >= 3 || (seen.len() >= 2 && hits * 2 >= seen.len() && hits >= 2) =>
        {
            format!("{name} (erkannt)")
        }
        _ => color_label(&games::opp_colors(g, db)),
    }
}

/// Fasst die Partien nach Gegnertyp zusammen, meiste Niederlagen zuerst.
pub fn analyze(games: &[GameRecord], db: &CardDb, meta: &[MetaArchetype]) -> Vec<OppGroup> {
    let mut groups: Vec<OppGroup> = Vec::new();
    let mut damage: Vec<HashMap<String, u32>> = Vec::new();
    for (i, g) in games.iter().enumerate() {
        if g.won.is_none() {
            continue;
        }
        let name = classify(g, db, meta);
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
