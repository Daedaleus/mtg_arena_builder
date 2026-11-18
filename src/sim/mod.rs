//! Probehände: spielt ein Deck viele Male gegen ein leeres Feld durch
//! (Züge 1–6) und misst, wie oft Länder oder Farben fehlen und wie viel des
//! verfügbaren Manas genutzt wird.
//!
//! Gespielt wird mit demselben Zugvorschlag wie im Begleitfenster
//! ([`crate::turnplan`]): Land legen, dann das Mana möglichst gut nutzen.
//! Mulligan: Starthände mit 0–1 oder 6–7 Ländern werden einmal auf 6 Karten
//! getauscht (London-Mulligan: 7 ziehen, eine Karte unter die Bibliothek).
//! Gleichzeitig spielt ein schneller Gegner mit (siehe [`pressure`]).

use crate::carddb::{Card, CardDb};
use crate::turnplan;

mod pressure;
mod profile;

use pressure::Board;
pub use pressure::{OPP_CURVE, Pressure, START_LIFE};
pub use profile::{
    Habits, MIN_HABIT_TURNS, MIN_PROFILE_GAMES, Opponent, Trial, habits_from, opponent_from, trial,
};

/// Simulierte Züge je Partie.
pub const TURNS: usize = 6;
/// Standardzahl der simulierten Partien.
pub const DEFAULT_GAMES: usize = 1000;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SimReport {
    pub games: usize,
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
    /// Anteil der Partien mit Länderflut (Zug 6: 5+ Länder im Spiel, 2+ auf der Hand).
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

const COLOR_NAMES: [&str; 5] = ["Weiß", "Blau", "Schwarz", "Rot", "Grün"];

fn is_land(db: &CardDb, grp: u32) -> bool {
    db.cards.get(&grp).is_some_and(Card::is_land)
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
        with_habits: habits.is_some(),
        size: deck.len(),
        lands: deck.iter().filter(|g| is_land(db, **g)).count(),
        ..Default::default()
    };
    if deck.len() < 8 || games == 0 {
        return r;
    }
    let mut rng = Rng(seed | 1);
    let (mut mulligans, mut screws, mut floods) = (0usize, 0usize, 0usize);
    let mut on_curve = [0usize; 4];
    let mut missing = [0usize; 5];
    let mut used = [0u64; TURNS];
    let mut avail = [0u64; TURNS];
    let mut life_sum = [0i64; TURNS];
    let mut opp_life_sum = [0i64; TURNS];
    let (mut low, mut dead, mut won, mut ahead, mut kills) = (0usize, 0usize, 0usize, 0usize, 0u64);

    for game in 0..games {
        let on_play = game % 2 == 0;
        let mut library = deck.to_vec();
        rng.shuffle(&mut library);
        let mut hand: Vec<u32> = library.drain(..7).collect();
        let lands_in = |h: &[u32]| h.iter().filter(|g| is_land(db, **g)).count();
        if !(2..=5).contains(&lands_in(&hand)) {
            mulligans += 1;
            library.append(&mut hand);
            rng.shuffle(&mut library);
            hand = library.drain(..7).collect();
            // Eine Karte unter die Bibliothek: bei vielen Ländern ein Land,
            // sonst die teuerste Nichtland-Karte.
            let bottom = if lands_in(&hand) >= 4 {
                hand.iter().position(|g| is_land(db, *g))
            } else {
                hand.iter()
                    .enumerate()
                    .filter(|(_, g)| !is_land(db, **g))
                    .max_by_key(|(_, g)| db.cards.get(g).map_or(0, |c| c.cmc))
                    .map(|(i, _)| i)
            };
            if let Some(i) = bottom {
                library.push(hand.remove(i));
            }
        }

        let mut battlefield: Vec<u32> = Vec::new();
        let mut permanents: Vec<u32> = Vec::new();
        let mut screwed = false;
        let mut missing_here = [false; 5];
        let mut board = Board::new();
        // Gestorbene Manakreaturen liefern kein Mana mehr.
        let opp_turn =
            |board: &mut Board, permanents: &mut Vec<u32>, turn: usize, rng: &mut Rng| {
                let new = opponent.creature(turn, rng);
                for grp in board.opp_turn_with(new) {
                    if let Some(i) = permanents.iter().position(|g| *g == grp) {
                        permanents.remove(i);
                    }
                }
            };
        for turn in 1..=TURNS {
            if !on_play {
                opp_turn(&mut board, &mut permanents, turn, &mut rng);
            }
            board.start_my_turn();
            if (turn > 1 || !on_play) && !library.is_empty() {
                hand.push(library.remove(0));
            }
            let plan = turnplan::plan(
                db,
                &hand,
                &battlefield,
                &permanents,
                false,
                &board.opp_creatures(),
                board.mine.len() as u32,
            );
            if let Some(land) = plan.land
                && let Some(i) = hand.iter().position(|g| *g == land)
            {
                battlefield.push(hand.remove(i));
            }
            // Gewohnheit: manchmal den teuersten Zauber des Vorschlags nicht spielen.
            let mut spells = plan.spells.clone();
            let mut mana_used = plan.mana_used;
            if let Some(h) = habits
                && !spells.is_empty()
                && rng.chance(h.weaker_turn)
                && let Some((i, cmc)) = spells
                    .iter()
                    .enumerate()
                    .map(|(i, g)| (i, db.cards.get(g).map_or(0, |c| c.cmc)))
                    .max_by_key(|(_, cmc)| *cmc)
            {
                spells.remove(i);
                mana_used -= cmc;
            }
            for g in &spells {
                if let Some(i) = hand.iter().position(|h| h == g) {
                    let card = hand.remove(i);
                    if let Some(c) = db.cards.get(&card) {
                        if c.mana.is_some() {
                            permanents.push(card);
                        }
                        board.cast(c);
                    }
                }
            }
            used[turn - 1] += u64::from(mana_used);
            avail[turn - 1] += u64::from(plan.mana_total);
            if turn <= 4 && battlefield.len() >= turn {
                on_curve[turn - 1] += 1;
            }
            let hesitant = habits.is_some_and(|h| rng.chance(h.skip_attack));
            for grp in board.my_attack_with(hesitant) {
                if let Some(i) = permanents.iter().position(|g| *g == grp) {
                    permanents.remove(i);
                }
            }
            if on_play {
                opp_turn(&mut board, &mut permanents, turn, &mut rng);
            }
            life_sum[turn - 1] += i64::from(board.life.max(0));
            opp_life_sum[turn - 1] += i64::from(board.opp_life.max(0));
            // Farbe fehlt: Karte wäre vom Manawert her bezahlbar gewesen.
            if (2..=4).contains(&turn) {
                let left = plan.mana_total - plan.mana_used;
                let sources = turnplan::sources(db, &battlefield, &permanents);
                let have = sources.iter().fold(0u8, |m, c| m | c);
                for c in hand.iter().filter_map(|g| db.cards.get(g)) {
                    if !c.is_land()
                        && c.cmc > 0
                        && c.cmc <= left
                        && !turnplan::can_pay(&[c], &sources)
                    {
                        screwed = true;
                        let (_, pips) = c.pips();
                        for (i, n) in pips.iter().enumerate() {
                            if *n > 0 && have & (1 << i) == 0 {
                                missing_here[i] = true;
                            }
                        }
                    }
                }
            }
        }
        screws += screwed as usize;
        dead += (board.life <= 0) as usize;
        won += (board.opp_life <= 0) as usize;
        ahead += (board.life > 0 && board.opp_life < board.life) as usize;
        low += (board.life <= 5) as usize;
        kills += u64::from(board.kills);
        for (i, m) in missing_here.iter().enumerate() {
            missing[i] += *m as usize;
        }
        if battlefield.len() >= 5 && lands_in(&hand) >= 2 {
            floods += 1;
        }
    }

    let share = |n: usize| n as f32 / games as f32;
    r.mulligan = share(mulligans);
    r.color_screw = share(screws);
    r.flood = share(floods);
    for t in 0..4 {
        r.on_curve[t] = share(on_curve[t]);
    }
    for t in 0..TURNS {
        r.mana_used[t] = used[t] as f32 / games as f32;
        r.mana_avail[t] = avail[t] as f32 / games as f32;
    }
    for t in 0..TURNS {
        r.pressure.life[t] = life_sum[t] as f32 / games as f32;
        r.pressure.opp_life[t] = opp_life_sum[t] as f32 / games as f32;
    }
    r.pressure.low = share(low);
    r.pressure.dead = share(dead);
    r.pressure.won = share(won);
    r.pressure.ahead = share(ahead);
    r.pressure.kills = kills as f32 / games as f32;
    r.missing_color = (0..5)
        .filter(|i| missing[*i] > 0)
        .max_by_key(|i| missing[*i]);
    r.advice = advice(&r);
    r
}

