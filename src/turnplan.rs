//! Turn suggestion for the companion: which land to play and which cards
//! from the hand to cast so the mana is used well.
//!
//! Fixed rules instead of a game AI: mana costs and colors are checked
//! exactly, the importance of a card is only estimated roughly from its role
//! (removal, card draw, creature …). Mana from sources other than lands is
//! not taken into account.

use crate::carddb::{
    Card, CardDb, GrpId, ROLE_COUNTER, ROLE_DRAW, ROLE_PUMP, ROLE_REMOVAL, ROLE_WIPE, keyword,
};

/// Keyword ability "haste" in the Arena database.
/// At most this many hand cards are combined (2^n possibilities).
const MAX_CANDIDATES: usize = 12;

/// Legend rule: a legendary card whose name is already on my side of the
/// battlefield would go straight to the graveyard – it is no option for the
/// turn suggestion, nor is a second copy in hand.
pub fn without_legend_duplicates(db: &CardDb, hand: &[GrpId], board: &[GrpId]) -> Vec<GrpId> {
    let name_of = |g: &GrpId| db.cards.get(g).filter(|c| c.legendary).map(|c| &c.name);
    let mut taken: std::collections::HashSet<&String> = board.iter().filter_map(name_of).collect();
    hand.iter()
        .filter(|g| name_of(g).is_none_or(|name| taken.insert(name)))
        .copied()
        .collect()
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    /// Land to play.
    pub land: Option<GrpId>,
    /// The suggested land enters tapped.
    pub land_tapped: bool,
    /// Cards to cast in a sensible order.
    pub spells: Vec<GrpId>,
    pub mana_used: u32,
    pub mana_total: u32,
    /// Instant-speed cards better kept for the opponent's turn that are still
    /// affordable with the remaining mana.
    pub hold: Vec<GrpId>,
    /// Strongest opposing creature – target for removal.
    pub target: Option<GrpId>,
}

/// Color mask a land produces (unknown lands count as any color).
fn land_colors(db: &CardDb, grp: GrpId) -> u8 {
    db.cards.get(&grp).map(|c| c.colors).unwrap_or(0x1F)
}

/// Mana sources (color masks) from untapped lands and other permanents with a
/// mana ability (e.g. Llanowar Elves); permanents without one don't count.
pub fn sources(db: &CardDb, lands: &[GrpId], permanents: &[GrpId]) -> Vec<u8> {
    lands
        .iter()
        .map(|g| land_colors(db, *g))
        .chain(
            permanents
                .iter()
                .filter_map(|g| db.cards.get(g).and_then(|c| c.mana)),
        )
        .collect()
}

/// Can the sum of the costs be paid with these mana sources (color masks)?
pub fn can_pay(cards: &[&Card], sources: &[u8]) -> bool {
    let mut generic = 0u32;
    let mut pips: Vec<u8> = Vec::new();
    for c in cards {
        let (g, colored) = c.pips();
        generic += g;
        for (i, n) in colored.iter().enumerate() {
            pips.extend(std::iter::repeat_n(1u8 << i, *n as usize));
        }
    }
    if (generic as usize + pips.len()) > sources.len() {
        return false;
    }
    // Distribute colored symbols to lands by bipartite matching.
    let mut owner: Vec<Option<usize>> = vec![None; sources.len()];
    fn assign(
        pip: usize,
        pips: &[u8],
        sources: &[u8],
        owner: &mut [Option<usize>],
        seen: &mut [bool],
    ) -> bool {
        for s in 0..sources.len() {
            if sources[s] & pips[pip] != 0 && !seen[s] {
                seen[s] = true;
                if owner[s].is_none_or(|o| assign(o, pips, sources, owner, seen)) {
                    owner[s] = Some(pip);
                    return true;
                }
            }
        }
        false
    }
    for p in 0..pips.len() {
        let mut seen = vec![false; sources.len()];
        if !assign(p, &pips, sources, &mut owner, &mut seen) {
            return false;
        }
    }
    true
}

/// Rough importance of a card for this turn.
fn value(c: &Card, threat: i32) -> i32 {
    let mut v = c.cmc as i32 * 10;
    if c.has_role(ROLE_REMOVAL) && !c.is_creature() {
        v += 20 + threat * 2;
    }
    if c.is_creature() {
        v += 2;
    }
    if c.has_role(ROLE_DRAW) {
        v += 1;
    }
    if c.is_instant() && !c.has_role(ROLE_REMOVAL) {
        // Rather keep instant tricks open.
        v -= 5;
    }
    v
}

fn order_key(c: &Card) -> u8 {
    if c.has_role(ROLE_REMOVAL) && !c.is_creature() {
        0
    } else if c.has_role(ROLE_DRAW) {
        1
    } else if !c.is_creature() {
        2
    } else {
        3
    }
}

/// Best selection from `cands` for these mana sources: (value, chosen cards).
fn best_subset<'a>(cands: &[&'a Card], sources: &[u8], threat: i32) -> (i32, Vec<&'a Card>) {
    let mut best: (i32, Vec<&Card>) = (0, Vec::new());
    for mask in 1u32..(1 << cands.len()) {
        let chosen: Vec<&Card> = cands
            .iter()
            .enumerate()
            .filter(|(i, _)| mask & (1 << i) != 0)
            .map(|(_, c)| *c)
            .collect();
        let cost: u32 = chosen.iter().map(|c| c.cmc).sum();
        if cost as usize > sources.len() {
            continue;
        }
        let score: i32 = chosen.iter().map(|c| value(c, threat)).sum();
        if score > best.0 && can_pay(&chosen, sources) {
            best = (score, chosen);
        }
    }
    best
}

