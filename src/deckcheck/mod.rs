//! "Improve deck": analyses the own games per deck and suggests cards to
//! swap.
//!
//! Deliberately careful: win rates of single cards only become meaningful
//! after many games. Until then the suggestions rely on what holds up even
//! after few games – how you actually use a card, whether your mana fits and
//! what tournament players play in the same archetype.
//!
//! Layout: data types here; `groups` (games per deck), `mana` (land check),
//! `analysis` (card usage, comparison, swaps).

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

/// From this many complete games on, the app shows win rates per card.
pub const MIN_GAMES_FOR_WINRATE: usize = 10;

/// All games played with the same decklist.
#[derive(Clone, Debug)]
pub struct DeckGroup {
    pub label: String,
    /// Decklist (grpIds, repeated = copies).
    pub deck: Vec<GrpId>,
    /// Brawl: the commander of the most recent game.
    pub commander: Option<GrpId>,
    /// Indices into the game list.
    pub games: Vec<usize>,
    pub wins: usize,
    /// Fixed deck of an event that can't be changed in Arena.
    pub fixed: bool,
}

/// Usage of a card over all games of a deck.
#[derive(Clone, Debug, Default)]
pub struct CardStat {
    pub key: String,
    pub name: String,
    pub copies: u32,
    pub rarity: Rarity,
    pub cmc: u32,
    pub land: bool,
    /// In how many games drawn or cast.
    pub seen: usize,
    pub cast: usize,
    /// In how many games affordable in hand for at least three own turns.
    pub held: usize,
    pub damage: u32,
    /// Wins in games in which the card was drawn.
    pub won_when_seen: usize,
    /// Share of tournament lists of the matching archetype that play the card.
    pub meta_share: Option<f32>,
}

#[derive(Clone, Debug, Default)]
pub struct ManaCheck {
    pub games: usize,
    /// Games in which lands were missing in the first three own turns.
    pub screw: usize,
    /// Games in which lands later sat uselessly in hand.
    pub flood: usize,
    pub lands: u32,
    pub size: u32,
    pub verdict: LandVerdict,
}

/// Verdict on the land count of a deck.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LandVerdict {
    /// Fewer than [`MIN_GAMES_FOR_LANDS`] complete games.
    #[default]
    TooFewGames,
    /// Often too few lands.
    TooFew,
    /// Often too many lands.
    TooMany,
    Fine,
}

/// At least this many complete games for a verdict on the lands.
pub const MIN_GAMES_FOR_LANDS: usize = 3;

impl LandVerdict {
    /// Should the land count change?
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
    /// Overlap of the non-land cards (0–1).
    pub overlap: f32,
    /// Cards that at least half of the tournament lists play, but you don't:
    /// (name, share, copies in your collection).
    pub missing: Vec<(String, f32, u32)>,
}

#[derive(Clone, Debug)]
pub struct Swap {
    pub out: String,
    pub reason: String,
    /// Replacement from the collection (name, reason).
    pub replacement: Option<(String, String)>,
}

#[derive(Clone, Debug, Default)]
pub struct DeckCheck {
    pub games: usize,
    pub complete: usize,
    /// Games with data on drawn and cast cards.
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

/// Tournament lists of an archetype for the comparison.
pub struct MetaArchetype<'a> {
    pub id: u32,
    pub name: String,
    pub lists: Vec<&'a Deck>,
}

/// Decklist as text for "Check own list" or the Arena import.
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
