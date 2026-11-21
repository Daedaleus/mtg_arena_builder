//! „Deck verbessern“: wertet die eigenen Partien je Deck aus und schlägt
//! Karten zum Tauschen vor.
//!
//! Bewusst vorsichtig: Gewinnraten einzelner Karten sind erst nach vielen
//! Partien aussagekräftig. Bis dahin stützen sich die Vorschläge auf das, was
//! auch nach wenigen Partien belastbar ist – wie du eine Karte tatsächlich
//! nutzt, ob dein Mana passt und was Turnierspieler im selben Archetyp spielen.
//!
//! Aufbau: Datentypen hier; `groups` (Partien je Deck), `mana` (Länder-Check),
//! `analysis` (Kartennutzung, Vergleich, Tausch).

use std::collections::{HashMap, HashSet};

use crate::carddb::{Card, CardDb, GrpId, Rarity, name_key};
use crate::deck::Deck;
use crate::games::GameRecord;
use crate::similar;

mod analysis;
mod groups;
mod mana;

pub use analysis::*;
pub use groups::*;
use mana::*;

/// Ab so vielen vollständigen Partien zeigt die App Gewinnraten je Karte.
pub const MIN_GAMES_FOR_WINRATE: usize = 10;

/// Alle Partien, die mit derselben Deckliste gespielt wurden.
#[derive(Clone, Debug)]
pub struct DeckGroup {
    pub label: String,
    /// Deckliste (grpIds, Mehrfachnennung = Anzahl).
    pub deck: Vec<GrpId>,
    /// Indizes in die Partienliste.
    pub games: Vec<usize>,
    pub wins: usize,
    /// Vorgegebenes Deck eines Events, das man in Arena nicht ändern kann.
    pub fixed: bool,
}

/// Nutzung einer Karte über alle Partien eines Decks.
#[derive(Clone, Debug, Default)]
pub struct CardStat {
    pub key: String,
    pub name: String,
    pub copies: u32,
    pub rarity: Rarity,
    pub cmc: u32,
    pub land: bool,
    /// In wie vielen Partien gezogen bzw. gewirkt.
    pub seen: usize,
    pub cast: usize,
    /// In wie vielen Partien mindestens drei eigene Züge bezahlbar auf der Hand.
    pub held: usize,
    pub damage: u32,
    /// Siege in Partien, in denen die Karte gezogen wurde.
    pub won_when_seen: usize,
    /// Anteil der Turnierlisten des passenden Archetyps, die die Karte spielen.
    pub meta_share: Option<f32>,
}

#[derive(Clone, Debug, Default)]
pub struct ManaCheck {
    pub games: usize,
    /// Partien, in denen in den ersten drei eigenen Zügen Länder fehlten.
    pub screw: usize,
    /// Partien, in denen später Länder nutzlos auf der Hand lagen.
    pub flood: usize,
    pub lands: u32,
    pub size: u32,
    pub verdict: LandVerdict,
}

/// Urteil über die Länderzahl eines Decks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LandVerdict {
    /// Weniger als [`MIN_GAMES_FOR_LANDS`] vollständige Partien.
    #[default]
    TooFewGames,
    /// Oft zu wenige Länder.
    TooFew,
    /// Oft zu viele Länder.
    TooMany,
    Fine,
}

/// Mindestens so viele vollständige Partien für ein Urteil über die Länder.
pub const MIN_GAMES_FOR_LANDS: usize = 3;

impl LandVerdict {
    /// Sollte man die Länderzahl ändern?
    pub fn needs_change(self) -> bool {
        matches!(self, LandVerdict::TooFew | LandVerdict::TooMany)
    }

    pub fn advice(self) -> String {
        match self {
            LandVerdict::TooFewGames => {
                tr!("deckcheck.statement_about_land_count")
            }
            LandVerdict::TooFew => {
                tr!("deckcheck.often_short_lands_play")
            }
            LandVerdict::TooMany => {
                tr!("deckcheck.often_had_too_many")
            }
            LandVerdict::Fine => tr!("deckcheck.land_count_fits_games"),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct MetaMatch {
    pub id: u32,
    pub archetype: String,
    pub lists: usize,
    /// Übereinstimmung der Nichtland-Karten (0–1).
    pub overlap: f32,
    /// Karten, die mindestens die Hälfte der Turnierlisten spielen, du aber nicht:
    /// (Name, Anteil, Exemplare in deiner Sammlung).
    pub missing: Vec<(String, f32, u32)>,
}

#[derive(Clone, Debug)]
pub struct Swap {
    pub out: String,
    pub reason: String,
    /// Ersatz aus der Sammlung (Name, Begründung).
    pub replacement: Option<(String, String)>,
}

#[derive(Clone, Debug, Default)]
pub struct DeckCheck {
    pub games: usize,
    pub complete: usize,
    /// Partien mit Daten zu gezogenen und gewirkten Karten.
    pub tracked: usize,
    pub wins: usize,
    pub cards: Vec<CardStat>,
    pub mana: ManaCheck,
    pub meta: Option<MetaMatch>,
    pub swaps: Vec<Swap>,
}

impl DeckCheck {
    pub fn show_winrates(&self) -> bool {
        self.tracked >= MIN_GAMES_FOR_WINRATE
    }
}

/// Turnierlisten eines Archetyps für den Vergleich.
pub struct MetaArchetype<'a> {
    pub id: u32,
    pub name: String,
    pub lists: Vec<&'a Deck>,
}

/// Deckliste als Text für „Eigene Liste prüfen“ bzw. den Arena-Import.
pub fn deck_text(deck: &[GrpId], db: &CardDb) -> String {
    let mut counts: Vec<(String, u32)> = Vec::new();
    for grp in deck {
        let name = db
            .cards
            .get(grp)
            .map(|c| c.name.clone())
            .unwrap_or_else(|| format!("#{grp}"));
        match counts.iter_mut().find(|(n, _)| *n == name) {
            Some((_, q)) => *q += 1,
            None => counts.push((name, 1)),
        }
    }
    let mut out = String::from("Deck\n");
    for (name, qty) in counts {
        out.push_str(&format!("{qty} {name}\n"));
    }
    out
}

#[cfg(test)]
#[path = "../tests/deckcheck.rs"]
mod tests;
