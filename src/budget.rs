//! Budget-Version eines Decks: Was mit den vorhandenen Wildcards hergestellt
//! wird und was durch ähnliche Karten aus der eigenen Sammlung ersetzt wird.
//!
//! Vorgehen: Fehlende Karten werden nach Wichtigkeit sortiert (Vierer-Sets von
//! Nichtland-Karten zuerst, dann Länder, dann Einzelkarten). Solange die
//! Wildcards der jeweiligen Seltenheit reichen, wird hergestellt. Der Rest wird
//! durch die ähnlichste eigene Karte ersetzt; fehlende Länder notfalls durch
//! Standardländer.

use std::collections::{HashMap, HashSet};

use crate::carddb::{CardDb, Rarity, color};
use crate::deck::DeckEval;
use crate::similar;

#[derive(Clone, Debug, Default)]
pub struct BudgetPlan {
    /// (Kartenschlüssel, Ersatz-Schlüssel)
    pub replacements: Vec<(String, String)>,
    /// Mit Wildcards herzustellen (Common, Uncommon, Rare, Mythic).
    pub crafted: [u32; 4],
    /// Durch eigene Karten ersetzte Exemplare.
    pub replaced: u32,
    /// Karten ohne passenden Ersatz, für die auch die Wildcards nicht reichen.
    pub unresolved: Vec<String>,
}

struct Need {
    key: String,
    name: String,
    qty: u32,
    missing: u32,
    rarity: Rarity,
    land: bool,
    cmc: u32,
}

impl Need {
    fn priority(&self) -> u32 {
        match (self.land, self.qty) {
            (false, q) if q >= 3 => 3,
            (true, _) => 2,
            (false, 2) => 2,
            _ => 1,
        }
    }
}

/// Fehlende Karten (Haupt- und ggf. Nebendeck zusammengefasst), wichtigste zuerst.
fn needs(eval: &DeckEval, db: &CardDb, include_side: bool) -> Vec<Need> {
    let mut needs: Vec<Need> = Vec::new();
    for l in eval
        .lines
        .iter()
        .filter(|l| (include_side || !l.side) && l.missing > 0)
    {
        let Some(key) = &l.key else { continue };
        let Some(card) = db.representative(key) else {
            continue;
        };
        match needs.iter_mut().find(|n| &n.key == key) {
            Some(n) => {
                n.qty += l.qty;
                n.missing += l.missing;
            }
            None => needs.push(Need {
                key: key.clone(),
                name: card.name.clone(),
                qty: l.qty,
                missing: l.missing,
                rarity: l.rarity,
                land: card.is_land(),
                cmc: card.cmc,
            }),
        }
    }
    needs.sort_by(|a, b| {
        b.priority()
            .cmp(&a.priority())
            .then(b.qty.cmp(&a.qty))
            .then(a.cmc.cmp(&b.cmc))
            .then_with(|| a.name.cmp(&b.name))
    });
    needs
}

impl Need {
    /// Wildcard-Fach (0 = Common … 3 = Mythic).
    fn slot(&self) -> usize {
        self.rarity.wildcard_slot()
    }
}

/// Plant die Budget-Version. `wildcards` = vorhandene Wildcards (C, U, R, M).
pub fn plan(
    eval: &DeckEval,
    db: &CardDb,
    owned: &HashMap<String, u32>,
    legal: Option<&HashSet<String>>,
    wildcards: [u32; 4],
    include_side: bool,
) -> BudgetPlan {
    let lines: Vec<_> = eval
        .lines
        .iter()
        .filter(|l| include_side || !l.side)
        .collect();
    let needs = needs(eval, db, include_side);

    // 1. Herstellen, solange die Wildcards reichen
    let mut plan = BudgetPlan::default();
    let mut budget = wildcards;
    let mut to_replace: Vec<&Need> = Vec::new();
    for n in &needs {
        let slot = n.slot();
        if n.missing <= budget[slot] {
            budget[slot] -= n.missing;
            plan.crafted[slot] += n.missing;
        } else {
            to_replace.push(n);
        }
    }

    // 2. Den Rest durch eigene Karten ersetzen
    let in_deck: HashSet<String> = lines.iter().filter_map(|l| l.key.clone()).collect();
    let colors = in_deck
        .iter()
        .filter_map(|k| db.representative(k))
        .filter(|c| !c.is_land())
        .fold(0u8, |m, c| m | c.colors);
    // Von eigenen Karten schon verplante Exemplare (inkl. derer, die das Deck selbst nutzt)
    let mut used: HashMap<String, u32> = HashMap::new();
    for l in &lines {
        if let Some(key) = &l.key {
            *used.entry(key.clone()).or_insert(0) += l.qty - l.missing;
        }
    }
    let available = |key: &str, used: &HashMap<String, u32>| {
        owned
            .get(key)
            .copied()
            .unwrap_or(0)
            .saturating_sub(used.get(key).copied().unwrap_or(0))
    };
    // Häufigste Farbe in den Manakosten – für Standardland-Ersatz
    let costs: String = in_deck
        .iter()
        .filter_map(|k| db.representative(k))
        .map(|c| c.mana_cost.clone())
        .collect();
    let basic_key = color::LETTERS
        .iter()
        .zip(color::BASIC_LANDS)
        .max_by_key(|(c, _)| costs.matches(**c).count())
        .and_then(|(_, name)| db.resolve(name));

    for n in to_replace {
        let candidates = similar::suggest(db, owned, legal, colors, &in_deck, &n.key, 10);
        let pick = candidates
            .iter()
            .find(|c| available(&c.key, &used) >= n.missing)
            .or_else(|| {
                candidates
                    .iter()
                    .filter(|c| available(&c.key, &used) > 0)
                    .max_by_key(|c| available(&c.key, &used))
            })
            .map(|c| c.key.clone());
        match pick {
            Some(rep) => {
                let take = available(&rep, &used).min(n.missing);
                *used.entry(rep.clone()).or_insert(0) += take;
                plan.replaced += take;
                if take < n.missing {
                    plan.unresolved.push(n.name.clone());
                }
                plan.replacements.push((n.key.clone(), rep));
            }
            None if n.land && basic_key.is_some() => {
                plan.replaced += n.missing;
                plan.replacements
                    .push((n.key.clone(), basic_key.clone().unwrap()));
            }
            None => plan.unresolved.push(n.name.clone()),
        }
    }
    plan
}

