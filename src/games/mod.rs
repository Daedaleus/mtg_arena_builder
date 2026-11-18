//! Partie-Auswertung aus der `Player.log` von MTG Arena.
//!
//! Arena schreibt mit eingeschalteten „Detaillierten Logs“ jeden Spielzustand
//! als JSON mit. Daraus werden je Partie Zugstatistiken gewonnen und dauerhaft
//! in `games.json` gespeichert – Arena überschreibt das Log bei jedem Start.
//! Aus den Statistiken leitet [`report`] konkrete Verbesserungstipps ab.
//!
//! Aufbau:
//! - Mitschreiben: `object` (Spielobjekt, gefahrloser Angriff), `tracker`
//!   (Spielzustand), `live_state` (Momentaufnahme fürs Begleitfenster),
//!   `parser` (Logdateien)
//! - Speichern: `record` (Datentypen), `store` (`games.json`)
//! - Auswerten: `moments` (einzelne Züge), `mistake` (Fehlerarten),
//!   `report` (alle Partien)

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::carddb::keyword as kw;
use crate::carddb::{Card, CardDb, color};

mod live_state;
mod mistake;
mod moments;
mod object;
mod parser;
mod record;
mod report;
mod store;
mod tracker;

pub use live_state::*;
pub use mistake::*;
pub use moments::*;
use object::*;
pub use parser::*;
pub use record::*;
pub use report::*;
pub use store::*;
use tracker::*;

/// Farben eines Decks als Buchstaben (z. B. „UR“), aus den Manakosten.
pub fn deck_colors(deck: &[u32], db: &CardDb) -> String {
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

/// Gegnerfarben aus den gesehenen Karten.
pub fn opp_colors(g: &GameRecord, db: &CardDb) -> String {
    let ids: Vec<u32> = g.opp_cards.keys().copied().collect();
    deck_colors(&ids, db)
}

#[cfg(test)]
#[path = "../tests/games.rs"]
mod tests;
