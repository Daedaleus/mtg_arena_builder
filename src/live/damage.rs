//! Schadensrechnung für die Siegchancen: Angriff und Brandzauber.

use super::*;

/// Schaden, der beim Angriff mit allen Kreaturen mindestens durchkommt, wenn
/// der Gegner bestmöglich blockt (er blockt die stärksten Angreifer, die er
/// blocken kann). Zaubertricks und Trampelschaden bleiben unberücksichtigt.
pub fn unblocked_damage(attackers: &[LiveCreature], blockers: &[LiveCreature]) -> i32 {
    let mut atk: Vec<&LiveCreature> = attackers.iter().collect();
    atk.sort_by_key(|a| std::cmp::Reverse(a.power));
    let mut ground = blockers.iter().filter(|b| !b.reach).count();
    let mut air = blockers.iter().filter(|b| b.reach).count();
    let mut damage = 0;
    for a in atk {
        let need = if a.menace { 2 } else { 1 };
        let blocked = if a.flying {
            // Flieger können nur Kreaturen mit Fliegend/Reichweite blocken.
            if air >= need {
                air -= need;
                true
            } else {
                false
            }
        } else if ground + air >= need {
            // Erst Bodenblocker einsetzen, Luftabwehr für Flieger aufheben.
            let g = need.min(ground);
            ground -= g;
            air -= need - g;
            true
        } else {
            false
        };
        if !blocked {
            damage += a.power;
        }
    }
    damage
}

/// Bester Brandschaden aus der Hand, den das verfügbare Mana bezahlen kann.
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
            best = (dmg, chosen.iter().map(|c| c.name.clone()).collect());
        }
    }
    best
}
