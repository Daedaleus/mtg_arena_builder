//! Format of a recorded game, derived from the Arena event name (and the deck
//! size where the name alone doesn't tell): Standard, Historic, Brawl …
//! Basis for analyses that only use games of one format.

use super::GameRecord;
use super::arena::event;
use crate::meta::Format;

/// Brawl decks with this many cards (commander included) are Historic Brawl;
/// smaller ones Standard Brawl.
pub const BRAWL_DECK_SIZE: usize = 100;

/// Format a game was played in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GameFormat {
    Standard,
    Historic,
    Explorer,
    Alchemy,
    Timeless,
    /// Historic Brawl (100 cards with commander).
    Brawl,
    /// Standard Brawl (60 cards with commander).
    StandardBrawl,
    /// Draft, sealed, cube.
    Limited,
    /// Events with special rules or fixed decks (welcome duels, starter
    /// decks, Jump In …) and unknown events.
    Other,
}

impl GameFormat {
    pub const ALL: [GameFormat; 9] = [
        GameFormat::Standard,
        GameFormat::Historic,
        GameFormat::Explorer,
        GameFormat::Alchemy,
        GameFormat::Timeless,
        GameFormat::Brawl,
        GameFormat::StandardBrawl,
        GameFormat::Limited,
        GameFormat::Other,
    ];

    /// What can be chosen as the game mode of the app (all but `Other`).
    pub const SELECTABLE: [GameFormat; 8] = [
        GameFormat::Standard,
        GameFormat::Historic,
        GameFormat::Explorer,
        GameFormat::Alchemy,
        GameFormat::Timeless,
        GameFormat::Brawl,
        GameFormat::StandardBrawl,
        GameFormat::Limited,
    ];

    /// Code in `settings.json`.
    pub fn code(self) -> &'static str {
        match self {
            GameFormat::Standard => "standard",
            GameFormat::Historic => "historic",
            GameFormat::Explorer => "explorer",
            GameFormat::Alchemy => "alchemy",
            GameFormat::Timeless => "timeless",
            GameFormat::Brawl => "brawl",
            GameFormat::StandardBrawl => "standardbrawl",
            GameFormat::Limited => "limited",
            GameFormat::Other => "other",
        }
    }

    pub fn from_code(code: &str) -> Option<GameFormat> {
        GameFormat::ALL
            .into_iter()
            .find(|f| f.code().eq_ignore_ascii_case(code))
    }

    /// Game mode for a tournament format (Arena has no Pioneer: Explorer).
    pub fn from_meta(format: Format) -> GameFormat {
        match format {
            Format::Standard => GameFormat::Standard,
            Format::Pioneer | Format::Explorer => GameFormat::Explorer,
            Format::Historic => GameFormat::Historic,
            Format::Alchemy => GameFormat::Alchemy,
        }
    }

    pub fn label(self) -> String {
        match self {
            GameFormat::Standard => tr!("game_format.standard"),
            GameFormat::Historic => tr!("game_format.historic"),
            GameFormat::Explorer => tr!("game_format.explorer"),
            GameFormat::Alchemy => tr!("game_format.alchemy"),
            GameFormat::Timeless => tr!("game_format.timeless"),
            GameFormat::Brawl => tr!("game_format.brawl"),
            GameFormat::StandardBrawl => tr!("game_format.standard_brawl"),
            GameFormat::Limited => tr!("game_format.limited"),
            GameFormat::Other => tr!("game_format.other"),
        }
    }

    /// Decks are built and checked for this mode (not for limited, where the
    /// deck comes from the draft).
    pub fn has_own_decks(self) -> bool {
        !matches!(self, GameFormat::Limited | GameFormat::Other)
    }

    /// Format of an Arena event; `deck_size` (commander included, 0 =
    /// unknown) separates Historic and Standard Brawl when the name doesn't.
    pub fn of(event_name: &str, deck_size: usize) -> GameFormat {
        let lower = event_name.to_lowercase();
        let words: Vec<&str> = lower.split('_').collect();
        let has = |w: &str| words.contains(&w);
        if lower.contains("brawl") {
            return if has("historic") || deck_size >= BRAWL_DECK_SIZE {
                GameFormat::Brawl
            } else if has("standard") || deck_size > 0 {
                GameFormat::StandardBrawl
            } else {
                // Arena's regular Brawl queue is Historic Brawl.
                GameFormat::Brawl
            };
        }
        if ["draft", "sealed", "cube"]
            .iter()
            .any(|w| lower.contains(w))
        {
            return GameFormat::Limited;
        }
        if event::fixed_deck(event_name) {
            return GameFormat::Other;
        }
        let named = [
            ("historic", GameFormat::Historic),
            ("explorer", GameFormat::Explorer),
            ("alchemy", GameFormat::Alchemy),
            ("timeless", GameFormat::Timeless),
            ("standard", GameFormat::Standard),
        ];
        if let Some((_, f)) = named.iter().find(|(w, _)| has(w)) {
            return *f;
        }
        // Ranked and play queues without a format name are Standard.
        match event_name {
            event::LADDER | event::LADDER_BO3 | event::PLAY | event::PLAY_BO3 => {
                GameFormat::Standard
            }
            _ => GameFormat::Other,
        }
    }

    /// The constructed format with tournament decks and legality, if any.
    pub fn meta(self) -> Option<Format> {
        match self {
            GameFormat::Standard => Some(Format::Standard),
            GameFormat::Historic => Some(Format::Historic),
            GameFormat::Explorer => Some(Format::Explorer),
            GameFormat::Alchemy => Some(Format::Alchemy),
            _ => None,
        }
    }

    /// Name of the format at Scryfall (for the legality list); none for
    /// limited and other events.
    pub fn scryfall(self) -> Option<&'static str> {
        match self {
            GameFormat::Standard => Some("standard"),
            GameFormat::Historic => Some("historic"),
            GameFormat::Explorer => Some("explorer"),
            GameFormat::Alchemy => Some("alchemy"),
            GameFormat::Timeless => Some("timeless"),
            GameFormat::Brawl => Some("brawl"),
            GameFormat::StandardBrawl => Some("standardbrawl"),
            GameFormat::Limited | GameFormat::Other => None,
        }
    }

    /// Deck size of the format, commander included (constructed: minimum).
    pub fn deck_size(self) -> Option<usize> {
        match self {
            GameFormat::Brawl => Some(BRAWL_DECK_SIZE),
            GameFormat::Limited | GameFormat::Other => None,
            _ => Some(60),
        }
    }

    /// Brawl in either size.
    pub fn is_brawl(self) -> bool {
        matches!(self, GameFormat::Brawl | GameFormat::StandardBrawl)
    }
}

impl GameRecord {
    /// Format the game was played in.
    pub fn format(&self) -> GameFormat {
        GameFormat::of(&self.event, self.deck.len() + self.commander.len())
    }
}
