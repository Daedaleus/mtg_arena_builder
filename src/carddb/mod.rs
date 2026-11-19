//! Kartendaten aus der lokalen MTG-Arena-Datenbank (SQLite).
//!
//! Aufbau: hier Kartentyp, Datenbank und Nachschlagen; `text` wertet
//! Fähigkeitstexte aus, `load` liest die SQLite-Datei von Arena. Fachbegriffe
//! für alle Module: [`color`] (Farben, Standardländer) und [`keyword`]
//! (Schlüsselwort-Fähigkeiten).

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

/// Seltenheit wie in der Arena-Datenbank: 0 = Token, 1 = Standardland,
/// 2 = Common, 3 = Uncommon, 4 = Rare, 5 = Mythic.
pub const RARITY_COMMON: u8 = 2;

#[derive(Clone, Debug, Default)]
pub struct Card {
    pub grp_id: u32,
    pub name: String,
    pub name_de: Option<String>,
    pub rarity: u8,
    pub set: String,
    pub collector: String,
    pub types: Vec<u8>,
    pub subtypes: Vec<u32>,
    pub basic: bool,
    /// Farbidentität als Bitmaske (Bit 0 = Weiß … Bit 4 = Grün).
    pub colors: u8,
    pub mana_cost: String,
    pub cmc: u32,
    pub abilities: Vec<u32>,
    pub power: String,
    pub toughness: String,
    pub is_primary: bool,
    /// Druck aus einem regulären Set (keine Sonder-/Bonusveröffentlichung).
    pub regular: bool,
    pub back_face: bool,
    /// Grobe Rolle laut Kartentext (siehe `ROLE_*`).
    pub roles: u8,
    /// Direkter Schaden an Spielern (nur Spontanzauber/Hexereien), z. B. 2 bei Burst Lightning.
    pub burn: u32,
    /// Land kommt immer getappt ins Spiel („enters tapped“ ohne „unless“).
    pub enters_tapped: bool,
    /// Nichtland mit Manafähigkeit („{T}: Add …“): erzeugte Farben, 0 = farblos.
    pub mana: Option<u8>,
}

/// Entfernt oder neutralisiert gegnerische Karten (Zerstören, Schaden, Verbannen …).
pub const ROLE_REMOVAL: u8 = 1;
/// Neutralisiert Zaubersprüche.
pub const ROLE_COUNTER: u8 = 2;
/// Zieht Karten.
pub const ROLE_DRAW: u8 = 4;
/// Räumt das ganze Spielfeld ab – trifft auch die eigenen Kreaturen.
pub const ROLE_WIPE: u8 = 8;

pub const TYPE_INSTANT: u8 = 4;
pub const TYPE_CREATURE: u8 = 2;
pub const TYPE_SORCERY: u8 = 10;

impl Card {
    pub fn has_role(&self, role: u8) -> bool {
        self.roles & role != 0
    }

    /// Name in der eingestellten Sprache: der deutsche Name aus Arena, sonst
    /// der englische. Zum Nachschlagen und für den Arena-Import gilt immer
    /// [`Card::name`].
    pub fn display_name(&self) -> &str {
        match crate::i18n::current() {
            crate::i18n::Locale::German => self.name_de.as_deref().unwrap_or(&self.name),
            crate::i18n::Locale::English => &self.name,
        }
    }

    /// Hat die Karte die Fähigkeit (siehe [`keyword`])?
    pub fn has_ability(&self, id: u32) -> bool {
        self.abilities.contains(&id)
    }

    pub fn is_creature(&self) -> bool {
        self.types.contains(&TYPE_CREATURE)
    }

    pub fn is_instant(&self) -> bool {
        self.types.contains(&TYPE_INSTANT)
    }

    /// Manakosten aufgeteilt in farbloses Mana und farbige Symbole je Farbe
    /// (W, U, B, R, G). Hybrid- und Phyrexia-Symbole zählen als farblos.
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
    pub cards: HashMap<u32, Card>,
    by_name: HashMap<String, Vec<u32>>,
    /// Vorderseite von „A // B“-Karten -> vollständiger Schlüssel.
    front_names: HashMap<String, String>,
    pub version: String,
}

/// Normalisiert Kartennamen für den Vergleich zwischen Quellen.
pub fn name_key(name: &str) -> String {
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
    /// Baut die Datenbank aus fertigen Karten (z. B. für Tests).
    pub fn from_cards(list: Vec<Card>, version: impl Into<String>) -> CardDb {
        let mut cards = HashMap::new();
        let mut by_name: HashMap<String, Vec<u32>> = HashMap::new();
        for card in list {
            by_name
                .entry(name_key(&card.name))
                .or_default()
                .push(card.grp_id);
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

    /// Löst einen Kartennamen (auch nur die Vorderseite) in den DB-Schlüssel auf.
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

    /// Alle Drucke (grpIds) zu einem aufgelösten Namensschlüssel.
    pub fn printings(&self, key: &str) -> &[u32] {
        self.by_name.get(key).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Repräsentativer Druck: bevorzugt aus einem regulären Set, möglichst neu.
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

    /// Anzeigename zu einem (englischen) Kartennamen; unbekannte Namen bleiben.
    pub fn display_name(&self, name: &str) -> String {
        self.resolve(name)
            .and_then(|k| self.representative(&k))
            .map_or_else(|| name.to_string(), |c| c.display_name().to_string())
    }

    /// Günstigste Seltenheit über alle Drucke – so viel kostet die Wildcard.
    pub fn craft_rarity(&self, key: &str) -> u8 {
        self.printings(key)
            .iter()
            .filter_map(|g| self.cards.get(g))
            .map(|c| c.rarity)
            .min()
            .unwrap_or(0)
    }

    pub fn names(&self) -> impl Iterator<Item = &String> {
        self.by_name.keys()
    }
}

pub fn rarity_label(r: u8) -> String {
    match r {
        0 | 1 => tr!("carddb.land"),
        2 => tr!("carddb.common"),
        3 => tr!("carddb.uncommon"),
        4 => tr!("carddb.rare"),
        5 => tr!("carddb.mythic"),
        _ => "?".into(),
    }
}

#[cfg(test)]
#[path = "../tests/carddb.rs"]
mod tests;
