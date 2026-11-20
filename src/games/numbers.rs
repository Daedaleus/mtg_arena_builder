//! Feste Partie-Nummern: chronologisch, die älteste Partie ist #1. Kommt eine
//! neue Partie dazu, behalten alle bisherigen ihre Nummer – auch wenn die
//! Liste neueste zuerst zeigt oder nach Modus gefiltert ist.

use std::collections::HashMap;

use super::GameRecord;

#[derive(Clone, Debug, Default)]
pub struct GameNumbers(HashMap<String, usize>);

impl GameNumbers {
    /// Nummeriert alle gespeicherten Partien (nicht nur eine gefilterte Auswahl).
    pub fn new(all: &[GameRecord]) -> GameNumbers {
        let mut order: Vec<&GameRecord> = all.iter().collect();
        order.sort_by(|a, b| {
            a.started_ms
                .cmp(&b.started_ms)
                .then_with(|| a.id.cmp(&b.id))
        });
        GameNumbers(
            order
                .iter()
                .enumerate()
                .map(|(i, g)| (g.id.clone(), i + 1))
                .collect(),
        )
    }

    /// Nummer der Partie; 0, wenn sie nicht zu den nummerierten gehört.
    pub fn of(&self, g: &GameRecord) -> usize {
        self.0.get(&g.id).copied().unwrap_or(0)
    }
}
