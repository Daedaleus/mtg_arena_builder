//! Kartendaten aus der lokalen MTG-Arena-Datenbank (SQLite).

use std::collections::{HashMap, HashSet};
use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags};

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

/// Schaden, den ein Fähigkeitstext einem Spieler zufügen kann (0 = keiner).
/// Erfasst z. B. „deals 3 damage to any target“ oder „… to each opponent“.
fn burn_of(text: &str) -> u32 {
    let t = strip_tags(text).to_lowercase();
    for target in [
        "any target",
        "target player",
        "target opponent",
        "each opponent",
        "target player or planeswalker",
    ] {
        let needle = format!(" damage to {target}");
        if let Some(end) = t.find(&needle) {
            let before = &t[..end];
            if let Some(num) = before
                .rsplit("deals ")
                .next()
                .and_then(|n| n.trim().parse::<u32>().ok())
            {
                return num;
            }
        }
    }
    0
}

/// Leitet aus einem (englischen) Fähigkeitstext grobe Rollen ab.
fn roles_of(text: &str) -> u8 {
    let t = strip_tags(text).to_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|n| t.contains(n));
    let mut roles = 0;
    let exile =
        t.contains("exile target") && !has(&["card from", "exile target card", "you control"]);
    if exile
        || (t.contains("destroy target") && !t.contains("destroy target land"))
        || has(&[
            "destroy all",
            "exile all creatures",
            "damage to any target",
            "damage to target creature",
            "damage to target permanent",
            "damage to up to",
            "damage to each creature",
            "damage divided",
            "target creature gets -",
            "creatures get -",
            "fights target",
            "fights up to",
            "fights another target",
            "damage equal to its power to target",
            "sacrifices a creature",
        ])
    {
        roles |= ROLE_REMOVAL;
    }
    if has(&[
        "destroy all",
        "exile all creatures",
        "damage to each creature",
        "creatures get -",
    ]) {
        roles |= ROLE_WIPE;
    }
    if t.contains("counter target") {
        roles |= ROLE_COUNTER;
    }
    if has(&[
        "draw a card",
        "draw two cards",
        "draw three cards",
        "draws two cards",
        "draw cards",
    ]) {
        roles |= ROLE_DRAW;
    }
    roles
}

