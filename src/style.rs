//! Spielstil: Welche Art Deck spielst du, womit gewinnst du – und welche
//! Turnierdecks passen deshalb zu dir?
//!
//! Decks werden grob in drei Typen eingeteilt (Aggro, Midrange, Control),
//! anhand von Manakurve, Kreaturenanteil und Interaktion (Entfernung,
//! Konter, Kartenziehen). Dazu kommen Gewohnheiten aus den eigenen Partien.

use crate::carddb::{Card, CardDb, ROLE_COUNTER, ROLE_DRAW, ROLE_REMOVAL};
use crate::deck::Deck;
use crate::games::{GameRecord, Mistake, Report, TurnStat};

/// Mindestens so viele Partien je Typ, bevor die Siegquote zählt.
pub const MIN_GAMES: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Style {
    Aggro,
    Midrange,
    Control,
}

pub const STYLES: [Style; 3] = [Style::Aggro, Style::Midrange, Style::Control];

impl Style {
    pub fn name(self) -> String {
        match self {
            Style::Aggro => tr!("style.aggro"),
            Style::Midrange => tr!("style.midrange"),
            Style::Control => tr!("style.control"),
        }
    }

    /// Kurze Beschreibung für Einsteiger.
    pub fn description(self) -> String {
        match self {
            Style::Aggro => {
                tr!("style.viele_guenstige_kreaturen_frueh")
            }
            Style::Midrange => {
                tr!("style.solide_kreaturen_etwas_entfernung")
            }
            Style::Control => {
                tr!("style.entfernung_konter_kartenziehen_spiel")
            }
        }
    }
}

/// Kennzahlen der Nichtland-Karten eines Decks.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DeckProfile {
    pub avg_cmc: f32,
    /// Anteil Kreaturen (0..1).
    pub creatures: f32,
    /// Anteil Entfernung, Konter und Kartenziehen (0..1).
    pub interaction: f32,
}

impl DeckProfile {
    pub fn of(cards: &[&Card]) -> DeckProfile {
        let spells: Vec<&&Card> = cards.iter().filter(|c| !c.is_land()).collect();
        if spells.is_empty() {
            return DeckProfile::default();
        }
        let n = spells.len() as f32;
        let share = |f: &dyn Fn(&Card) -> bool| spells.iter().filter(|c| f(c)).count() as f32 / n;
        DeckProfile {
            avg_cmc: spells.iter().map(|c| c.cmc as f32).sum::<f32>() / n,
            creatures: share(&|c| c.is_creature()),
            interaction: share(&|c| {
                !c.is_creature()
                    && (c.has_role(ROLE_REMOVAL)
                        || c.has_role(ROLE_COUNTER)
                        || c.has_role(ROLE_DRAW))
            }),
        }
    }

    /// Profil einer Deckliste (Hauptdeck, nach Namen).
    pub fn of_deck(deck: &Deck, db: &CardDb) -> DeckProfile {
        let cards: Vec<&Card> = deck
            .main
            .iter()
            .filter_map(|e| {
                let c = db.resolve(&e.name).and_then(|k| db.representative(&k))?;
                Some(std::iter::repeat_n(c, e.qty as usize))
            })
            .flatten()
            .collect();
        DeckProfile::of(&cards)
    }

    /// Profil eines Decks aus grpIds (z. B. eigene Partien).
    pub fn of_ids(deck: &[u32], db: &CardDb) -> DeckProfile {
        let cards: Vec<&Card> = deck.iter().filter_map(|g| db.cards.get(g)).collect();
        DeckProfile::of(&cards)
    }

    pub fn style(&self) -> Style {
        if self.avg_cmc <= 2.6 && self.creatures >= 0.5 {
            Style::Aggro
        } else if self.interaction >= 0.35 || self.creatures < 0.3 {
            Style::Control
        } else {
            Style::Midrange
        }
    }
}

/// Bilanz mit einem Decktyp.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StyleRecord {
    pub style: Style,
    pub games: usize,
    pub wins: usize,
}

impl StyleRecord {
    pub fn rate(&self) -> f32 {
        if self.games == 0 {
            0.0
        } else {
            self.wins as f32 / self.games as f32
        }
    }
}

/// Ergebnis der Spielstil-Analyse.
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerStyle {
    pub games: usize,
    /// Bilanz je Decktyp (nur gespielte), meistgespielter zuerst.
    pub records: Vec<StyleRecord>,
    /// Anteil eigener Züge (ab Zug 3) mit Angriff.
    pub attack_rate: f32,
    pub avg_turns: f32,
    /// Auffällige Gewohnheiten in Worten.
    pub traits: Vec<String>,
    pub recommended: Style,
    /// Warum dieser Typ empfohlen wird.
    pub reason: String,
}

/// Ab diesem Anteil der eigenen Züge gilt ein Fehler als Gewohnheit.
const HABIT_SHARE: f32 = 0.15;

/// Wertet die Partien aus. `report` ist die Auswertung derselben Partien.
pub fn analyze(games: &[GameRecord], db: &CardDb, report: &Report) -> PlayerStyle {
    let records = style_records(games, db);
    let attack_rate = attack_rate(games);
    let per_turn = |n: usize| {
        if report.my_turns == 0 {
            0.0
        } else {
            n as f32 / report.my_turns as f32
        }
    };
    let habits = Habits {
        passive: per_turn(
            report.count(Mistake::UnusedMana)
                + report.count(Mistake::BetterTurn)
                + report.count(Mistake::HeldCard),
        ),
        skipped_attacks: per_turn(report.count(Mistake::SkippedAttack)),
    };
    let (recommended, reason) = recommend_style(&records, &habits);
    PlayerStyle {
        games: games.len(),
        traits: traits(attack_rate, report.avg_turns, &habits),
        records,
        attack_rate: attack_rate.unwrap_or(0.0),
        avg_turns: report.avg_turns,
        recommended,
        reason,
    }
}