/// Empfehlungen aus den Kennzahlen.
fn advice(r: &SimReport) -> Vec<String> {
    let pct = |x: f32| (x * 100.0).round() as u32;
    let mut out = Vec::new();
    if r.on_curve[2] < 0.6 {
        out.push(format!(
            "Zu wenige Länder: Nur in {} % der Partien liegen im 3. Zug drei Länder. 1–2 Länder mehr spielen.",
            pct(r.on_curve[2])
        ));
    }
    if r.flood >= 0.25 {
        out.push(format!(
            "Zu viele Länder: In {} % der Partien liegen im 6. Zug noch zwei oder mehr Länder auf der Hand. 1–2 Länder durch günstige Karten ersetzen.",
            pct(r.flood)
        ));
    }
    if r.color_screw >= 0.2 {
        let color = r
            .missing_color
            .map(|i| format!(" (meist {})", COLOR_NAMES[i]))
            .unwrap_or_default();
        out.push(format!(
            "Farben fehlen: In {} % der Partien war in Zug 2–4 eine Karte nur wegen der Farbe nicht spielbar{color}. Mehr Länder dieser Farbe oder Zweifarbländer.",
            pct(r.color_screw)
        ));
    }
    // Zu langsam nur, wenn man das Rennen nicht selbst gewinnt.
    if r.pressure.low >= 0.4 && r.pressure.ahead < r.pressure.low {
        out.push(format!(
            "Gegen schnelle Decks zu langsam: In {} % der Partien hast du nach Zug 6 höchstens 5 Leben. \
             Mehr günstige Kreaturen zum Blocken oder Entfernung mit Manawert 1–2.",
            pct(r.pressure.low)
        ));
    }
    let used: f32 = r.mana_used[..4].iter().sum();
    let avail: f32 = r.mana_avail[..4].iter().sum();
    if avail > 0.0 && used / avail < 0.55 {
        out.push(format!(
            "Kurve zu hoch: In den Zügen 1–4 wird nur {} % des Manas genutzt. Mehr günstige Karten (Manawert 1–2).",
            pct(used / avail)
        ));
    }
    if r.mulligan >= 0.25 {
        out.push(format!(
            "Viele Mulligans ({} %): Die Länderzahl liegt weit weg von etwa 40 % des Decks.",
            pct(r.mulligan)
        ));
    }
    if out.is_empty() {
        out.push("Das Deck läuft in den Probehänden rund – Länder und Kurve passen.".into());
    }
    out
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
