//! Damage calculation for the chances to win: burn spells from the hand (the
//! attack damage comes from [`crate::combat::unblocked_damage`]).

use super::*;

/// Best burn damage from the hand that the available mana can pay for.
pub(super) fn best_burn(hand: &[&Card], mana: u32) -> (u32, Vec<String>) {
    let burn: Vec<&&Card> = hand
        .iter()
        .filter(|c| c.burn > 0 && c.cmc <= mana)
        .take(10)
        .collect();
    let mut best = (0u32, Vec::new());
    for mask in 1u32..(1 << burn.len()) {
        let chosen: Vec<&&&Card> = burn
            .iter()
            .enumerate()
            .filter(|(i, _)| mask & (1 << i) != 0)
            .map(|(_, c)| c)
            .collect();
        let cost: u32 = chosen.iter().map(|c| c.cmc).sum();
        let dmg: u32 = chosen.iter().map(|c| c.burn).sum();
        if cost <= mana && dmg > best.0 {
            best = (
                dmg,
                chosen
                    .iter()
                    .map(|c| c.display_name().to_string())
                    .collect(),
            );
        }
    }
    best
}