/// Anteile eigener Züge mit typischen Fehlern.
struct Habits {
    /// Mana ungenutzt, besserer Zug möglich oder Karte lange zurückgehalten.
    passive: f32,
    /// Gefahrlosen Angriff ausgelassen.
    skipped_attacks: f32,
}

impl Habits {
    fn noticeable(&self) -> bool {
        self.passive >= HABIT_SHARE || self.skipped_attacks >= HABIT_SHARE
    }
}

/// Bilanz je Decktyp (Partien mit bekanntem Deck und Ergebnis), meistgespielter zuerst.
fn style_records(games: &[GameRecord], db: &CardDb) -> Vec<StyleRecord> {
    let mut records: Vec<StyleRecord> = Vec::new();
    for g in games
        .iter()
        .filter(|g| g.won.is_some() && !g.deck.is_empty())
    {
        let style = DeckProfile::of_ids(&g.deck, db).style();
        let i = match records.iter().position(|r| r.style == style) {
            Some(i) => i,
            None => {
                records.push(StyleRecord {
                    style,
                    games: 0,
                    wins: 0,
                });
                records.len() - 1
            }
        };
        records[i].games += 1;
        records[i].wins += (g.won == Some(true)) as usize;
    }
    records.sort_by_key(|r| std::cmp::Reverse(r.games));
    records
}

/// Anteil eigener Züge ab Zug 3 mit Angriff (`None` ohne solche Züge).
fn attack_rate(games: &[GameRecord]) -> Option<f32> {
    let turns: Vec<&TurnStat> = games
        .iter()
        .filter(|g| g.complete)
        .flat_map(|g| g.turns.iter().filter(|t| t.mine && t.turn >= 3))
        .collect();
    (!turns.is_empty())
        .then(|| turns.iter().filter(|t| t.attacked).count() as f32 / turns.len() as f32)
}

/// Auffällige Gewohnheiten in Worten.
fn traits(attack_rate: Option<f32>, avg_turns: f32, habits: &Habits) -> Vec<String> {
    let mut traits = Vec::new();
    if let Some(rate) = attack_rate {
        traits.push(tr!("style.greifst_deiner_zuege_ab", v = rate * 100.0 : {:.0}));
    }
    if avg_turns > 0.0 {
        let length = if avg_turns >= 14.0 {
            tr!("style.eher_lange_partien")
        } else if avg_turns <= 9.0 {
            tr!("style.eher_kurze_partien")
        } else {
            String::new()
        };
        traits.push(tr!("style.partien_dauern_schnitt_zuege", avg_turns = avg_turns : {:.0}, length = length));
    }
    if habits.passive >= HABIT_SHARE {
        traits.push(tr!("style.laesst_oft_mana_ungenutzt"));
    }
    if habits.skipped_attacks >= HABIT_SHARE {
        traits.push(tr!("style.laesst_gefahrlose_angriffe_oefter"));
    }
    traits
}

/// Empfohlener Decktyp mit Begründung: der mit klar der besten Siegquote,
/// sonst bei passiven Gewohnheiten Aggro, sonst der meistgespielte.
fn recommend_style(records: &[StyleRecord], habits: &Habits) -> (Style, String) {
    let best = records
        .iter()
        .filter(|r| r.games >= MIN_GAMES)
        .max_by(|a, b| a.rate().total_cmp(&b.rate()));
    let others_best = |s: Style| {
        records
            .iter()
            .filter(|r| r.style != s && r.games >= MIN_GAMES)
            .map(StyleRecord::rate)
            .fold(0.0f32, f32::max)
    };
    match best {
        Some(b) if b.rate() >= others_best(b.style) + 0.1 => (
            b.style,
            tr!(
                "style.decks_gewinnst_haeufigsten_partien",
                style = b.style.name(),
                wins = b.wins,
                games = b.games
            ),
        ),
        _ if habits.noticeable() => (Style::Aggro, tr!("style.aggro_decks_einfachsten_spielen")),
        _ => match records.first() {
            Some(r) => (
                r.style,
                tr!("style.spielst_liebsten_noch_wenige", style = r.style.name()),
            ),
            None => (Style::Aggro, tr!("style.noch_keine_partien_bekanntem")),
        },
    }
}

/// Ein Turnierdeck als Empfehlungskandidat.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub deck_id: u32,
    /// Archetyp bzw. Bezeichnung.
    pub label: String,
    /// Woher die Liste stammt, z. B. „Turnierdecks“.
    pub source: String,
    pub style: Style,
    pub summary: crate::deck::Summary,
}

/// Bis zu `n` Decks des gewünschten Typs, für die eigene Sammlung günstigste
/// zuerst; je Bezeichnung nur das günstigste.
pub fn recommend(
    cands: &[Candidate],
    style: Style,
    wildcards: [u32; 4],
    n: usize,
) -> Vec<Candidate> {
    let mut list: Vec<&Candidate> = cands.iter().filter(|c| c.style == style).collect();
    list.sort_by_key(|c| crate::present::cost_key(&c.summary, wildcards));
    let mut out: Vec<Candidate> = Vec::new();
    for c in list {
        if out.len() >= n {
            break;
        }
        if !out.iter().any(|o| o.label == c.label) {
            out.push(c.clone());
        }
    }
    out
}

#[cfg(test)]
#[path = "tests/style.rs"]
mod tests;