/// Wie viele fehlende Exemplare deckt die Ersatzkarte `rep` ab?
/// Standardländer gibt es in Arena unbegrenzt.
pub fn replaced_qty(db: &CardDb, owned: &HashMap<String, u32>, rep: &str, missing: u32) -> u32 {
    if db.representative(rep).is_some_and(|c| c.basic) {
        missing
    } else {
        owned.get(rep).copied().unwrap_or(0).min(missing)
    }
}

/// Zusammenfassung einer Budget-Version für die Statuszeile.
pub fn plan_text(plan: &BudgetPlan) -> String {
    let c = plan.crafted;
    let mut text = tr!(
        "budget.budget_version_cards_replaced",
        replaced = plan.replaced,
        v = c[0],
        v2 = c[1],
        v3 = c[2],
        v4 = c[3]
    );
    if !plan.unresolved.is_empty() {
        text.push_str(&tr!(
            "budget.no_suitable_replacement",
            v = plan.unresolved.join(", ")
        ));
    }
    text
}

#[cfg(test)]
#[path = "tests/budget.rs"]
mod tests;

/// Ein Schritt des Wildcard-Plans.
#[derive(Clone, Debug, PartialEq)]
pub struct CraftStep {
    pub name: String,
    pub qty: u32,
    /// Wildcard-Fach (0 = Common … 3 = Mythic).
    pub slot: usize,
    /// Nach diesem Schritt insgesamt noch fehlende Wildcards (C, U, R, M).
    pub short: [u32; 4],
    /// Anteil des Decks in Prozent, der danach vorhanden ist.
    pub complete: f32,
}

/// Weg zum fertigen Deck: was jetzt herstellbar ist und in welcher
/// Reihenfolge der Rest folgen sollte.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WildcardPlan {
    pub now: Vec<CraftStep>,
    pub later: Vec<CraftStep>,
    /// Karten, die es in Arena nicht gibt.
    pub unavailable: Vec<String>,
    /// Anteil des Decks in Prozent, der schon vorhanden ist.
    pub complete: f32,
}

/// Wildcard-Plan für ein Deck: Wichtigste Karten zuerst (wie die
/// Budget-Version). Was mit den vorhandenen Wildcards geht, kommt in `now`,
/// der Rest in der empfohlenen Reihenfolge in `later`.
pub fn wildcard_plan(
    eval: &DeckEval,
    db: &CardDb,
    wildcards: [u32; 4],
    include_side: bool,
) -> WildcardPlan {
    let lines: Vec<_> = eval
        .lines
        .iter()
        .filter(|l| include_side || !l.side)
        .collect();
    let total: u32 = lines.iter().map(|l| l.qty).sum::<u32>().max(1);
    let mut have: u32 = lines.iter().map(|l| l.qty - l.missing).sum();
    let mut plan = WildcardPlan {
        complete: 100.0 * have as f32 / total as f32,
        unavailable: lines
            .iter()
            .filter(|l| l.key.is_none() && l.missing > 0)
            .map(|l| l.name.clone())
            .collect(),
        ..Default::default()
    };
    let mut budget = wildcards;
    let mut short = [0u32; 4];
    let mut later: Vec<&Need> = Vec::new();
    let needs = needs(eval, db, include_side);
    for n in &needs {
        let slot = n.slot();
        if n.missing <= budget[slot] {
            budget[slot] -= n.missing;
            have += n.missing;
            plan.now.push(CraftStep {
                name: n.name.clone(),
                qty: n.missing,
                slot,
                short,
                complete: 100.0 * have as f32 / total as f32,
            });
        } else {
            later.push(n);
        }
    }
    for n in later {
        let slot = n.slot();
        // Reste der jetzigen Wildcards zuerst verbrauchen.
        let from_stock = budget[slot].min(n.missing);
        budget[slot] -= from_stock;
        short[slot] += n.missing - from_stock;
        have += n.missing;
        plan.later.push(CraftStep {
            name: n.name.clone(),
            qty: n.missing,
            slot,
            short,
            complete: 100.0 * have as f32 / total as f32,
        });
    }
    plan
}
