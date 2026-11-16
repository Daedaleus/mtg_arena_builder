//! Decklisten einlesen und gegen die Sammlung prüfen.

use std::collections::HashMap;

use crate::carddb::{CardDb, RARITY_COMMON};

#[derive(Clone, Debug)]
pub struct DeckEntry {
    pub qty: u32,
    pub name: String,
}

#[derive(Clone, Debug, Default)]
pub struct Deck {
    pub main: Vec<DeckEntry>,
    pub side: Vec<DeckEntry>,
}

/// Zerlegt eine Zeile wie `4 Watery Grave` oder `4 Watery Grave (EOE) 261`.
fn parse_entry(line: &str) -> Option<DeckEntry> {
    let (qty, rest) = line.split_once(char::is_whitespace)?;
    let qty: u32 = qty.trim_end_matches(['x', 'X']).parse().ok()?;
    let mut name = rest.trim();
    // Arena-Suffix „(SET) Nummer“ entfernen
    if let Some(open) = name.rfind(" (") {
        let tail = &name[open + 2..];
        if let Some(close) = tail.find(')') {
            let set = &tail[..close];
            if !set.is_empty() && set.len() <= 6 && set.chars().all(|c| c.is_ascii_alphanumeric()) {
                name = name[..open].trim();
            }
        }
    }
    (qty > 0 && !name.is_empty()).then(|| DeckEntry {
        qty,
        name: name.to_string(),
    })
}

/// Liest Listen im Arena- und MTGO-Textformat.
pub fn parse(text: &str) -> Deck {
    let mut deck = Deck::default();
    let mut in_side = false;
    let mut saw_blank_after_main = false;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            saw_blank_after_main = !deck.main.is_empty();
            continue;
        }
        match line.to_lowercase().trim_end_matches(':') {
            "deck" | "commander" | "mainboard" | "main" => {
                in_side = false;
                saw_blank_after_main = false;
                continue;
            }
            "sideboard" | "companion" | "side" => {
                in_side = true;
                continue;
            }
            _ => {}
        }
        let Some(entry) = parse_entry(line) else {
            continue;
        };
        // MTGO-Export ohne Überschrift: Leerzeile trennt das Sideboard ab.
        if saw_blank_after_main {
            in_side = true;
            saw_blank_after_main = false;
        }
        if in_side {
            deck.side.push(entry)
        } else {
            deck.main.push(entry)
        }
    }
    deck
}

#[derive(Clone, Debug)]
pub struct LineEval {
    pub qty: u32,
    pub name: String,
    /// Schlüssel in der Kartendatenbank; `None` = Karte gibt es in Arena nicht.
    pub key: Option<String>,
    pub side: bool,
    pub rarity: u8,
    /// Besitz insgesamt (alle Drucke).
    pub owned: u32,
    pub missing: u32,
    pub basic: bool,
}

#[derive(Clone, Debug, Default)]
pub struct DeckEval {
    pub lines: Vec<LineEval>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Summary {
    pub total: u32,
    pub missing: u32,
    /// Fehlende Karten je Seltenheit (Common, Uncommon, Rare, Mythic).
    pub need: [u32; 4],
    /// Karten, die es in Arena gar nicht gibt.
    pub unavailable: u32,
}

impl Summary {
    pub fn percent(&self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            100.0 * (self.total - self.missing) as f32 / self.total as f32
        }
    }

    /// Reichen die vorhandenen Wildcards, um alles Fehlende zu craften?
    pub fn craftable(&self, wildcards: [u32; 4]) -> bool {
        self.unavailable == 0 && (0..4).all(|i| self.need[i] <= wildcards[i])
    }

    pub fn complete(&self) -> bool {
        self.missing == 0
    }
}

