//! Game analysis from MTG Arena's `Player.log`.
//!
//! With "Detailed Logs" turned on, Arena writes every game state as JSON.
//! From it, turn statistics are gained per game and stored permanently in
//! `games.json` – Arena overwrites the log on every start.
//! From the statistics, [`report`] derives concrete tips for improvement.
//!
//! Layout:
//! - Recording: `arena` (protocol values of the log), `object` (game object,
//!   safe attack), `tracker` (game state), `live_state`
//!   (snapshot for the companion), `parser` (log files)
//! - Storage: `record` (data types), `store` (`games.json`)
//! - Choice of games: `filter` (game mode, deck), `format` (format per game)
//! - Analysis: `moments` (single turns), `mistake` (kinds of mistakes),
//!   `report` (all games)

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::carddb::keyword as kw;
use crate::carddb::{Card, CardDb, GrpId, color};
use arena::{Phase, annotation, card_state, message, step, zone};

pub mod arena;
mod filter;
mod format;
mod live_state;
mod mistake;
mod moments;
mod numbers;
mod object;
mod parser;
mod record;
mod report;
mod store;
mod tracker;

pub use filter::{GameMode, GamesFilter, deck_key};
pub use format::{BRAWL_DECK_SIZE, GameFormat};
pub use live_state::*;
pub use mistake::*;
pub use moments::*;
pub use numbers::GameNumbers;
use object::*;
pub use parser::*;
pub use record::*;
pub use report::*;
pub use store::*;
use tracker::*;

/// Colors of a deck as letters (e.g. "UR"), from the mana costs.
pub fn deck_colors(deck: &[GrpId], db: &CardDb) -> String {
    let costs: String = deck
        .iter()
        .filter_map(|g| db.cards.get(g))
        .map(|c| c.mana_cost.as_str())
        .collect();
    color::LETTERS
        .iter()
        .filter(|c| costs.contains(**c))
        .collect()
}

/// Opponent colors from the cards seen.
pub fn opp_colors(g: &GameRecord, db: &CardDb) -> String {
    let ids: Vec<GrpId> = g.opp_cards.keys().copied().collect();
    deck_colors(&ids, db)
}

#[cfg(test)]
#[path = "../tests/games.rs"]
mod tests;
