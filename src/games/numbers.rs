//! Fixed game numbers: chronological, the oldest game is #1. When a new game
//! arrives, all earlier ones keep their number – even when the list shows
//! the newest first or is filtered by mode.

use std::collections::HashMap;

use super::GameRecord;

#[derive(Clone, Debug, Default)]
pub struct GameNumbers(HashMap<String, usize>);

impl GameNumbers {
    /// Numbers all stored games (not only a filtered selection).
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

    /// Number of the game; 0 if it isn't among the numbered ones.
    pub fn of(&self, g: &GameRecord) -> usize {
        self.0.get(&g.id).copied().unwrap_or(0)
    }
}
