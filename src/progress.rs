//! Verlauf: Rang und Siegquote über die Zeit, je Deck.
//!
//! Alle Reihen laufen über die Partien in zeitlicher Reihenfolge (älteste
//! zuerst); die x-Achse ist die laufende Nummer der Partie.

use crate::deckcheck::DeckGroup;
use crate::games::{GameRecord, Rank};

/// Constructed-Klassen von unten nach oben.
pub const CLASSES: [&str; 6] = ["Bronze", "Silver", "Gold", "Platinum", "Diamond", "Mythic"];
/// Fortschrittsschritte je Stufe (für die Darstellung als Kommazahl).
const STEPS_PER_LEVEL: f64 = 6.0;

/// Deutscher Name einer Klasse.
pub fn class_name(class: &str) -> String {
    match class {
        "Silver" => tr!("progress.silver"),
        "Platinum" => tr!("progress.platinum"),
        "Diamond" => tr!("progress.diamond"),
        "Mythic" => tr!("progress.mythic"),
        c => c.to_string(),
    }
}

/// Spielweise, deren Rang gemeint ist.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Queue {
    Constructed,
    Limited,
}

/// (Klasse, Stufe, Schritt) der gewählten Spielweise.
fn parts(r: &Rank, q: Queue) -> (&str, u32, u32) {
    match q {
        Queue::Constructed => (&r.class, r.level, r.step),
        Queue::Limited => (&r.limited_class, r.limited_level, r.limited_step),
    }
}

/// Constructed-Rang als Text, z. B. „Bronze 4, Schritt 2“.
pub fn rank_label(r: &Rank) -> String {
    queue_label(r, Queue::Constructed)
}

pub fn queue_label(r: &Rank, q: Queue) -> String {
    let (class, level, step) = parts(r, q);
    if class == "Mythic" {
        return class_name(class).to_string();
    }
    tr!(
        "progress.step",
        v = class_name(class),
        level = level,
        step = step
    )
}

/// Constructed-Rang als Zahl (siehe [`queue_points`]).
pub fn rank_points(r: &Rank) -> f64 {
    queue_points(r, Queue::Constructed)
}

/// Rang als Zahl für das Diagramm: je Klasse 4 Punkte (eine je Stufe),
/// Bronze 4 = 0, Silber 4 = 4 …; Schritte als Bruchteil.
pub fn queue_points(r: &Rank, q: Queue) -> f64 {
    let (class, level, step) = parts(r, q);
    let class = CLASSES.iter().position(|c| *c == class).unwrap_or(0) as f64;
    let level = 4.0 - f64::from(level.clamp(1, 4));
    let step = (f64::from(step) / STEPS_PER_LEVEL).min(0.99);
    class * 4.0 + level + step
}

/// Beschriftung der y-Achse zu [`rank_points`].
pub fn points_label(points: f64) -> String {
    let p = points.max(0.0);
    let class = ((p / 4.0) as usize).min(CLASSES.len() - 1);
    if CLASSES[class] == "Mythic" {
        return tr!("progress.mythic");
    }
    let level = 4 - ((p - class as f64 * 4.0) as u32).min(3);
    tr!(
        "progress.text",
        v = class_name(CLASSES[class]),
        level = level
    )
}

/// Indizes der Partien in zeitlicher Reihenfolge (älteste zuerst).
fn chronological(games: &[GameRecord]) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..games.len()).collect();
    idx.sort_by_key(|i| games[*i].started_ms);
    idx
}

/// Constructed-Rang nach jeder Partie: (laufende Nummer, Rangpunkte).
pub fn rank_series(games: &[GameRecord]) -> Vec<[f64; 2]> {
    queue_series(games, Queue::Constructed)
}

/// Rang der Spielweise nach jeder Partie: (laufende Nummer, Rangpunkte).
pub fn queue_series(games: &[GameRecord], q: Queue) -> Vec<[f64; 2]> {
    chronological(games)
        .iter()
        .enumerate()
        .filter_map(|(n, i)| {
            let r = games[*i].rank_after.as_ref()?;
            Some([(n + 1) as f64, queue_points(r, q)])
        })
        .collect()
}

/// Gleitende Siegquote in Prozent über die letzten `window` beendeten Partien.
/// `filter` wählt die Partien (z. B. eines Decks); die x-Achse bleibt die
/// laufende Nummer über alle Partien, damit Decks vergleichbar sind.
pub fn winrate_series(
    games: &[GameRecord],
    window: usize,
    filter: impl Fn(usize) -> bool,
) -> Vec<[f64; 2]> {
    let mut recent: Vec<bool> = Vec::new();
    let mut out = Vec::new();
    for (n, i) in chronological(games).iter().enumerate() {
        let Some(won) = games[*i].won else { continue };
        if !filter(*i) {
            continue;
        }
        recent.push(won);
        if recent.len() > window.max(1) {
            recent.remove(0);
        }
        let wins = recent.iter().filter(|w| **w).count();
        out.push([(n + 1) as f64, 100.0 * wins as f64 / recent.len() as f64]);
    }
    out
}

/// Siegquote-Verlauf je Deck (nur Decks mit mindestens `min_games` Partien).
pub fn deck_series(
    games: &[GameRecord],
    groups: &[DeckGroup],
    window: usize,
    min_games: usize,
) -> Vec<(String, Vec<[f64; 2]>)> {
    groups
        .iter()
        .filter(|g| g.games.len() >= min_games)
        .map(|g| {
            let series = winrate_series(games, window, |i| g.games.contains(&i));
            (g.label.clone(), series)
        })
        .collect()
}

/// Kurzer Vergleich: Siegquote der letzten `n` Partien gegenüber den davor.
/// `None`, solange es nicht genug Partien für beide Abschnitte gibt.
pub fn trend(games: &[GameRecord], n: usize) -> Option<(f64, f64)> {
    let results: Vec<bool> = chronological(games)
        .iter()
        .filter_map(|i| games[*i].won)
        .collect();
    if n == 0 || results.len() < n * 2 {
        return None;
    }
    let rate =
        |part: &[bool]| 100.0 * part.iter().filter(|w| **w).count() as f64 / part.len() as f64;
    let (before, recent) = results.split_at(results.len() - n);
    Some((rate(recent), rate(before)))
}

#[cfg(test)]
#[path = "tests/progress.rs"]
mod tests;
