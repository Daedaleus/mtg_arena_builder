//! Card data from the local MTG Arena database (SQLite).
//!
//! Layout: card type, database and lookup here; `text` evaluates ability
//! texts, `load` reads Arena's SQLite file. Domain terms for all modules:
//! [`color`] (colors, basic lands) and [`keyword`]
//! (keyword abilities).

use std::collections::{HashMap, HashSet};
use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags};

pub mod color;
pub mod keyword;
mod load;
mod text;

#[cfg(test)]
use load::*;
use text::*;

pub const TYPE_LAND: u8 = 5;

/// Id of a card printing in Arena ("grpId"). Every printing has its own;
/// several often share the same name.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize,
)]
#[serde(transparent)]
pub struct GrpId(pub u32);

impl GrpId {
    /// Id from an Arena value (JSON number in the log, database column).
    pub fn new(id: u32) -> GrpId {
        GrpId(id)
    }

    /// 0 = no card printing (e.g. face-down cards in the log).
    pub fn is_known(self) -> bool {
        self.0 > 0
    }
}

impl std::fmt::Display for GrpId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl From<u32> for GrpId {
    fn from(id: u32) -> GrpId {
        GrpId(id)
    }
}

/// Normalised card name (see [`name_key`]) – groups all printings of a card;
/// key for collection, tournament lists and replacement cards.
/// Deliberately only an alias: the keys come from many text sources
/// (MTGTop8, magic.gg, collection) and are never confused with numbers;
/// they are always created through [`name_key`].
pub type CardKey = String;

/// Rarity of a card (order = value: mythic is the highest).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Rarity {
    #[default]
    Token,
    BasicLand,
    Common,
    Uncommon,
    Rare,
    Mythic,
}

impl Rarity {
    /// The four wildcard tiers; the index is the wildcard slot.
    pub const WILDCARDS: [Rarity; 4] = [
        Rarity::Common,
        Rarity::Uncommon,
        Rarity::Rare,
        Rarity::Mythic,
    ];

    /// Value from the Arena database: 0 = token, 1 = basic land,
    /// 2 = common, 3 = uncommon, 4 = rare, 5 = mythic.
    pub fn from_arena(value: u8) -> Rarity {
        match value {
            1 => Rarity::BasicLand,
            2 => Rarity::Common,
            3 => Rarity::Uncommon,
            4 => Rarity::Rare,
            5 => Rarity::Mythic,
            _ => Rarity::Token,
        }
    }

    /// Does the card cost a wildcard (no tokens and basic lands)?
    pub fn needs_wildcard(self) -> bool {
        self >= Rarity::Common
    }

    /// Wildcard slot: 0 = common … 3 = mythic (tokens and basic lands like common).
    pub fn wildcard_slot(self) -> usize {
        match self {
            Rarity::Uncommon => 1,
            Rarity::Rare => 2,
            Rarity::Mythic => 3,
            _ => 0,
        }
    }

