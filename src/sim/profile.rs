//! Persönliche Probehände: Gegner und Spielgewohnheiten aus den eigenen
//! Partien statt fester Annahmen.
//!
//! - [`Opponent`]: wie viel Kraft die Gegner je Zug aufs Feld bringen
//!   (Durchschnitt über die eigenen Partien) und welche Decks am häufigsten sind.
//! - [`Habits`]: wie oft man einen besseren Zug auslässt und wie oft man
//!   einen gefahrlosen Angriff nicht macht.

use super::*;
use crate::games::{self, GameRecord};

/// Mindestens so viele vollständige Partien mit Gegnerdaten für ein Profil.
pub const MIN_PROFILE_GAMES: usize = 5;
/// Mindestens so viele auswertbare eigene Züge für Gewohnheiten.
pub const MIN_HABIT_TURNS: usize = 20;

/// Gegner der Simulation: erwartete neue Kraft und Widerstandskraft je Zug.
#[derive(Clone, Debug, PartialEq)]
pub struct Opponent {
    pub power: [f32; TURNS],
    pub toughness: [f32; TURNS],
    /// Kurzbeschreibung, z. B. „schneller Gegner“ oder „deine Gegner (meist Mono-Rot)“.
    pub label: String,
    /// Partien, aus denen das Profil stammt (0 = feste Annahme).
    pub games: usize,
}

impl Opponent {
    /// Fester, schneller Gegner (eine Kreatur je Zug nach [`OPP_CURVE`]).
    pub fn fast() -> Opponent {
        Opponent {
            power: OPP_CURVE.map(|(p, _)| p as f32),
            toughness: OPP_CURVE.map(|(_, t)| t as f32),
            label: "schneller Gegner".into(),
            games: 0,
        }
    }

    /// Neue Kreatur im Gegnerzug `turn` (1-basiert) oder keine.
    /// Bruchteile werden zufällig gerundet (0,4 = in 40 % der Fälle eins mehr).
    pub(super) fn creature(&self, turn: usize, rng: &mut Rng) -> Option<(i32, i32)> {
        let k = (turn - 1).min(TURNS - 1);
        let power = rng.round(self.power[k]);
        (power > 0).then(|| (power, rng.round(self.toughness[k]).max(1)))
    }
}

/// Spielgewohnheiten aus den eigenen Partien.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Habits {
    /// Anteil eigener Züge, in denen ein deutlich besserer Zug möglich war.
    pub weaker_turn: f32,
    /// Anteil der Züge mit gefahrlosem Angriff, in denen nicht angegriffen wurde.
    pub skip_attack: f32,
    /// Ausgewertete eigene Züge.
    pub turns: usize,
}

