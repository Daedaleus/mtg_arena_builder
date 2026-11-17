//! Spielstil: Welche Art Deck spielst du, womit gewinnst du – und welche
//! Turnierdecks passen deshalb zu dir?
//!
//! Decks werden grob in drei Typen eingeteilt (Aggro, Midrange, Control),
//! anhand von Manakurve, Kreaturenanteil und Interaktion (Entfernung,
//! Konter, Kartenziehen). Dazu kommen Gewohnheiten aus den eigenen Partien.

use crate::carddb::{Card, CardDb, ROLE_COUNTER, ROLE_DRAW, ROLE_REMOVAL};
use crate::deck::Deck;
use crate::games::{GameRecord, Report};

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
    pub fn name(self) -> &'static str {
        match self {
            Style::Aggro => "Aggro",
            Style::Midrange => "Midrange",
            Style::Control => "Control",
        }
    }

    /// Kurze Beschreibung für Einsteiger.
    pub fn description(self) -> &'static str {
        match self {
            Style::Aggro => {
                "viele günstige Kreaturen, früh angreifen und den Gegner schnell besiegen"
            }
            Style::Midrange => {
                "solide Kreaturen und etwas Entfernung, flexibel in beide Richtungen"
            }
            Style::Control => {
                "Entfernung, Konter und Kartenziehen – das Spiel lang machen und spät gewinnen"
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

/// Wertet die Partien aus. `report` ist die Auswertung derselben Partien.
pub fn analyze(games: &[GameRecord], db: &CardDb, report: &Report) -> PlayerStyle {
    let mut records: Vec<StyleRecord> = Vec::new();
    for g in games
        .iter()
        .filter(|g| g.won.is_some() && !g.deck.is_empty())
    {
        let style = DeckProfile::of_ids(&g.deck, db).style();
        let rec = match records.iter_mut().find(|r| r.style == style) {
            Some(r) => r,
            None => {
                records.push(StyleRecord {
                    style,
                    games: 0,
                    wins: 0,
                });
                records.last_mut().expect("eben angelegt")
            }
        };
        rec.games += 1;
        rec.wins += (g.won == Some(true)) as usize;
    }
    records.sort_by_key(|r| std::cmp::Reverse(r.games));

    let my_turns: Vec<_> = games
        .iter()
        .filter(|g| g.complete)
        .flat_map(|g| g.turns.iter().filter(|t| t.mine && t.turn >= 3))
        .collect();
    let attack_rate = if my_turns.is_empty() {
        0.0
    } else {
        my_turns.iter().filter(|t| t.attacked).count() as f32 / my_turns.len() as f32
    };
    let count = |title: &str| {
        report
            .categories
            .iter()
            .find(|c| c.title == title)
            .map_or(0, |c| c.items.len())
    };
    let per_turn = |n: usize| {
        if report.my_turns == 0 {
            0.0
        } else {
            n as f32 / report.my_turns as f32
        }
    };
    let passive = per_turn(
        count("Mana ungenutzt")
            + count("Besserer Zug möglich")
            + count("Karte lange nicht gespielt"),
    );
    let skipped_attacks = per_turn(count("Gefahrlosen Angriff ausgelassen"));

    let mut traits = Vec::new();
    if !my_turns.is_empty() {
        traits.push(format!(
            "Du greifst in {:.0} % deiner Züge (ab Zug 3) an.",
            attack_rate * 100.0
        ));
    }
    if report.avg_turns > 0.0 {
        traits.push(format!(
            "Deine Partien dauern im Schnitt {:.0} Züge{}.",
            report.avg_turns,
            if report.avg_turns >= 14.0 {
                " – eher lange Partien"
            } else if report.avg_turns <= 9.0 {
                " – eher kurze Partien"
            } else {
                ""
            }
        ));
    }
    if passive >= 0.15 {
        traits.push(
            "Du lässt oft Mana ungenutzt oder hältst Karten lange zurück – einfache, direkte Decks fallen dir vermutlich leichter."
                .into(),
        );
    }
    if skipped_attacks >= 0.15 {
        traits.push("Du lässt gefahrlose Angriffe öfter aus – du spielst eher vorsichtig.".into());
    }

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
    let (recommended, reason) = match best {
        Some(b) if b.rate() >= others_best(b.style) + 0.1 => (
            b.style,
            format!(
                "Mit {}-Decks gewinnst du am häufigsten ({} von {} Partien).",
                b.style.name(),
                b.wins,
                b.games
            ),
        ),
        _ if passive >= 0.15 || skipped_attacks >= 0.15 => (
            Style::Aggro,
            "Aggro-Decks sind am einfachsten zu spielen: Kreaturen ausspielen und angreifen. \
             Das passt zu den Stellen, an denen du bisher Mana oder Angriffe liegen lässt."
                .into(),
        ),
        _ => match records.first() {
            Some(r) => (
                r.style,
                format!(
                    "Du spielst am liebsten {} – noch zu wenige Partien, um andere Typen zu vergleichen.",
                    r.style.name()
                ),
            ),
            None => (
                Style::Aggro,
                "Noch keine Partien mit bekanntem Deck. Für den Einstieg sind Aggro-Decks am einfachsten.".into(),
            ),
        },
    };
    PlayerStyle {
        games: games.len(),
        records,
        attack_rate,
        avg_turns: report.avg_turns,
        traits,
        recommended,
        reason,
    }
}

/// Ein Turnierdeck als Empfehlungskandidat.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub deck_id: u32,
    /// Archetyp bzw. Bezeichnung.
    pub label: String,
    /// Woher die Liste stammt, z. B. „Turnierdecks“.
    pub source: &'static str,
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
