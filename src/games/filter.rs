//! Choice of games for "My games": by game mode (groups instead of single
//! Arena events) and by deck. So Brawl and Standard games or different decks
//! don't mix in one analysis.

use super::GameRecord;
use super::arena::event;
use crate::carddb::{CardDb, GrpId, name_key};

/// Group of Arena events with comparable games.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GameMode {
    /// Ranked (Bo1 and Bo3, all formats).
    Ranked,
    /// "Play" without rank.
    Play,
    Brawl,
    /// Draft, sealed, cube.
    Limited,
    /// Events with a fixed or special deck (welcome duels, starter decks,
    /// Jump In, color challenge …).
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

    /// Assigns an Arena event to a group.
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
            GameMode::Ranked => tr!("games_filter.ranked"),
            GameMode::Play => tr!("games_filter.play"),
            GameMode::Brawl => tr!("games_filter.brawl"),
            GameMode::Limited => tr!("games_filter.limited_draft_sealed"),
            GameMode::Event => tr!("games_filter.events_starter_decks"),
        }
    }
}

/// Key of a decklist: the same cards in another printing = the same deck.
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

/// Which games are analysed; `None` = all.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GamesFilter {
    pub mode: Option<GameMode>,
    /// Key of the decklist (see [`deck_key`]).
    pub deck: Option<Vec<String>>,
}

impl GamesFilter {
    /// Mode and deck of game `g` – preselection with the last game.
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