/// Gegnerprofil aus den eigenen Partien (`None` bei zu wenig Daten).
pub fn opponent_from(history: &[GameRecord], db: &CardDb) -> Option<Opponent> {
    let games: Vec<&GameRecord> = history
        .iter()
        .filter(|g| g.complete && g.turns.iter().any(|t| !t.opp_creatures.is_empty()))
        .collect();
    if games.len() < MIN_PROFILE_GAMES {
        return None;
    }
    // Kraft und Widerstand auf dem Feld am Ende des k-ten Gegnerzugs.
    let mut power = [0f32; TURNS];
    let mut toughness = [0f32; TURNS];
    let mut count = [0usize; TURNS];
    for g in &games {
        for (k, t) in g.turns.iter().filter(|t| !t.mine).take(TURNS).enumerate() {
            power[k] += t.opp_creatures.iter().map(|(_, p)| *p as f32).sum::<f32>();
            toughness[k] += t
                .opp_creatures
                .iter()
                .map(|(grp, p)| {
                    db.cards
                        .get(grp)
                        .and_then(|c| c.toughness.parse::<f32>().ok())
                        .unwrap_or(*p as f32)
                })
                .sum::<f32>();
            count[k] += 1;
        }
    }
    // Zuwachs je Zug = neue Kreatur des Gegners in diesem Zug.
    let avg = |v: &[f32; TURNS], k: usize| {
        if count[k] == 0 {
            0.0
        } else {
            v[k] / count[k] as f32
        }
    };
    let mut opp = Opponent {
        power: [0.0; TURNS],
        toughness: [0.0; TURNS],
        label: String::new(),
        games: games.len(),
    };
    for k in 0..TURNS {
        let before = |v: &[f32; TURNS]| if k == 0 { 0.0 } else { avg(v, k - 1) };
        opp.power[k] = (avg(&power, k) - before(&power)).max(0.0);
        opp.toughness[k] = (avg(&toughness, k) - before(&toughness)).max(opp.power[k]);
    }
    // Häufigste Gegner nach Farben.
    let mut by_name: Vec<(String, usize)> = Vec::new();
    for g in &games {
        let name = crate::opponents::color_label(&games::opp_colors(g, db));
        match by_name.iter_mut().find(|(n, _)| *n == name) {
            Some((_, n)) => *n += 1,
            None => by_name.push((name, 1)),
        }
    }
    by_name.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let top: Vec<String> = by_name.iter().take(2).map(|(n, _)| n.clone()).collect();
    opp.label = format!("deine Gegner (meist {})", top.join(", "));
    Some(opp)
}

/// Gewohnheiten aus den eigenen Partien (`None` bei zu wenig Daten).
pub fn habits_from(history: &[GameRecord], db: &CardDb) -> Option<Habits> {
    let mine: Vec<&games::TurnStat> = history
        .iter()
        .filter(|g| g.complete)
        .flat_map(|g| g.turns.iter().filter(|t| t.mine && !t.lands.is_empty()))
        .collect();
    if mine.len() < MIN_HABIT_TURNS {
        return None;
    }
    let weaker = mine
        .iter()
        .filter(|t| games::key_moment(t, db).is_some())
        .count();
    let could_attack: Vec<&&games::TurnStat> = mine
        .iter()
        .filter(|t| t.attacked || !t.safe_idle.is_empty())
        .collect();
    let skipped = could_attack
        .iter()
        .filter(|t| !t.safe_idle.is_empty())
        .count();
    Some(Habits {
        weaker_turn: weaker as f32 / mine.len() as f32,
        skip_attack: if could_attack.is_empty() {
            0.0
        } else {
            skipped as f32 / could_attack.len() as f32
        },
        turns: mine.len(),
    })
}

/// Probehände in drei Varianten: gegen den festen schnellen Gegner, gegen
/// die eigenen Gegner mit Zugvorschlag und „so wie du spielst“.
#[derive(Clone, Debug, PartialEq)]
pub struct Trial {
    pub fast: SimReport,
    /// Gegen die eigenen Gegner, mit Zugvorschlag (falls genug Partien).
    pub mine: Option<SimReport>,
    /// Gegen die eigenen Gegner, mit den eigenen Gewohnheiten.
    pub mine_habits: Option<SimReport>,
    pub habits: Option<Habits>,
    /// Partien, aus denen das Gegnerprofil stammt.
    pub profile_games: usize,
}

/// Führt alle Varianten aus; `history` sind die eigenen Partien.
pub fn trial(deck: &[u32], db: &CardDb, history: &[GameRecord], games: usize, seed: u64) -> Trial {
    let fast = simulate(deck, db, games, seed);
    let opponent = opponent_from(history, db);
    let habits = habits_from(history, db);
    let mine = opponent
        .as_ref()
        .map(|o| simulate_with(deck, db, games, seed, o, None));
    let mine_habits = match (&opponent, &habits) {
        (Some(o), Some(h)) => Some(simulate_with(deck, db, games, seed, o, Some(h))),
        _ => None,
    };
    Trial {
        fast,
        mine,
        mine_habits,
        habits,
        profile_games: opponent.map_or(0, |o| o.games),
    }
}