    pub fn label(self) -> String {
        match self {
            Rarity::Token | Rarity::BasicLand => tr!("carddb.land"),
            Rarity::Common => tr!("carddb.common"),
            Rarity::Uncommon => tr!("carddb.uncommon"),
            Rarity::Rare => tr!("carddb.rare"),
            Rarity::Mythic => tr!("carddb.mythic"),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Card {
    pub grp_id: GrpId,
    pub name: String,
    pub name_de: Option<String>,
    pub rarity: Rarity,
    pub set: String,
    pub collector: String,
    pub types: Vec<u8>,
    pub subtypes: Vec<u32>,
    pub basic: bool,
    /// Legendary: each player may only have one card of this name in play.
    pub legendary: bool,
    /// Color identity as a bit mask (bit 0 = white … bit 4 = green).
    pub colors: u8,
    pub mana_cost: String,
    pub cmc: u32,
    pub abilities: Vec<u32>,
    pub power: String,
    pub toughness: String,
    pub is_primary: bool,
    /// Printing from a regular set (no special or bonus release).
    pub regular: bool,
    pub back_face: bool,
    /// Rough role according to the card text (see `ROLE_*`).
    pub roles: u8,
    /// Direct damage to players (instants/sorceries only), e.g. 2 for Burst Lightning.
    pub burn: u32,
    /// Land always enters tapped ("enters tapped" without "unless").
    pub enters_tapped: bool,
    /// Non-land with a mana ability ("{T}: Add …"): colors produced, 0 = colorless.
    pub mana: Option<u8>,
    /// Has an activated ability that costs tapping ("{T}: …", "{T}, …: …").
    pub tap_ability: bool,
    /// Token (e.g. Treasure): only looked up by grpId from the log, never by
    /// name – many tokens share their name with a real card.
    pub token: bool,
}

/// Removes or neutralises opposing cards (destroy, damage, exile …).
pub const ROLE_REMOVAL: u8 = 1;
/// Counters spells.
pub const ROLE_COUNTER: u8 = 2;
/// Draws cards.
pub const ROLE_DRAW: u8 = 4;
/// Combat trick: a creature gets +X/+X until end of turn.
pub const ROLE_PUMP: u8 = 16;
/// Clears the whole battlefield – also hits the own creatures.
pub const ROLE_WIPE: u8 = 8;

pub const TYPE_INSTANT: u8 = 4;
pub const TYPE_CREATURE: u8 = 2;
pub const TYPE_SORCERY: u8 = 10;

impl Card {
    pub fn has_role(&self, role: u8) -> bool {
        self.roles & role != 0
    }

    /// Name in the selected language: the German name from Arena, otherwise
    /// the English one. Lookups and the Arena import always use
    /// [`Card::name`].
    pub fn display_name(&self) -> &str {
        match crate::i18n::current() {
            crate::i18n::Locale::German => self.name_de.as_deref().unwrap_or(&self.name),
            crate::i18n::Locale::English => &self.name,
        }
    }

    /// Does the card have the ability (see [`keyword`])?
    pub fn has_ability(&self, id: u32) -> bool {
        self.abilities.contains(&id)
    }

    pub fn is_creature(&self) -> bool {
        self.types.contains(&TYPE_CREATURE)
    }

    pub fn is_instant(&self) -> bool {
        self.types.contains(&TYPE_INSTANT)
    }

    /// Mana costs split into generic mana and colored symbols per color
    /// (W, U, B, R, G). Hybrid and Phyrexian symbols count as generic.
    pub fn pips(&self) -> (u32, [u32; 5]) {
        let mut generic = 0;
        let mut colored = [0u32; 5];
        for sym in self.mana_cost.split(['{', '}']).filter(|t| !t.is_empty()) {
            match sym {
                "W" => colored[0] += 1,
                "U" => colored[1] += 1,
                "B" => colored[2] += 1,
                "R" => colored[3] += 1,
                "G" => colored[4] += 1,
                "X" => {}
                n => generic += n.parse::<u32>().unwrap_or(1),
            }
        }
        (generic, colored)
    }

    pub fn is_land(&self) -> bool {
        self.types.contains(&TYPE_LAND)
    }
}

pub struct CardDb {
    pub cards: HashMap<GrpId, Card>,
    by_name: HashMap<CardKey, Vec<GrpId>>,
    /// Front face of "A // B" cards -> full key.
    front_names: HashMap<String, String>,
    pub version: String,
}

/// Normalises card names for comparison between sources.
pub fn name_key(name: &str) -> CardKey {
    let mut s = name
        .trim()
        .to_lowercase()
        .replace(['\u{2019}', '\u{2018}', '`'], "'")
        .replace(" // ", "\u{1}")
        .replace(" / ", "\u{1}")
        .replace("//", "\u{1}")
        .replace('\u{1}', " // ");
    while s.contains("  ") {
        s = s.replace("  ", " ");
    }
    s
}

impl CardDb {
    /// Builds the database from finished cards (e.g. for tests).
    pub fn from_cards(list: Vec<Card>, version: impl Into<String>) -> CardDb {
        let mut cards = HashMap::new();
        let mut by_name: HashMap<String, Vec<GrpId>> = HashMap::new();
        for card in list {
            if !card.token {
                by_name
                    .entry(name_key(&card.name))
                    .or_default()
                    .push(card.grp_id);
            }
            cards.insert(card.grp_id, card);
        }
        let mut front_names = HashMap::new();
        for (key, ids) in by_name.iter_mut() {
            ids.sort_unstable();
            if let Some((front, _)) = key.split_once(" // ") {
                front_names.insert(front.to_string(), key.clone());
            }
        }
        CardDb {
            cards,
            by_name,
            front_names,
            version: version.into(),
        }
    }

    /// Number of real cards (without tokens).
    pub fn card_count(&self) -> usize {
        self.cards.values().filter(|c| !c.token).count()
    }

    /// Resolves a card name (also only the front face) into the database key.
    pub fn resolve(&self, name: &str) -> Option<String> {
        let key = name_key(name);
        if self.by_name.contains_key(&key) {
            return Some(key);
        }
        if let Some((front, _)) = key.split_once(" // ")
            && self.by_name.contains_key(front)
        {
            return Some(front.to_string());
        }
        self.front_names.get(&key).cloned()
    }

    /// All printings (grpIds) of a resolved name key.
    pub fn printings(&self, key: &str) -> &[GrpId] {
        self.by_name.get(key).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Representative printing: preferably from a regular set, as new as possible.
    pub fn representative(&self, key: &str) -> Option<&Card> {
        let cards: Vec<&Card> = self
            .printings(key)
            .iter()
            .filter_map(|g| self.cards.get(g))
            .collect();
        cards
            .iter()
            .rev()
            .find(|c| c.is_primary && c.regular && !c.back_face)
            .or_else(|| cards.iter().rev().find(|c| c.is_primary && !c.back_face))
            .or_else(|| cards.iter().rev().find(|c| !c.back_face))
            .or(cards.last())
            .copied()
    }

    /// Display name for an (English) card name; unknown names stay.
    pub fn display_name(&self, name: &str) -> String {
        self.resolve(name)
            .and_then(|k| self.representative(&k))
            .map_or_else(|| name.to_string(), |c| c.display_name().to_string())
    }

    /// Cheapest rarity over all printings – that's what the wildcard costs.
    pub fn craft_rarity(&self, key: &str) -> Rarity {
        self.printings(key)
            .iter()
            .filter_map(|g| self.cards.get(g))
            .map(|c| c.rarity)
            .min()
            .unwrap_or_default()
    }

    pub fn names(&self) -> impl Iterator<Item = &String> {
        self.by_name.keys()
    }
}

#[cfg(test)]
#[path = "../tests/carddb.rs"]
mod tests;
