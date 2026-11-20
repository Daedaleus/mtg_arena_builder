//! Auswahl der Partien für „Meine Partien“: nach Spielmodus (Gruppen statt
//! einzelner Arena-Events) und nach Deck. So mischen sich Brawl- und
//! Standard-Partien oder verschiedene Decks nicht in einer Auswertung.

use super::GameRecord;
use super::arena::event;
use crate::carddb::{CardDb, GrpId, name_key};

/// Gruppe von Arena-Events mit vergleichbaren Partien.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GameMode {
    /// Rangliste (Bo1 und Bo3, alle Formate).
    Ranked,
    /// „Spielen“ ohne Rang.
    Play,
    Brawl,
    /// Draft, Sealed, Cube.
    Limited,
    /// Events mit vorgegebenem oder besonderem Deck (Willkommens-Duelle,
    /// Starterdecks, Jump In, Farb-Challenge …).
    Event,
}

impl GameMode {
    pub const ALL: [GameMode; 5] = [
        GameMode::Ranked,
        GameMode::Play,
        GameMode::Brawl,
        GameMode::Limited,
        GameMode::Event,
    ];

    /// Ordnet ein Arena-Event einer Gruppe zu.
    pub fn of(event_name: &str) -> GameMode {
        let lower = event_name.to_lowercase();
        if lower.contains("brawl") {
            GameMode::Brawl
        } else if ["draft", "sealed", "cube"]
            .iter()
            .any(|w| lower.contains(w))
        {
            GameMode::Limited
        } else if event_name == event::LADDER
            || event_name == event::LADDER_BO3
            || lower.ends_with("_ladder")
        {
            GameMode::Ranked
        } else if event_name == event::PLAY || event_name == event::PLAY_BO3 {
            GameMode::Play
        } else {
            GameMode::Event
        }
    }

    pub fn label(self) -> String {
        match self {
            GameMode::Ranked => tr!("games_filter.rangliste"),
            GameMode::Play => tr!("games_filter.spielen"),
            GameMode::Brawl => tr!("games_filter.brawl"),
            GameMode::Limited => tr!("games_filter.limited"),
            GameMode::Event => tr!("games_filter.events"),
        }
    }
}

/// Schlüssel einer Deckliste: gleiche Karten in anderem Druck = gleiches Deck.
pub fn deck_key(deck: &[GrpId], db: &CardDb) -> Vec<String> {
    let mut key: Vec<String> = deck
        .iter()
        .map(|g| {
            db.cards
                .get(g)
                .map(|c| name_key(&c.name))
                .unwrap_or_else(|| g.to_string())
        })
        .collect();
    key.sort();
    key
}

/// Welche Partien ausgewertet werden; `None` = alle.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GamesFilter {
    pub mode: Option<GameMode>,
    /// Schlüssel der Deckliste (siehe [`deck_key`]).
    pub deck: Option<Vec<String>>,
}

impl GamesFilter {
    /// Modus und Deck der Partie `g` – Vorbelegung mit der letzten Partie.
    pub fn like(g: &GameRecord, db: &CardDb) -> GamesFilter {
        GamesFilter {
            mode: Some(GameMode::of(&g.event)),
            deck: (!g.deck.is_empty()).then(|| deck_key(&g.deck, db)),
        }
    }

    pub fn matches(&self, g: &GameRecord, db: &CardDb) -> bool {
        self.mode.is_none_or(|m| GameMode::of(&g.event) == m)
            && self
                .deck
                .as_ref()
                .is_none_or(|k| !g.deck.is_empty() && deck_key(&g.deck, db) == *k)
    }
}
