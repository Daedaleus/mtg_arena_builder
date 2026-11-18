//! Kampfhilfen für das Begleitfenster: Blockvorschlag im Zug des Gegners und
//! Warnung vor typischen Tricks des gegnerischen Decks.

use std::collections::HashSet;

use crate::carddb::{Card, CardDb, keyword};
use crate::games::LiveCreature;
use crate::opponents::ArchShare;

/// Schlüsselwort-Fähigkeit „Aufblitzen“ in der Arena-Datenbank.
/// Ab diesem Anteil der Turnierlisten gilt eine Karte als „oft gespielt“.
const MIN_SHARE: f32 = 0.25;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BlockKind {
    /// Blocker überlebt und tötet den Angreifer.
    Win,
    /// Blocker überlebt, Angreifer auch.
    Safe,
    /// Beide sterben, der Blocker ist nicht wertvoller.
    Trade,
    /// Blocker stirbt allein – nur, weil der Schaden sonst tödlich wäre.
    Chump,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    /// Ein Blocker, bei Bedrohlichkeit zwei.
    pub blockers: Vec<u32>,
    pub attacker: u32,
    pub kind: BlockKind,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BlockPlan {
    pub blocks: Vec<Block>,
    /// Schaden, der trotz dieser Blocks durchkommt.
    pub damage: i32,
    /// Ohne Blocks käme so viel Schaden durch.
    pub damage_unblocked: i32,
    /// Auch mit allen sinnvollen Blocks tödlich.
    pub lethal: bool,
}

fn kills(src: &LiveCreature, dst: &LiveCreature) -> bool {
    src.power > 0 && !dst.indestructible && (src.power >= dst.toughness || src.deathtouch)
}

/// (Blocker stirbt, Angreifer stirbt) – mit Erstschlag.
fn outcome(blocker: &LiveCreature, attacker: &LiveCreature) -> (bool, bool) {
    let a = kills(attacker, blocker);
    let b = kills(blocker, attacker);
    if attacker.first_strike && !blocker.first_strike && a {
        (true, false)
    } else if blocker.first_strike && !attacker.first_strike && b {
        (false, true)
    } else {
        (a, b)
    }
}

/// Ergebnis eines Kampfes: (stirbt der Angreifer, stirbt welcher Blocker).
/// Der Angreifer verteilt seinen Schaden der Reihe nach auf die Blocker;
/// Erstschlag tötet Blocker, bevor sie zurückschlagen.
pub fn fight(attacker: &LiveCreature, blockers: &[&LiveCreature]) -> (bool, Vec<bool>) {
    if let [b] = blockers {
        let (b_dies, a_dies) = outcome(b, attacker);
        return (a_dies, vec![b_dies]);
    }
    let mut left = attacker.power.max(0);
    let dies: Vec<bool> = blockers
        .iter()
        .map(|b| {
            if b.indestructible || left == 0 {
                false
            } else if attacker.deathtouch {
                left -= 1;
                true
            } else if left >= b.toughness {
                left -= b.toughness;
                true
            } else {
                false
            }
        })
        .collect();
    let hits: i32 = blockers
        .iter()
        .zip(&dies)
        .filter(|(b, d)| !(**d && attacker.first_strike && !b.first_strike))
        .map(|(b, _)| b.power.max(0))
        .sum();
    let deathtouch = blockers.iter().any(|b| b.deathtouch && b.power > 0);
    let a_dies = !attacker.indestructible && hits > 0 && (deathtouch || hits >= attacker.toughness);
    (a_dies, dies)
}

fn can_block(blocker: &LiveCreature, attacker: &LiveCreature) -> bool {
    !attacker.flying || blocker.reach
}

fn worth(c: &LiveCreature) -> i32 {
    c.power + c.toughness + if c.deathtouch || c.first_strike { 2 } else { 0 }
}

/// Schaden, den ein Angreifer trotz Block (oder ohne) anrichtet.
fn through(attacker: &LiveCreature, blockers: &[&LiveCreature]) -> i32 {
    if blockers.is_empty() {
        attacker.power
    } else if attacker.trample {
        let toughness: i32 = blockers.iter().map(|b| b.toughness.max(0)).sum();
        (attacker.power - toughness).max(0)
    } else {
        0
    }
}

/// Berechnet einen Blockvorschlag. Angreifer mit Bedrohlichkeit brauchen zwei
/// Blocker: Sie werden nur aufgehalten, wenn zwei Blocker sie gemeinsam töten
/// und dabei keiner stirbt, oder als Opfer, wenn der Schaden sonst tödlich wäre.
pub fn plan_blocks(attackers: &[LiveCreature], blockers: &[LiveCreature], life: i32) -> BlockPlan {
    let mut atk: Vec<usize> = (0..attackers.len()).collect();
    atk.sort_by_key(|i| std::cmp::Reverse(attackers[*i].power));
    let mut free: Vec<usize> = (0..blockers.len()).collect();
    // Bevorzugt die „billigsten“ Blocker einsetzen.
    free.sort_by_key(|i| worth(&blockers[*i]));
    let mut assigned: Vec<Option<(Vec<usize>, BlockKind)>> = vec![None; attackers.len()];

    let pass = |wanted: BlockKind,
                free: &mut Vec<usize>,
                assigned: &mut [Option<(Vec<usize>, BlockKind)>]| {
        for &a in &atk {
            let att = &attackers[a];
            if assigned[a].is_some() {
                continue;
            }
            if att.menace {
                if wanted == BlockKind::Win
                    && let Some(pair) = menace_pair(att, blockers, free)
                {
                    free.retain(|b| !pair.contains(b));
                    assigned[a] = Some((pair, wanted));
                }
                continue;
            }
            let pick = free.iter().position(|&b| {
                let bl = &blockers[b];
                if !can_block(bl, att) {
                    return false;
                }
                let (b_dies, a_dies) = outcome(bl, att);
                match wanted {
                    BlockKind::Win => !b_dies && a_dies,
                    BlockKind::Safe => !b_dies && !a_dies && through(att, &[bl]) < att.power,
                    BlockKind::Trade => b_dies && a_dies && worth(bl) <= worth(att),
                    BlockKind::Chump => true,
                }
            });
            if let Some(p) = pick {
                assigned[a] = Some((vec![free.remove(p)], wanted));
            }
        }
    };
    pass(BlockKind::Win, &mut free, &mut assigned);
    pass(BlockKind::Safe, &mut free, &mut assigned);
    pass(BlockKind::Trade, &mut free, &mut assigned);

    let damage = |assigned: &[Option<(Vec<usize>, BlockKind)>]| -> i32 {
        (0..attackers.len())
            .map(|a| {
                let by: Vec<&LiveCreature> = assigned[a]
                    .iter()
                    .flat_map(|(bs, _)| bs.iter().map(|b| &blockers[*b]))
                    .collect();
                through(&attackers[a], &by)
            })
            .sum()
    };
    // Nur wenn es sonst tödlich wäre: Kreaturen opfern, größte Angreifer zuerst.
    if damage(&assigned) >= life {
        for &a in &atk {
            if damage(&assigned) < life {
                break;
            }
            let att = &attackers[a];
            if assigned[a].is_some() {
                continue;
            }
            let able: Vec<usize> = free
                .iter()
                .copied()
                .filter(|b| can_block(&blockers[*b], att))
                .take(if att.menace { 2 } else { 1 })
                .collect();
            if able.len() == if att.menace { 2 } else { 1 } {
                free.retain(|b| !able.contains(b));
                assigned[a] = Some((able, BlockKind::Chump));
            }
        }
    }

    let total = damage(&assigned);
    BlockPlan {
        blocks: atk
            .iter()
            .filter_map(|&a| {
                assigned[a].as_ref().map(|(bs, kind)| Block {
                    blockers: bs.iter().map(|b| blockers[*b].grp_id).collect(),
                    attacker: attackers[a].grp_id,
                    kind: *kind,
                })
            })
            .collect(),
        damage: total,
        damage_unblocked: attackers.iter().map(|a| a.power).sum(),
        lethal: total >= life,
    }
}

/// Zwei Blocker, die einen Angreifer mit Bedrohlichkeit gemeinsam töten, ohne
/// dass einer von ihnen stirbt (die billigsten passenden zuerst).
fn menace_pair(
    att: &LiveCreature,
    blockers: &[LiveCreature],
    free: &[usize],
) -> Option<Vec<usize>> {
    if att.indestructible {
        return None;
    }
    let able: Vec<usize> = free
        .iter()
        .copied()
        .filter(|b| can_block(&blockers[*b], att) && !kills(att, &blockers[*b]))
        .collect();
    for (n, &x) in able.iter().enumerate() {
        for &y in &able[n + 1..] {
            let (bx, by) = (&blockers[x], &blockers[y]);
            if bx.deathtouch || by.deathtouch || bx.power + by.power >= att.toughness {
                return Some(vec![x, y]);
            }
        }
    }
    None
}

fn is_trick(c: &Card) -> bool {
    !c.is_land() && c.cmc > 0 && (c.is_instant() || c.has_ability(keyword::FLASH))
}

/// Spontane Karten, die der Gegner mit seinem offenen Mana spielen könnte:
/// schon gesehene zuerst, dann die im erkannten Archetyp häufigen.
/// Ergebnis: (Name, Anteil der Turnierlisten oder `None` = schon gesehen).
pub fn likely_tricks(
    db: &CardDb,
    arch: Option<&ArchShare>,
    seen: &[u32],
    open_mana: u32,
) -> Vec<(String, Option<f32>)> {
    let mut out: Vec<(String, Option<f32>)> = Vec::new();
    let mut names: HashSet<String> = HashSet::new();
    for c in seen.iter().filter_map(|g| db.cards.get(g)) {
        if is_trick(c) && c.cmc <= open_mana && names.insert(c.name.clone()) {
            out.push((c.name.clone(), None));
        }
    }
    if let Some(arch) = arch {
        let mut common: Vec<(&Card, f32)> = arch
            .share
            .iter()
            .filter(|(_, s)| **s >= MIN_SHARE)
            .filter_map(|(k, s)| db.representative(k).map(|c| (c, *s)))
            .filter(|(c, _)| is_trick(c) && c.cmc <= open_mana)
            .collect();
        common.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.name.cmp(&b.0.name)));
        for (c, s) in common {
            if names.insert(c.name.clone()) {
                out.push((c.name.clone(), Some(s)));
            }
        }
    }
    out
}

#[cfg(test)]
#[path = "tests/combat.rs"]
mod tests;
