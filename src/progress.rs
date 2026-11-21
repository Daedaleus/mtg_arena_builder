//! Progress: rank and win rate over time, per deck.
//!
//! All series run over the games in chronological order (oldest first); the
//! x axis is the running number of the game.

use crate::deckcheck::DeckGroup;
use crate::games::{GameRecord, Rank};

/// Constructed classes from bottom to top.
pub const CLASSES: [&str; 6] = ["Bronze", "Silver", "Gold", "Platinum", "Diamond", "Mythic"];
/// Progress steps per tier (for showing them as a decimal).
const STEPS_PER_LEVEL: f64 = 6.0;

/// Display name of a class.
pub fn class_name(class: &str) -> String {
    match class {
        "Silver" => tr!("progress.silver"),
        "Platinum" => tr!("progress.platinum"),
        "Diamond" => tr!("progress.diamond"),
        "Mythic" => tr!("progress.mythic"),
        c => c.to_string(),
    }
}

/// Queue whose rank is meant.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Queue {
    Constructed,
    Limited,
}

/// (class, tier, step) of the chosen queue.
fn parts(r: &Rank, q: Queue) -> (&str, u32, u32) {
    match q {
        Queue::Constructed => (&r.class, r.level, r.step),
        Queue::Limited => (&r.limited_class, r.limited_level, r.limited_step),
    }
}

/// Constructed rank as text, e.g. "Bronze 4, step 2".
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

/// Constructed rank as a number (see [`queue_points`]).
pub fn rank_points(r: &Rank) -> f64 {
    queue_points(r, Queue::Constructed)
}

/// Rank as a number for the chart: 4 points per class (one per tier),
/// Bronze 4 = 0, Silver 4 = 4 …; steps as a fraction.
pub fn queue_points(r: &Rank, q: Queue) -> f64 {
    let (class, level, step) = parts(r, q);
    let class = CLASSES.iter().position(|c| *c == class).unwrap_or(0) as f64;
    let level = 4.0 - f64::from(level.clamp(1, 4));
    let step = (f64::from(step) / STEPS_PER_LEVEL).min(0.99);
    class * 4.0 + level + step
}

/// Label of the y axis for [`rank_points`].
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

/// Indices of the games in chronological order (oldest first).
fn chronological(games: &[GameRecord]) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..games.len()).collect();
    idx.sort_by_key(|i| games[*i].started_ms);
    idx
}

/// Constructed rank after each game: (running number, rank points).
pub fn rank_series(games: &[GameRecord]) -> Vec<[f64; 2]> {
    queue_series(games, Queue::Constructed)
}

/// Rank of the queue after each game: (running number, rank points).
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

/// Moving win rate in percent over the last `window` finished games.
/// `filter` chooses the games (e.g. of one deck); the x axis stays the running
/// number over all games, so decks are comparable.
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

/// Win rate progress per deck (only decks with at least `min_games` games).
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

/// Short comparison: win rate of the last `n` games against the ones before.
/// `None` as long as there aren't enough games for both parts.
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
