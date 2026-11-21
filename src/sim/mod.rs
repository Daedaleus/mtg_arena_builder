//! Sample hands: plays a deck many times and measures how often lands or
//! colors are missing, how much of the available mana is used and how the
//! deck holds up against an opponent.
//!
//! It plays with the same turn suggestion as the companion
//! ([`crate::turnplan`]): play a land, then use the mana as well as possible.
//! Mulligan: opening hands with 0–1 or 6–7 lands are swapped once for 6
//! cards (London mulligan: draw 7, one card to the bottom of the library).
//!
//! Layout: `game` (one simulated game and the totals), `pressure`
//! (battlefield, combat and race), `profile` (opponents and habits from the
//! own games), `advice` (recommendations from the key figures).

use crate::carddb::{Card, CardDb, GrpId};
use crate::turnplan;

mod advice;
mod game;
mod pressure;
mod profile;

use advice::advice;
use game::{Game, Tally};
use pressure::Board;
pub use pressure::{OPP_CURVE, Pressure, START_LIFE};
pub use profile::{
    Habits, MIN_HABIT_TURNS, MIN_PROFILE_GAMES, Opponent, Trial, habits_from, opponent_from, trial,
};

/// Simulated turns per game.
pub const TURNS: usize = 10;
/// Turn after which flooding is measured.
pub const FLOOD_TURN: usize = 6;
/// The fixed fast opponent only tests the start: this many turns.
pub const FAST_TURNS: usize = 6;
/// Default number of simulated games.
pub const DEFAULT_GAMES: usize = 1000;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SimReport {
    pub games: usize,
    /// Turns played (values in the per-turn fields after that are 0).
    pub turns: usize,
    pub size: usize,
    pub lands: usize,
    /// Share of games with a mulligan.
    pub mulligan: f32,
    /// Share of games with at least T lands in play on turn T (T = 1..4).
    pub on_curve: [f32; 4],
    /// Share of games in which a card on turns 2–4 was unaffordable only
    /// because of its colors.
    pub color_screw: f32,
    /// Color missing most often then (index W, U, B, R, G).
    pub missing_color: Option<usize>,
    /// Share of games with flooding (after turn 6: 5+ lands in play, 2+ in hand).
    pub flood: f32,
    /// Average mana used and available per turn.
    pub mana_used: [f32; TURNS],
    pub mana_avail: [f32; TURNS],
    /// How well the deck holds up against a fast opponent.
    pub pressure: Pressure,
    /// Who it played against (e.g. "fast opponent").
    pub opponent: String,
    /// Played with the own play habits?
    pub with_habits: bool,
    /// Short recommendations, most important first.
    pub advice: Vec<String>,
    /// No problems found (then `advice` only contains praise).
    pub smooth: bool,
}

/// Small, reproducible random number generator (xorshift64*).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Random rounding: 2.3 becomes 3 in 30 % of cases, otherwise 2.
    fn round(&mut self, x: f32) -> i32 {
        let base = x.max(0.0).floor();
        base as i32 + self.chance(x.max(0.0) - base) as i32
    }

    /// `true` with probability `p`.
    fn chance(&mut self, p: f32) -> bool {
        (self.next() % 10_000) as f32 / 10_000.0 < p
    }

    fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            let j = (self.next() % (i as u64 + 1)) as usize;
            v.swap(i, j);
        }
    }
}

fn is_land(db: &CardDb, grp: GrpId) -> bool {
    db.cards.get(&grp).is_some_and(Card::is_land)
}

fn lands_in(db: &CardDb, cards: &[GrpId]) -> usize {
    cards.iter().filter(|g| is_land(db, **g)).count()
}

/// Plays `games` games and evaluates them. `deck` = grpIds with repetition.
pub fn simulate(deck: &[GrpId], db: &CardDb, games: usize, seed: u64) -> SimReport {
    simulate_with(deck, db, games, seed, &Opponent::fast(), None)
}

/// Like [`simulate`], but against `opponent` and – if given – with the own
/// play habits (skipping better plays, skipping attacks).
pub fn simulate_with(
    deck: &[GrpId],
    db: &CardDb,
    games: usize,
    seed: u64,
    opponent: &Opponent,
    habits: Option<&Habits>,
) -> SimReport {
    let mut r = SimReport {
        games,
        opponent: opponent.label.clone(),
        turns: opponent.turns.clamp(FLOOD_TURN, TURNS),
        with_habits: habits.is_some(),
        size: deck.len(),
        lands: deck.iter().filter(|g| is_land(db, **g)).count(),
        ..Default::default()
    };
    if deck.len() < 8 || games == 0 {
        return r;
    }
    let mut rng = Rng(seed | 1);
    let mut tally = Tally::default();
    for game in 0..games {
        let mut g = Game::deal(deck, db, game % 2 == 0, &mut rng);
        for turn in 1..=r.turns {
            if !g.on_play {
                g.opp_turn(turn, opponent, &mut rng);
            }
            g.my_turn(turn, opponent, habits, &mut rng);
            if g.on_play {
                g.opp_turn(turn, opponent, &mut rng);
            }
            g.end_turn(turn);
        }
        tally.add(&g);
    }
    tally.fill(&mut r);
    r.advice = advice(&r);
    r.smooth = r.advice.is_empty();
    if r.smooth {
        r.advice.push(tr!("sim_advice.deck_runs_smoothly_sample"));
    }
    r
}

/// Convert a decklist (names) into grpIds; unknown cards drop out.
pub fn deck_from_entries<'a>(
    entries: impl IntoIterator<Item = (&'a str, u32)>,
    db: &CardDb,
) -> Vec<GrpId> {
    let mut out = Vec::new();
    for (name, qty) in entries {
        if let Some(c) = db.resolve(name).and_then(|k| db.representative(&k)) {
            out.extend(std::iter::repeat_n(c.grp_id, qty as usize));
        }
    }
    out
}

#[cfg(test)]
#[path = "../tests/sim.rs"]
mod tests;