pub fn evaluate(deck: &Deck, db: &CardDb, owned: &HashMap<String, u32>) -> DeckEval {
    let mut remaining: HashMap<String, u32> = HashMap::new();
    let mut lines = Vec::new();
    let entries = deck
        .main
        .iter()
        .map(|e| (e, false))
        .chain(deck.side.iter().map(|e| (e, true)));
    for (entry, side) in entries {
        let key = db.resolve(&entry.name);
        let mut line = LineEval {
            qty: entry.qty,
            name: entry.name.clone(),
            key: key.clone(),
            side,
            rarity: 0,
            owned: 0,
            missing: entry.qty,
            basic: false,
        };
        if let Some(key) = key {
            let rep = db.representative(&key);
            line.name = rep.map(|c| c.name.clone()).unwrap_or(line.name);
            line.basic = rep.is_some_and(|c| c.basic);
            line.rarity = db.craft_rarity(&key);
            line.owned = owned.get(&key).copied().unwrap_or(0);
            if line.basic {
                // Standardländer stellt Arena unbegrenzt zur Verfügung.
                line.missing = 0;
            } else {
                let left = remaining.entry(key).or_insert(line.owned);
                let used = (*left).min(entry.qty);
                *left -= used;
                line.missing = entry.qty - used;
            }
        }
        lines.push(line);
    }
    DeckEval { lines }
}

impl DeckEval {
    /// Zusammenfassung. `replaced` liefert je Zeile, wie viele fehlende Exemplare
    /// durch eine Ersatzkarte abgedeckt sind.
    pub fn summary(&self, include_side: bool, replaced: impl Fn(&LineEval) -> u32) -> Summary {
        let mut s = Summary::default();
        for line in self.lines.iter().filter(|l| include_side || !l.side) {
            s.total += line.qty;
            let missing = line.missing.saturating_sub(replaced(line));
            s.missing += missing;
            if missing == 0 {
                continue;
            }
            if line.key.is_none() {
                s.unavailable += missing;
            } else if line.rarity >= RARITY_COMMON {
                s.need[(line.rarity - RARITY_COMMON).min(3) as usize] += missing;
            }
        }
        s
    }
}

/// Erzeugt Text für „Importieren“ in Arena. `replacement` liefert optional den
/// Ersatz (Name, Anzahl) für fehlende Exemplare einer Zeile.
pub fn export_arena(
    eval: &DeckEval,
    db: &CardDb,
    collection: Option<&HashMap<u32, u32>>,
    with_sets: bool,
    include_side: bool,
    replacement: impl Fn(&LineEval) -> Option<(String, u32)>,
) -> String {
    let fmt = |key: Option<&str>, name: &str, qty: u32| -> String {
        let card = key.and_then(|k| {
            // Bevorzugt einen Druck, den man besitzt.
            let owned_print = db
                .printings(k)
                .iter()
                .filter(|g| collection.is_some_and(|c| c.get(g).copied().unwrap_or(0) > 0))
                .filter_map(|g| db.cards.get(g))
                .next_back();
            owned_print.or_else(|| db.representative(k))
        });
        match card {
            Some(c) if with_sets && !c.set.is_empty() && !c.collector.is_empty() => {
                format!("{qty} {} ({}) {}", c.name, c.set, c.collector)
            }
            Some(c) => format!("{qty} {}", c.name),
            None => format!("{qty} {name}"),
        }
    };
    let mut out = String::from("Deck\n");
    let mut side_started = false;
    for line in &eval.lines {
        if line.side {
            if !include_side {
                break;
            }
            if !side_started {
                out.push_str("\nSideboard\n");
                side_started = true;
            }
        }
        let (rep_qty, rep_name) = match replacement(line) {
            Some((name, n)) => (n.min(line.missing), Some(name)),
            None => (0, None),
        };
        let keep = line.qty - rep_qty;
        if keep > 0 {
            out.push_str(&fmt(line.key.as_deref(), &line.name, keep));
            out.push('\n');
        }
        if let Some(name) = rep_name.filter(|_| rep_qty > 0) {
            out.push_str(&fmt(db.resolve(&name).as_deref(), &name, rep_qty));
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
#[path = "tests/deck.rs"]
mod tests;
