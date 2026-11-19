//! Probehände: spielt ein Deck viele Male durch und misst, wie oft Länder oder
//! Farben fehlen, wie viel des verfügbaren Manas genutzt wird und wie das Deck
//! gegen einen Gegner besteht.
//!
//! Gespielt wird mit demselben Zugvorschlag wie im Begleitfenster
//! ([`crate::turnplan`]): Land legen, dann das Mana möglichst gut nutzen.
//! Mulligan: Starthände mit 0–1 oder 6–7 Ländern werden einmal auf 6 Karten
//! getauscht (London-Mulligan: 7 ziehen, eine Karte unter die Bibliothek).
//!
//! Aufbau: `game` (eine simulierte Partie und die Summen), `pressure`
//! (Spielfeld, Kampf und Rennen), `profile` (Gegner und Gewohnheiten aus den
//! eigenen Partien), `advice` (Empfehlungen aus den Kennzahlen).

use crate::carddb::{Card, CardDb};
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

/// Simulierte Züge je Partie.
pub const TURNS: usize = 10;
/// Zug, nach dem die Länderflut gemessen wird.
pub const FLOOD_TURN: usize = 6;
/// Der feste schnelle Gegner prüft nur den Start: so viele Züge.
pub const FAST_TURNS: usize = 6;
/// Standardzahl der simulierten Partien.
pub const DEFAULT_GAMES: usize = 1000;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SimReport {
    pub games: usize,
    /// Gespielte Züge (Werte in den Feldern je Zug danach sind 0).
    pub turns: usize,
    pub size: usize,
    pub lands: usize,
    /// Anteil der Partien mit Mulligan.
    pub mulligan: f32,
    /// Anteil der Partien mit mindestens T Ländern im Spiel in Zug T (T = 1..4).
    pub on_curve: [f32; 4],
    /// Anteil der Partien, in denen in Zug 2–4 eine Karte nur wegen der
    /// Farben nicht bezahlbar war.
    pub color_screw: f32,
    /// Farbe, die dabei am häufigsten fehlte (Index W, U, B, R, G).
    pub missing_color: Option<usize>,
    /// Anteil der Partien mit Länderflut (nach Zug 6: 5+ Länder im Spiel, 2+ auf der Hand).
    pub flood: f32,
    /// Durchschnittlich genutztes und verfügbares Mana je Zug.
    pub mana_used: [f32; TURNS],
    pub mana_avail: [f32; TURNS],
    /// Wie gut das Deck einem schnellen Gegner standhält.
    pub pressure: Pressure,
    /// Gegen wen gespielt wurde (z. B. „schneller Gegner“).
    pub opponent: String,
    /// Mit den eigenen Spielgewohnheiten gespielt?
    pub with_habits: bool,
    /// Kurze Empfehlungen, wichtigste zuerst.
    pub advice: Vec<String>,
    /// Keine Probleme gefunden (dann steht in `advice` nur ein Lob).
    pub smooth: bool,
}

/// Kleiner, reproduzierbarer Zufallsgenerator (xorshift64*).
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

    /// Zufällige Rundung: 2,3 wird in 30 % der Fälle zu 3, sonst zu 2.
    fn round(&mut self, x: f32) -> i32 {
        let base = x.max(0.0).floor();
        base as i32 + self.chance(x.max(0.0) - base) as i32
    }

    /// `true` mit Wahrscheinlichkeit `p`.
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

fn is_land(db: &CardDb, grp: u32) -> bool {
    db.cards.get(&grp).is_some_and(Card::is_land)
}

fn lands_in(db: &CardDb, cards: &[u32]) -> usize {
    cards.iter().filter(|g| is_land(db, **g)).count()
}

/// Spielt `games` Partien und wertet sie aus. `deck` = grpIds mit Wiederholung.
pub fn simulate(deck: &[u32], db: &CardDb, games: usize, seed: u64) -> SimReport {
    simulate_with(deck, db, games, seed, &Opponent::fast(), None)
}

/// Wie [`simulate`], aber gegen `opponent` und – falls angegeben – mit den
/// eigenen Spielgewohnheiten (bessere Züge auslassen, Angriffe auslassen).
pub fn simulate_with(
    deck: &[u32],
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
        r.advice
            .push(tr!("sim_advice.deck_laeuft_probehaenden_rund"));
    }
    r
}

/// Deckliste (Namen) in grpIds umwandeln; unbekannte Karten fallen weg.
pub fn deck_from_entries<'a>(
    entries: impl IntoIterator<Item = (&'a str, u32)>,
    db: &CardDb,
) -> Vec<u32> {
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
