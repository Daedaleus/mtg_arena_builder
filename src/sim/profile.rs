//! Personal sample hands: opponents and play habits from the own games
//! instead of fixed assumptions.
//!
//! - [`Opponent`]: how much power the opponents bring onto the battlefield
//!   per turn (average over the own games), how often they play removal and
//!   combat tricks, how many of their creatures fly, and which decks are most
//!   common.
//! - [`Habits`]: how often you skip a better play and how often you don't
//!   make a safe attack.

use super::*;
use crate::carddb::{ROLE_COUNTER, ROLE_DRAW, ROLE_REMOVAL, ROLE_WIPE, keyword};
use crate::games::{self, GameMode, GameRecord};

/// At least this many complete games with opponent data for a profile.
pub const MIN_PROFILE_GAMES: usize = 5;
/// At least this many evaluable own turns for habits.
pub const MIN_HABIT_TURNS: usize = 20;
/// At least this many games reaching an opponent turn for it to count.
pub const MIN_TURN_SAMPLES: usize = 5;
/// At most this much new power per opponent turn and game counts (outliers
/// like whole token armies shouldn't distort the average).
pub const MAX_GAIN: f32 = 6.0;
/// At most this often removal or a trick per opponent turn.
pub const MAX_RATE: f32 = 0.5;

/// Opponent of the simulation: expected new power and toughness per turn.
#[derive(Clone, Debug, PartialEq)]
pub struct Opponent {
    pub power: [f32; TURNS],
    pub toughness: [f32; TURNS],
    /// Short description, e.g. "fast opponent" or "your opponents (mostly mono-red)".
    pub label: String,
    /// Games the profile comes from (0 = fixed assumption).
    pub games: usize,
    /// How many turns are played.
    pub turns: usize,
    /// Chance per opponent turn to remove an own creature.
    pub removal: f32,
    /// Chance per combat of a trick (+2/+2 for an opposing creature).
    pub tricks: f32,
    /// Share of new opposing creatures that fly.
    pub flying: f32,
}

impl Opponent {
    /// Fixed, fast opponent (creatures following [`OPP_CURVE`]).
    pub fn fast() -> Opponent {
        Opponent {
            power: OPP_CURVE.map(|(p, _)| p as f32),
            toughness: OPP_CURVE.map(|(_, t)| t as f32),
            label: tr!("sim_profile.fast_opponent"),
            games: 0,
            turns: FAST_TURNS,
            removal: 0.0,
            tricks: 0.0,
            flying: 0.0,
        }
    }

    /// New creature in opponent turn `turn` (1-based) or none.
    /// Fractions are rounded randomly (0.4 = one more in 40 % of cases).
    pub(super) fn creature(&self, turn: usize, rng: &mut Rng) -> Option<(i32, i32)> {
        let k = (turn - 1).min(TURNS - 1);
        let power = rng.round(self.power[k]);
        (power > 0).then(|| (power, rng.round(self.toughness[k]).max(1)))
    }
}

/// Play habits from the own games.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Habits {
    /// Share of own turns in which a clearly better play was possible.
    pub weaker_turn: f32,
    /// Share of turns with a safe attack in which there was no attack.
    pub skip_attack: f32,
    /// Own turns evaluated.
    pub turns: usize,
}

/// Games that fit the simulation: complete and of the same kind (Brawl for
/// Brawl decks, otherwise constructed – not Brawl or limited, which have
/// another pace and other decks).
fn usable(g: &GameRecord, rules: Rules) -> bool {
    g.complete && rules.plays_like(GameMode::of(&g.event))
}

/// Opponent profile from the own games (`None` with too little data).
pub fn opponent_from(history: &[GameRecord], db: &CardDb, rules: Rules) -> Option<Opponent> {
    let games: Vec<&GameRecord> = history
        .iter()
        .filter(|g| usable(g, rules) && g.turns.iter().any(|t| !t.opp_creatures.is_empty()))
        .collect();
    if games.len() < MIN_PROFILE_GAMES {
        return None;
    }
    // Gain in power and toughness per opponent turn – computed within the
    // same game, so long games (bigger battlefield) don't distort the late
    // turns.
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
        flying: 0.0,
    };
    // Removal and tricks seen per opponent turn; share of flying creatures.
    let (mut removal, mut tricks, mut opp_turns) = (0u32, 0u32, 0usize);
    let (mut creatures, mut flyers) = (0u32, 0u32);
    for g in &games {
        opp_turns += g.turns.iter().filter(|t| !t.mine).count();
        for (grp, n) in &g.opp_cards {
            let Some(c) = db.cards.get(grp) else { continue };
            if c.is_creature() {
                creatures += n;
                if c.has_ability(keyword::FLYING) {
                    flyers += n;
                }
                continue;
            }
            if c.is_land() {
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
    opp.flying = if creatures == 0 {
        0.0
    } else {
        flyers as f32 / creatures as f32
    };
    for k in 0..TURNS {
        if count[k] >= MIN_TURN_SAMPLES {
            opp.power[k] = power[k] / count[k] as f32;
            opp.toughness[k] = (toughness[k] / count[k] as f32).max(opp.power[k]);
        } else if k > 0 {
            // Too few games this long: keep the pace of the previous turn.
            opp.power[k] = opp.power[k - 1];
            opp.toughness[k] = opp.toughness[k - 1];
        }
    }
    // Most frequent opponents by colors.
    let mut by_name: Vec<(String, usize)> = Vec::new();
    for g in &games {
        let name = crate::carddb::color::label(&games::opp_colors(g, db));
        match by_name.iter_mut().find(|(n, _)| *n == name) {
            Some((_, n)) => *n += 1,
            None => by_name.push((name, 1)),
        }
    }
    by_name.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let top: Vec<String> = by_name.iter().take(2).map(|(n, _)| n.clone()).collect();
    opp.label = tr!("sim_profile.opponents_mostly", v = top.join(", "));
    Some(opp)
}

/// Habits from the own games (`None` with too little data).
pub fn habits_from(history: &[GameRecord], db: &CardDb, rules: Rules) -> Option<Habits> {
    let mine: Vec<&games::TurnStat> = history
        .iter()
        .filter(|g| usable(g, rules))
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

/// Sample hands in three variants: against the fixed fast opponent, against
/// the own opponents with the turn suggestion, and "the way you play".
#[derive(Clone, Debug, PartialEq)]
pub struct Trial {
    pub fast: SimReport,
    /// Against the own opponents, with the turn suggestion (if enough games).
    pub mine: Option<SimReport>,
    /// Against the own opponents, with the own habits.
    pub mine_habits: Option<SimReport>,
    pub habits: Option<Habits>,
    /// Games the opponent profile comes from.
    pub profile_games: usize,
    /// The opponent profile itself (if enough games).
    pub opponent: Option<Opponent>,
}

/// Runs all variants; `history` are the own games, `deck` the library
/// (without a Brawl commander, that is in `rules`).
pub fn trial(
    deck: &[GrpId],
    db: &CardDb,
    history: &[GameRecord],
    games: usize,
    seed: u64,
    rules: Rules,
) -> Trial {
    let fast = simulate_with(deck, db, games, seed, &Opponent::fast(), None, rules);
    let opponent = opponent_from(history, db, rules);
    let habits = habits_from(history, db, rules);
    let mine = opponent
        .as_ref()
        .map(|o| simulate_with(deck, db, games, seed, o, None, rules));
    let mine_habits = match (&opponent, &habits) {
        (Some(o), Some(h)) => Some(simulate_with(deck, db, games, seed, o, Some(h), rules)),
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
