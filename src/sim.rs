//! Probehände: spielt ein Deck viele Male gegen ein leeres Feld durch
//! (Züge 1–6) und misst, wie oft Länder oder Farben fehlen und wie viel des
//! verfügbaren Manas genutzt wird.
//!
//! Gespielt wird mit demselben Zugvorschlag wie im Begleitfenster
//! ([`crate::turnplan`]): Land legen, dann das Mana möglichst gut nutzen.
//! Mulligan: Starthände mit 0–1 oder 6–7 Ländern werden einmal auf 6 Karten
//! getauscht (London-Mulligan: 7 ziehen, eine Karte unter die Bibliothek).

use crate::carddb::{Card, CardDb};
use crate::turnplan;

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
    let mut r = SimReport {
        games,
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

    for game in 0..games {
        let on_play = game % 2 == 0;
        let mut library = deck.to_vec();
        rng.shuffle(&mut library);
        let mut hand: Vec<u32> = library.drain(..7).collect();
        let lands_in = |h: &[u32]| h.iter().filter(|g| is_land(db, **g)).count();
        if !(2..=5).contains(&lands_in(&hand)) {
            mulligans += 1;
            library.extend(hand.drain(..));
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
        for turn in 1..=TURNS {
            if (turn > 1 || !on_play) && !library.is_empty() {
                hand.push(library.remove(0));
            }
            let plan = turnplan::plan(db, &hand, &battlefield, &permanents, false, &[], 0);
            if let Some(land) = plan.land
                && let Some(i) = hand.iter().position(|g| *g == land)
            {
                battlefield.push(hand.remove(i));
            }
            for g in &plan.spells {
                if let Some(i) = hand.iter().position(|h| h == g) {
                    let card = hand.remove(i);
                    if db.cards.get(&card).is_some_and(|c| c.mana.is_some()) {
                        permanents.push(card);
                    }
                }
            }
            used[turn - 1] += u64::from(plan.mana_used);
            avail[turn - 1] += u64::from(plan.mana_total);
            if turn <= 4 && battlefield.len() >= turn {
                on_curve[turn - 1] += 1;
            }
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
#[path = "tests/sim.rs"]
mod tests;
