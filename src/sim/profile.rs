//! Persönliche Probehände: Gegner und Spielgewohnheiten aus den eigenen
//! Partien statt fester Annahmen.
//!
//! - [`Opponent`]: wie viel Kraft die Gegner je Zug aufs Feld bringen
//!   (Durchschnitt über die eigenen Partien), wie oft sie Entfernung und
//!   Kampftricks spielen und welche Decks am häufigsten sind.
//! - [`Habits`]: wie oft man einen besseren Zug auslässt und wie oft man
//!   einen gefahrlosen Angriff nicht macht.

use super::*;
use crate::carddb::{ROLE_COUNTER, ROLE_DRAW, ROLE_REMOVAL, ROLE_WIPE};
use crate::games::{self, GameRecord};

/// Mindestens so viele vollständige Partien mit Gegnerdaten für ein Profil.
pub const MIN_PROFILE_GAMES: usize = 5;
/// Mindestens so viele auswertbare eigene Züge für Gewohnheiten.
pub const MIN_HABIT_TURNS: usize = 20;
/// Mindestens so viele Partien, die einen Gegnerzug erreichen, damit er zählt.
pub const MIN_TURN_SAMPLES: usize = 5;
/// Höchstens so viel neue Kraft je Gegnerzug und Partie zählt (Ausreißer wie
/// ganze Spielstein-Armeen sollen den Schnitt nicht verzerren).
pub const MAX_GAIN: f32 = 6.0;
/// Höchstens so oft je Gegnerzug Entfernung bzw. ein Trick.
pub const MAX_RATE: f32 = 0.5;

/// Gegner der Simulation: erwartete neue Kraft und Widerstandskraft je Zug.
#[derive(Clone, Debug, PartialEq)]
pub struct Opponent {
    pub power: [f32; TURNS],
    pub toughness: [f32; TURNS],
    /// Kurzbeschreibung, z. B. „schneller Gegner“ oder „deine Gegner (meist Mono-Rot)“.
    pub label: String,
    /// Partien, aus denen das Profil stammt (0 = feste Annahme).
    pub games: usize,
    /// Wie viele Züge gespielt werden.
    pub turns: usize,
    /// Chance je Gegnerzug, eine eigene Kreatur zu entfernen.
    pub removal: f32,
    /// Chance je Kampf auf einen Trick (+2/+2 für eine gegnerische Kreatur).
    pub tricks: f32,
}

impl Opponent {
    /// Fester, schneller Gegner (Kreaturen nach [`OPP_CURVE`]).
    pub fn fast() -> Opponent {
        Opponent {
            power: OPP_CURVE.map(|(p, _)| p as f32),
            toughness: OPP_CURVE.map(|(_, t)| t as f32),
            label: "schneller Gegner".into(),
            games: 0,
            turns: FAST_TURNS,
            removal: 0.0,
            tricks: 0.0,
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
    // Zuwachs an Kraft und Widerstand je Gegnerzug – innerhalb derselben
    // Partie gerechnet, damit lange Partien (größeres Feld) die späten Züge
    // nicht verzerren.
    let mut power = [0f32; TURNS];
    let mut toughness = [0f32; TURNS];
    let mut count = [0usize; TURNS];
    for g in &games {
        let (mut last_p, mut last_t) = (0f32, 0f32);
        for (k, t) in g.turns.iter().filter(|t| !t.mine).take(TURNS).enumerate() {
            let p: f32 = t.opp_creatures.iter().map(|(_, p)| *p as f32).sum();
            let tough: f32 = t
                .opp_creatures
                .iter()
                .map(|(grp, p)| {
                    db.cards
                        .get(grp)
                        .and_then(|c| c.toughness.parse::<f32>().ok())
                        .unwrap_or(*p as f32)
                })
                .sum();
            power[k] += (p - last_p).clamp(0.0, MAX_GAIN);
            toughness[k] += (tough - last_t).clamp(0.0, MAX_GAIN);
            (last_p, last_t) = (p, tough);
            count[k] += 1;
        }
    }
    let mut opp = Opponent {
        power: [0.0; TURNS],
        toughness: [0.0; TURNS],
        label: String::new(),
        games: games.len(),
        turns: TURNS,
        removal: 0.0,
        tricks: 0.0,
    };
    // Gesehene Entfernung und Tricks je Gegnerzug.
    let (mut removal, mut tricks, mut opp_turns) = (0u32, 0u32, 0usize);
    for g in &games {
        opp_turns += g.turns.iter().filter(|t| !t.mine).count();
        for (grp, n) in &g.opp_cards {
            let Some(c) = db.cards.get(grp) else { continue };
            if c.is_creature() || c.is_land() {
                continue;
            }
            if c.has_role(ROLE_REMOVAL) || c.has_role(ROLE_WIPE) {
                removal += n;
            } else if c.is_instant() && !c.has_role(ROLE_COUNTER) && !c.has_role(ROLE_DRAW) {
                tricks += n;
            }
        }
    }
    let rate = |n: u32| (n as f32 / opp_turns.max(1) as f32).min(MAX_RATE);
    opp.removal = rate(removal);
    opp.tricks = rate(tricks);
    for k in 0..TURNS {
        if count[k] >= MIN_TURN_SAMPLES {
            opp.power[k] = power[k] / count[k] as f32;
            opp.toughness[k] = (toughness[k] / count[k] as f32).max(opp.power[k]);
        } else if k > 0 {
            // Zu wenige Partien so lang: Tempo des Vorzugs beibehalten.
            opp.power[k] = opp.power[k - 1];
            opp.toughness[k] = opp.toughness[k - 1];
        }
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
    /// Das Gegnerprofil selbst (falls genug Partien).
    pub opponent: Option<Opponent>,
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
        profile_games: opponent.as_ref().map_or(0, |o| o.games),
        opponent,
    }
}
