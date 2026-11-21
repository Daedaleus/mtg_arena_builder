//! Brawl rules for a decklist: commander, deck size, one copy per card,
//! commander colour identity and legality in the format.

use std::collections::{HashMap, HashSet};

use super::Deck;
use crate::carddb::CardDb;
use crate::games::GameFormat;

/// A broken Brawl rule; card names are the English names of the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BrawlProblem {
    /// The list has no `Commander` section.
    NoCommander,
    /// The commander is neither a legendary creature nor a legendary
    /// planeswalker.
    NotACommander(String),
    /// Wrong number of cards (commander included).
    Size { have: u32, want: u32 },
    /// More than one copy (basic lands excepted).
    Duplicates(Vec<String>),
    /// Colours outside the commander's colour identity.
    OutsideColors(Vec<String>),
    /// Not legal in the format.
    NotLegal(Vec<String>),
}

/// Checks a Brawl list; empty when everything fits. `legal` = legal card
/// names of the format (keys), if known. Unknown cards are left out.
pub fn check(
    deck: &Deck,
    db: &CardDb,
    format: GameFormat,
    legal: Option<&HashSet<String>>,
) -> Vec<BrawlProblem> {
    let mut problems = Vec::new();
    let commanders: Vec<_> = deck
        .commander
        .iter()
        .filter_map(|n| db.resolve(n).and_then(|k| db.representative(&k)))
        .collect();
    if deck.commander.is_empty() {
        problems.push(BrawlProblem::NoCommander);
    }
    for c in commanders.iter().filter(|c| !c.can_be_commander()) {
        problems.push(BrawlProblem::NotACommander(c.name.clone()));
    }
    let have: u32 = deck.main.iter().map(|e| e.qty).sum();
    if let Some(want) = format.deck_size()
        && have != want as u32
    {
        problems.push(BrawlProblem::Size {
            have,
            want: want as u32,
        });
    }
    let identity = commanders.iter().fold(0u8, |m, c| m | c.colors);
    let mut copies: HashMap<String, (String, u32)> = HashMap::new();
    let (mut outside, mut not_legal) = (Vec::new(), Vec::new());
    for e in &deck.main {
        let Some(key) = db.resolve(&e.name) else {
            continue;
        };
        let Some(card) = db.representative(&key) else {
            continue;
        };
        if !card.basic {
            copies
                .entry(key.clone())
                .or_insert((card.name.clone(), 0))
                .1 += e.qty;
        }
        if !commanders.is_empty() && card.colors & !identity != 0 {
            outside.push(card.name.clone());
        }
        if legal.is_some_and(|l| !l.contains(&key)) {
            not_legal.push(card.name.clone());
        }
    }
    let mut duplicates: Vec<String> = copies
        .into_values()
        .filter(|(_, n)| *n > 1)
        .map(|(name, _)| name)
        .collect();
    duplicates.sort();
    for (list, problem) in [
        (
            duplicates,
            BrawlProblem::Duplicates as fn(Vec<String>) -> BrawlProblem,
        ),
        (outside, BrawlProblem::OutsideColors),
        (not_legal, BrawlProblem::NotLegal),
    ] {
        if !list.is_empty() {
            problems.push(problem(list));
        }
    }
    problems
}

impl BrawlProblem {
    /// Display text; card names in the selected language.
    pub fn user_text(&self, db: &CardDb) -> String {
        let names = |list: &[String]| {
            list.iter()
                .map(|n| db.display_name(n))
                .collect::<Vec<_>>()
                .join(", ")
        };
        match self {
            BrawlProblem::NoCommander => tr!("brawl.no_commander"),
            BrawlProblem::NotACommander(n) => {
                tr!("brawl.not_a_commander", name = db.display_name(n))
            }
            BrawlProblem::Size { have, want } => tr!("brawl.size", have = have, want = want),
            BrawlProblem::Duplicates(l) => tr!("brawl.duplicates", v = names(l)),
            BrawlProblem::OutsideColors(l) => tr!("brawl.outside_colors", v = names(l)),
            BrawlProblem::NotLegal(l) => tr!("brawl.not_legal", v = names(l)),
        }
    }
}
