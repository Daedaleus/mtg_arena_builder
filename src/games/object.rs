//! A game object (card in play) as the log sees it, and whether an attack is
//! safe.

use super::*;

/// Game object with the values the analysis needs.
#[derive(Clone, Default)]
pub(super) struct Obj {
    pub(super) grp_id: GrpId,
    pub(super) zone: u32,
    pub(super) owner: u32,
    pub(super) controller: u32,
    pub(super) land: bool,
    pub(super) creature: bool,
    pub(super) tapped: bool,
    pub(super) attacking: bool,
    pub(super) blocking: bool,
    pub(super) sick: bool,
    pub(super) power: i32,
    pub(super) toughness: i32,
    pub(super) keywords: Vec<u32>,
}

impl Obj {
    pub(super) fn live(&self) -> LiveCreature {
        LiveCreature {
            grp_id: self.grp_id.0,
            power: self.power.max(0),
            toughness: self.toughness,
            flying: self.has(kw::FLYING),
            reach: self.has(kw::FLYING) || self.has(kw::REACH),
            menace: self.has(kw::MENACE),
            deathtouch: self.has(kw::DEATHTOUCH),
            first_strike: self.has(kw::FIRST_STRIKE) || self.has(kw::DOUBLE_STRIKE),
            trample: self.has(kw::TRAMPLE),
            indestructible: self.has(kw::INDESTRUCTIBLE),
        }
    }

    pub(super) fn has(&self, kw: u32) -> bool {
        self.keywords.contains(&kw)
    }

    pub(super) fn can_block(&self, attacker: &Obj) -> bool {
        !attacker.has(kw::FLYING) || self.has(kw::FLYING) || self.has(kw::REACH)
    }

    pub(super) fn kills(&self, other: &Obj) -> bool {
        self.power > 0 && (self.power >= other.toughness || self.has(kw::DEATHTOUCH))
    }
}

/// Can `attacker` attack without a possible block killing it?
pub(super) fn safe_attack(attacker: &Obj, blockers: &[&Obj]) -> bool {
    let eligible: Vec<&&Obj> = blockers.iter().filter(|b| b.can_block(attacker)).collect();
    if attacker.has(kw::MENACE) {
        if eligible.len() < 2 {
            return true;
        }
        let mut powers: Vec<i32> = eligible.iter().map(|b| b.power.max(0)).collect();
        powers.sort_unstable_by(|a, b| b.cmp(a));
        let deathtouch = eligible
            .iter()
            .any(|b| b.has(kw::DEATHTOUCH) && b.power > 0);
        return !deathtouch && powers[0] + powers[1] < attacker.toughness;
    }
    eligible.iter().all(|b| !b.kills(attacker))
}
