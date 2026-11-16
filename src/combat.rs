//! Kampfhilfen für das Begleitfenster: Blockvorschlag im Zug des Gegners und
//! Warnung vor typischen Tricks des gegnerischen Decks.

use std::collections::HashSet;

use crate::carddb::{Card, CardDb};
use crate::games::LiveCreature;
use crate::live::ArchShare;

/// Schlüsselwort-Fähigkeit „Aufblitzen“ in der Arena-Datenbank.
const ABILITY_FLASH: u32 = 7;
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
    pub blocker: u32,
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

fn can_block(blocker: &LiveCreature, attacker: &LiveCreature) -> bool {
    !attacker.flying || blocker.reach
}

fn worth(c: &LiveCreature) -> i32 {
    c.power + c.toughness + if c.deathtouch || c.first_strike { 2 } else { 0 }
}

/// Schaden, den ein Angreifer trotz Block (oder ohne) anrichtet.
fn through(attacker: &LiveCreature, blocker: Option<&LiveCreature>) -> i32 {
    match blocker {
        None => attacker.power,
        Some(b) if attacker.trample => (attacker.power - b.toughness.max(0)).max(0),
        Some(_) => 0,
    }
}

/// Berechnet einen Blockvorschlag. Angreifer mit Bedrohlichkeit werden nur
/// im Notfall (mit zwei Blockern) aufgehalten – das übernimmt hier niemand,
/// sie zählen einfach als ungeblockt.
pub fn plan_blocks(attackers: &[LiveCreature], blockers: &[LiveCreature], life: i32) -> BlockPlan {
    let mut atk: Vec<usize> = (0..attackers.len()).collect();
    atk.sort_by_key(|i| std::cmp::Reverse(attackers[*i].power));
    let mut free: Vec<usize> = (0..blockers.len()).collect();
    // Bevorzugt die „billigsten“ Blocker einsetzen.
    free.sort_by_key(|i| worth(&blockers[*i]));
    let mut assigned: Vec<Option<(usize, BlockKind)>> = vec![None; attackers.len()];

    let pass =
        |wanted: BlockKind, free: &mut Vec<usize>, assigned: &mut [Option<(usize, BlockKind)>]| {
            for &a in &atk {
                let att = &attackers[a];
                if assigned[a].is_some() || att.menace {
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
                        BlockKind::Safe => !b_dies && !a_dies && through(att, Some(bl)) < att.power,
                        BlockKind::Trade => b_dies && a_dies && worth(bl) <= worth(att),
                        BlockKind::Chump => true,
                    }
                });
                if let Some(p) = pick {
                    assigned[a] = Some((free.remove(p), wanted));
                }
            }
        };
    pass(BlockKind::Win, &mut free, &mut assigned);
    pass(BlockKind::Safe, &mut free, &mut assigned);
    pass(BlockKind::Trade, &mut free, &mut assigned);

    let damage = |assigned: &[Option<(usize, BlockKind)>]| -> i32 {
        (0..attackers.len())
            .map(|a| through(&attackers[a], assigned[a].map(|(b, _)| &blockers[b])))
            .sum()
    };
    // Nur wenn es sonst tödlich wäre: Kreaturen opfern, größte Angreifer zuerst.
    if damage(&assigned) >= life {
        for &a in &atk {
            if damage(&assigned) < life {
                break;
            }
            let att = &attackers[a];
            if assigned[a].is_some() || att.menace {
                continue;
            }
            if let Some(p) = free.iter().position(|&b| can_block(&blockers[b], att)) {
                assigned[a] = Some((free.remove(p), BlockKind::Chump));
            }
        }
    }

    let total = damage(&assigned);
    BlockPlan {
        blocks: atk
            .iter()
            .filter_map(|&a| {
                assigned[a].map(|(b, kind)| Block {
                    blocker: blockers[b].grp_id,
                    attacker: attackers[a].grp_id,
                    kind,
                })
            })
            .collect(),
        damage: total,
        damage_unblocked: attackers.iter().map(|a| a.power).sum(),
        lethal: total >= life,
    }
}

fn is_trick(c: &Card) -> bool {
    !c.is_land() && c.cmc > 0 && (c.is_instant() || c.abilities.contains(&ABILITY_FLASH))
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
mod tests {
    use super::*;

    fn cr(grp: u32, power: i32, toughness: i32) -> LiveCreature {
        LiveCreature {
            grp_id: grp,
            power,
            toughness,
            ..Default::default()
        }
    }

    #[test]
    fn blocks() {
        // 3/3 blockt 2/2 und gewinnt; 1/1 bleibt zurück.
        let p = plan_blocks(&[cr(1, 2, 2)], &[cr(10, 1, 1), cr(11, 3, 3)], 20);
        assert_eq!(p.blocks.len(), 1);
        assert_eq!(p.blocks[0].blocker, 11);
        assert_eq!(p.blocks[0].kind, BlockKind::Win);
        assert_eq!(p.damage, 0);

        // 2/2 gegen 4/4: kein lohnender Block bei viel Leben …
        let p = plan_blocks(&[cr(1, 4, 4)], &[cr(10, 2, 2)], 20);
        assert!(p.blocks.is_empty());
        assert_eq!(p.damage, 4);
        // … aber Pflicht, wenn der Schaden tödlich wäre.
        let p = plan_blocks(&[cr(1, 4, 4)], &[cr(10, 2, 2)], 4);
        assert_eq!(p.blocks[0].kind, BlockKind::Chump);
        assert!(!p.lethal);

        // Gleich starke Kreaturen tauschen.
        let p = plan_blocks(&[cr(1, 3, 3)], &[cr(10, 3, 3)], 20);
        assert_eq!(p.blocks[0].kind, BlockKind::Trade);

        // Erstschlag: 2/2 mit Erstschlag gewinnt gegen 3/2.
        let mut fs = cr(10, 2, 2);
        fs.first_strike = true;
        let p = plan_blocks(&[cr(1, 3, 2)], &[fs], 20);
        assert_eq!(p.blocks[0].kind, BlockKind::Win);

        // Flieger kann nicht vom Boden geblockt werden.
        let mut flyer = cr(1, 3, 3);
        flyer.flying = true;
        let p = plan_blocks(&[flyer], &[cr(10, 5, 5)], 3);
        assert!(p.blocks.is_empty());
        assert!(p.lethal);
    }
}