/// Computes the suggestion for the own turn.
pub fn plan(
    db: &CardDb,
    hand: &[GrpId],
    untapped_lands: &[GrpId],
    mana_permanents: &[GrpId],
    land_played: bool,
    opp_creatures: &[(GrpId, i32)],
    my_creatures: u32,
) -> Plan {
    let cards: Vec<&Card> = hand.iter().filter_map(|g| db.cards.get(g)).collect();
    let target = opp_creatures
        .iter()
        .max_by_key(|(_, p)| *p)
        .map(|(g, _)| *g);
    let threat = opp_creatures.iter().map(|(_, p)| *p).max().unwrap_or(0);

    let mut cands: Vec<&Card> = Vec::new();
    let mut held: Vec<&Card> = Vec::new();
    for c in cards.iter().filter(|c| !c.is_land() && c.cmc > 0) {
        let removal = c.has_role(ROLE_REMOVAL) && !c.is_creature();
        let trick = c.is_instant() && c.has_role(ROLE_PUMP);
        if (c.has_role(ROLE_COUNTER) || trick) && !removal {
            // Counters and combat tricks are kept open for the right moment.
            held.push(c);
        } else if c.has_role(ROLE_WIPE) && opp_creatures.len() as u32 <= my_creatures {
            // Board wipes only when the opponent has more creatures.
        } else if removal && opp_creatures.is_empty() {
            if c.is_instant() {
                held.push(c);
            }
        } else {
            cands.push(c);
        }
    }
    // Expensive cards first, so the important ones stay when there are too many hand cards.
    cands.sort_by_key(|c| std::cmp::Reverse(c.cmc));
    cands.truncate(MAX_CANDIDATES);

    let base = sources(db, untapped_lands, mana_permanents);
    let mut land_options: Vec<Option<&Card>> = vec![None];
    if !land_played {
        let mut lands: Vec<&Card> = cards.iter().filter(|c| c.is_land()).copied().collect();
        lands.sort_by_key(|c| c.grp_id);
        lands.dedup_by_key(|c| c.grp_id);
        if !lands.is_empty() {
            land_options = lands.into_iter().map(Some).collect();
        }
    }
    // Colors the hand needs that no source provides yet.
    let base_mask = base.iter().fold(0u8, |m, c| m | c);
    let wanted = cards
        .iter()
        .filter(|c| !c.is_land())
        .fold(0u8, |m, c| m | c.colors)
        & !base_mask;

    let mut best: Option<((i32, bool, u32), Plan)> = None;
    for land in land_options {
        let mut sources = base.clone();
        let tapped = land.is_some_and(|l| l.enters_tapped);
        if let Some(l) = land
            && !l.enters_tapped
        {
            sources.push(l.colors);
        }
        let (score, mut chosen) = best_subset(&cands, &sources, threat);
        chosen.sort_by_key(|c| (order_key(c), std::cmp::Reverse(c.cmc)));
        let used: u32 = chosen.iter().map(|c| c.cmc).sum();
        let left = sources.len() as u32 - used;
        let new_colors = land.map_or(0, |l| (l.colors & wanted).count_ones());
        // Same benefit: rather play the tapped land now, then new colors.
        let key = (score, tapped, new_colors);
        let p = Plan {
            land: land.map(|l| l.grp_id),
            land_tapped: tapped,
            spells: chosen.iter().map(|c| c.grp_id).collect(),
            mana_used: used,
            mana_total: sources.len() as u32,
            hold: held
                .iter()
                .filter(|c| c.cmc <= left)
                .map(|c| c.grp_id)
                .collect(),
            target: if chosen
                .iter()
                .any(|c| c.has_role(ROLE_REMOVAL) && !c.is_creature())
            {
                target
            } else {
                None
            },
        };
        if best.as_ref().is_none_or(|(k, _)| key > *k) {
            best = Some((key, p));
        }
    }
    best.map(|(_, p)| p).unwrap_or_default()
}

/// Creatures without haste in the plan that can also be cast after the attack.
pub fn creatures_after_combat(db: &CardDb, plan: &Plan) -> bool {
    let cards: Vec<&Card> = plan.spells.iter().filter_map(|g| db.cards.get(g)).collect();
    !cards.is_empty()
        && cards
            .iter()
            .all(|c| c.is_creature() && !c.has_ability(keyword::HASTE))
}

/// Instant removal that is affordable now (in the opponent's turn).
pub fn instant_removal(
    db: &CardDb,
    hand: &[GrpId],
    untapped_lands: &[GrpId],
    mana_permanents: &[GrpId],
) -> Vec<GrpId> {
    let sources = sources(db, untapped_lands, mana_permanents);
    hand.iter()
        .filter_map(|g| db.cards.get(g))
        .filter(|c| c.is_instant() && c.has_role(ROLE_REMOVAL) && can_pay(&[c], &sources))
        .map(|c| c.grp_id)
        .collect()
}

#[cfg(test)]
#[path = "tests/turnplan.rs"]
mod tests;