impl Card {
    pub fn has_role(&self, role: u8) -> bool {
        self.roles & role != 0
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

fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

fn parse_list<T: std::str::FromStr>(s: &str) -> Vec<T> {
    s.split(',')
        .filter_map(|p| p.split(':').next())
        .filter_map(|p| p.trim().parse().ok())
        .collect()
}

/// Manawert aus Arenas Schreibweise, z. B. `o2oUoB` oder `o1o(R/G)`.
fn cmc_of(mana: &str) -> u32 {
    mana.split('o')
        .filter(|t| !t.is_empty())
        .map(|t| match t.parse::<u32>() {
            Ok(n) => n,
            Err(_) if t.eq_ignore_ascii_case("x") => 0,
            Err(_) => 1,
        })
        .sum()
}

/// Lesbare Manakosten: `o2oUoB` -> `{2}{U}{B}`.
fn pretty_mana(mana: &str) -> String {
    mana.split('o')
        .filter(|t| !t.is_empty())
        .map(|t| format!("{{{}}}", t.trim_matches(|c| c == '(' || c == ')')))
        .collect()
}

/// Leitet Rollen, Brandschaden und „kommt getappt“ aus den Fähigkeiten ab.
fn apply_abilities(
    card: &mut Card,
    roles: &HashMap<u32, u8>,
    burn: &HashMap<u32, u32>,
    tapped: &HashSet<u32>,
) {
    card.roles = card
        .abilities
        .iter()
        .filter_map(|a| roles.get(a))
        .fold(0, |m, r| m | r);
    card.enters_tapped = card.abilities.iter().any(|a| tapped.contains(a));
    // Brandschaden zählt nur bei Spontanzaubern und Hexereien – bei Kreaturen
    // steckt er oft in Fähigkeiten, die nicht sofort wirken.
    if card
        .types
        .iter()
        .any(|t| *t == TYPE_INSTANT || *t == TYPE_SORCERY)
    {
        card.burn = card
            .abilities
            .iter()
            .filter_map(|a| burn.get(a))
            .copied()
            .max()
            .unwrap_or(0);
    }
}

/// Kommt ein Land laut Fähigkeitstext immer getappt ins Spiel?
fn enters_tapped_of(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("enters tapped") && !lower.contains("unless")
}

impl CardDb {
    pub fn load(path: &Path) -> Result<CardDb> {
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .with_context(|| {
            format!(
                "Kartendatenbank {} konnte nicht geöffnet werden",
                path.display()
            )
        })?;

        let has_de: bool = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name = 'Localizations_deDE'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n > 0)
            .unwrap_or(false);
        let version: String = conn
            .query_row(
                "SELECT Version FROM Versions WHERE Type = 'Data'",
                [],
                |r| r.get(0),
            )
            .unwrap_or_default();

        let (de_col, de_join) = if has_de {
            (
                "de.Loc",
                "LEFT JOIN Localizations_deDE de ON de.LocId = c.TitleId AND de.Formatted = 1",
            )
        } else {
            ("NULL", "")
        };
        let sql = format!(
            "SELECT c.GrpId, en.Loc, {de_col}, c.Rarity, c.ExpansionCode, c.CollectorNumber,
                    c.IsPrimaryCard, c.OldSchoolManaText, c.Types, c.Subtypes, c.Supertypes,
                    c.ColorIdentity, c.LinkedFaceType, c.AbilityIds, c.Power, c.Toughness,
                    c.DigitalReleaseSet
             FROM Cards c
             JOIN Localizations_enUS en ON en.LocId = c.TitleId AND en.Formatted = 1
             {de_join}
             WHERE c.IsToken = 0 AND c.Rarity > 0"
        );
        let mut stmt = conn.prepare(&sql)?;
        let text = |r: &rusqlite::Row, i: usize| -> String {
            r.get::<_, Option<String>>(i)
                .ok()
                .flatten()
                .unwrap_or_default()
        };
        let rows = stmt.query_map([], |r| {
            let mana = text(r, 7);
            let colors = parse_list::<u8>(&text(r, 11))
                .into_iter()
                .filter(|c| (1..=5).contains(c))
                .fold(0u8, |m, c| m | (1 << (c - 1)));
            Ok(Card {
                grp_id: r.get::<_, i64>(0)? as u32,
                name: strip_tags(&text(r, 1)),
                name_de: r
                    .get::<_, Option<String>>(2)
                    .ok()
                    .flatten()
                    .map(|s| strip_tags(&s))
                    .filter(|s| !s.is_empty() && !s.starts_with('#')),
                rarity: r.get::<_, i64>(3)? as u8,
                set: text(r, 4),
                collector: text(r, 5),
                is_primary: r.get::<_, i64>(6).unwrap_or(0) != 0,
                cmc: cmc_of(&mana),
                mana_cost: pretty_mana(&mana),
                types: parse_list(&text(r, 8)),
                subtypes: parse_list(&text(r, 9)),
                basic: parse_list::<u8>(&text(r, 10)).contains(&1),
                colors,
                back_face: r.get::<_, i64>(12).unwrap_or(0) == 1,
                abilities: parse_list(&text(r, 13)),
                power: text(r, 14),
                toughness: text(r, 15),
                regular: text(r, 16).is_empty(),
                roles: 0,
                burn: 0,
                enters_tapped: false,
            })
        })?;

        // Fähigkeitstexte -> Rollen (Removal, Counter, Kartenziehen)
        let mut ability_roles: HashMap<u32, u8> = HashMap::new();
        let mut ability_burn: HashMap<u32, u32> = HashMap::new();
        let mut ability_tapped: HashSet<u32> = HashSet::new();
        if let Ok(mut stmt) = conn.prepare(
            "SELECT a.Id, l.Loc FROM Abilities a
             JOIN Localizations_enUS l ON l.LocId = a.TextId AND l.Formatted = 1",
        ) {
            let found = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)? as u32,
                    r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                ))
            })?;
            for (id, text) in found.flatten() {
                let roles = roles_of(&text);
                if roles != 0 {
                    ability_roles.insert(id, roles);
                }
                if enters_tapped_of(&text) {
                    ability_tapped.insert(id);
                }
                let burn = burn_of(&text);
                if burn > 0 {
                    ability_burn.insert(id, burn);
                }
            }
        }

        let mut cards = Vec::new();
        for card in rows {
            let mut card = card?;
            if card.name.is_empty() {
                continue;
            }
            apply_abilities(&mut card, &ability_roles, &ability_burn, &ability_tapped);
            cards.push(card);
        }
        Ok(CardDb::from_cards(cards, version))
    }

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

pub fn rarity_label(r: u8) -> &'static str {
    match r {
        0 | 1 => "Land",
        2 => "Common",
        3 => "Uncommon",
        4 => "Rare",
        5 => "Mythic",
        _ => "?",
    }
}

#[cfg(test)]
#[path = "tests/carddb.rs"]
mod tests;
