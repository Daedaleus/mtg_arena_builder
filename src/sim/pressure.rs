//! Opponent pressure: a fast opponent plays a creature every turn and
//! attacks with everything that is ready. Blocks follow the companion's block
//! suggestion, removal hits the strongest opposing creature.
//!
//! Both sides attack (race): own creatures attack when it is lethal, or safe
//! and you survive the next counterattack. Whoever attacked is tapped and
//! can't block in the opponent's next turn. The game ends as soon as one
//! side is at 0 life.
//!
//! The opponent can also remove an own creature (at the start of its turn,
//! the strongest) and play a trick in combat: after the blocks one of its
//! creatures gets +2/+2.

use super::*;
use crate::carddb::keyword as kw;
use crate::carddb::{ROLE_REMOVAL, ROLE_WIPE};
use crate::combat;
use crate::games::LiveCreature;

/// Creature of the fast opponent per turn: (power, toughness).
/// Hard start on turns 1–6, after that only smaller stragglers.
pub const OPP_CURVE: [(i32, i32); TURNS] = [
    (2, 1),
    (2, 2),
    (3, 2),
    (3, 3),
    (4, 3),
    (4, 4),
    (2, 2),
    (2, 2),
    (2, 2),
    (2, 2),
];
pub use crate::games::START_LIFE;
/// Bonus of an opposing combat trick (+X/+X until end of turn).
pub const TRICK_BONUS: i32 = 2;

/// Combat tricks in a fight: for the attacking side (on a blocked attacker,
/// otherwise extra damage) and for the defending side (on a blocker).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Tricks {
    attacker: bool,
    defender: bool,
}

/// How well does the deck hold up against a fast opponent?
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pressure {
    /// Average own life after each turn.
    pub life: [f32; TURNS],
    /// Average life of the opponent after each turn.
    pub opp_life: [f32; TURNS],
    /// Share of games with at most 5 life at the end (defeated included).
    pub low: f32,
    /// Share of games lost by the last turn.
    pub dead: f32,
    /// Share of games in which the opponent is defeated by the last turn.
    pub won: f32,
    /// Share of games in which you are ahead at the end (still alive and the
    /// opponent defeated or with less life).
    pub ahead: f32,
    /// Average number of defeated opposing creatures.
    pub kills: f32,
}

/// Creature from a card (stats and keywords), with a unique id.
pub(super) fn creature_of(card: &Card, id: u32) -> LiveCreature {
    let has = |k: u32| card.has_ability(k);
    LiveCreature {
        grp_id: id,
        power: card.power.parse().unwrap_or(0),
        toughness: card.toughness.parse().unwrap_or(1),
        flying: has(kw::FLYING),
        reach: has(kw::FLYING) || has(kw::REACH),
        menace: has(kw::MENACE),
        deathtouch: has(kw::DEATHTOUCH),
        first_strike: has(kw::FIRST_STRIKE) || has(kw::DOUBLE_STRIKE),
        trample: has(kw::TRAMPLE),
        indestructible: has(kw::INDESTRUCTIBLE),
    }
}

/// A creature in play.
#[derive(Clone, Debug)]
pub(super) struct Unit {
    /// grpId of the card (for mana creatures; 0 for the opponent).
    pub card: GrpId,
    pub c: LiveCreature,
    /// May attack (no more summoning sickness).
    pub ready: bool,
    pub tapped: bool,
    /// Prowess: +1/+1 until end of turn per noncreature spell cast.
    pub prowess: bool,
}

/// Can `blocker` block the attacker, kill it and survive itself?
/// (A trade is fine in a race.)
fn threatens(blocker: &LiveCreature, attacker: &LiveCreature) -> bool {
    let (attacker_dies, blocker_dies) = combat::fight(attacker, &[blocker]);
    (!attacker.flying || blocker.reach) && attacker_dies && !blocker_dies[0]
}

/// Combat: `attackers` attack, `defenders` block following the block suggestion.
/// An opposing trick only comes after the blocks (the block suggestion doesn't
/// know it). Returns (damage to the defender, ids of dead attackers, of dead
/// blockers).
fn battle(
    attackers: &[LiveCreature],
    defenders: &[LiveCreature],
    life: i32,
    tricks: Tricks,
) -> (i32, Vec<u32>, Vec<u32>) {
    let plan = combat::plan_blocks(attackers, defenders, life);
    let mut damage = plan.damage;
    let first_block = plan.blocks.first();
    let pumped_attacker = first_block.filter(|_| tricks.attacker).map(|b| b.attacker);
    let pumped_blocker = first_block
        .filter(|_| tricks.defender)
        .and_then(|b| b.blockers.first().copied());
    if tricks.attacker && pumped_attacker.is_none() && damage > 0 {
        damage += TRICK_BONUS;
    }
    let pump = |c: &LiveCreature| {
        let mut c = c.clone();
        if pumped_attacker == Some(c.grp_id) || pumped_blocker == Some(c.grp_id) {
            c.power += TRICK_BONUS;
            c.toughness += TRICK_BONUS;
        }
        c
    };
    let (mut dead_att, mut dead_def) = (Vec::new(), Vec::new());
    for b in &plan.blocks {
        let Some(att) = attackers.iter().find(|a| a.grp_id == b.attacker) else {
            continue;
        };
        let att = pump(att);
        let by: Vec<LiveCreature> = defenders
            .iter()
            .filter(|c| b.blockers.contains(&c.grp_id))
            .map(pump)
            .collect();
        let by: Vec<&LiveCreature> = by.iter().collect();
        let (att_dies, dies) = combat::fight(&att, &by);
        if att_dies {
            dead_att.push(att.grp_id);
        }
        dead_def.extend(
            by.iter()
                .zip(dies)
                .filter(|(_, d)| *d)
                .map(|(c, _)| c.grp_id),
        );
    }
    (damage, dead_att, dead_def)
}

/// What happens in an opponent turn besides the new creature.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct OppTurn {
    /// The new creature flies.
    pub flying: bool,
    /// The opponent first removes the strongest own creature.
    pub removal: bool,
    /// The opponent has a combat trick for an attacker.
    pub trick: bool,
    /// I have a combat trick for a blocker (see `Board::trick_used`).
    pub own_trick: bool,
}

/// Battlefield of a simulated game against the fast opponent.
pub(super) struct Board {
    pub mine: Vec<Unit>,
    pub opp: Vec<Unit>,
    pub life: i32,
    pub opp_life: i32,
    pub kills: u32,
    /// Noncreature spells cast this own turn (for prowess).
    pub spells_cast: i32,
    /// An own combat trick was used in the last fight.
    pub trick_used: bool,
    next_id: u32,
}

impl Board {
    pub fn new() -> Board {
        Board {
            mine: Vec::new(),
            opp: Vec::new(),
            life: START_LIFE,
            opp_life: START_LIFE,
            kills: 0,
            spells_cast: 0,
            trick_used: false,
            next_id: 1,
        }
    }

    fn id(&mut self) -> u32 {
        self.next_id += 1;
        self.next_id
    }

    /// Is the game decided?
    pub fn over(&self) -> bool {
        self.life <= 0 || self.opp_life <= 0
    }

    /// Opposing creatures for the turn suggestion: (id, power).
    pub fn opp_creatures(&self) -> Vec<(GrpId, i32)> {
        // The opponent's creatures have no card, only stats.
        self.opp.iter().map(|u| (u.card, u.c.power)).collect()
    }

    /// Start of the own turn: own creatures untap and become ready.
    pub fn start_my_turn(&mut self) {
        self.spells_cast = 0;
        for u in &mut self.mine {
            u.tapped = false;
            u.ready = true;
        }
    }

    /// Own creature as it fights right now: with the prowess bonus of this turn.
    fn fighter(&self, u: &Unit) -> LiveCreature {
        let mut c = u.c.clone();
        if u.prowess {
            c.power += self.spells_cast;
            c.toughness += self.spells_cast;
        }
        c
    }

    /// An own card was played.
    pub fn cast(&mut self, card: &Card) {
        if !card.is_creature() {
            self.spells_cast += 1;
        }
        if card.is_creature() {
            let id = self.id();
            self.mine.push(Unit {
                card: card.grp_id,
                c: creature_of(card, id),
                ready: card.has_ability(kw::HASTE),
                tapped: false,
                prowess: card.has_ability(kw::PROWESS),
            });
        } else if card.has_role(ROLE_WIPE) {
            self.kills += self.opp.len() as u32;
            self.opp.clear();
            self.mine.clear();
        } else if card.has_role(ROLE_REMOVAL) || card.burn > 0 {
            // Lethal burn goes to the face.
            if card.burn > 0 && card.burn as i32 >= self.opp_life {
                self.opp_life -= card.burn as i32;
                return;
            }
            // Burn spells only hit what they can kill.
            let target = self
                .opp
                .iter()
                .enumerate()
                .filter(|(_, u)| card.burn == 0 || u.c.toughness <= card.burn as i32)
                .max_by_key(|(_, u)| u.c.power)
                .map(|(i, _)| i);
            match target {
                Some(i) if card.has_role(ROLE_REMOVAL) => {
                    self.opp.remove(i);
                    self.kills += 1;
                }
                // No suitable target: burn to the opponent's face.
                _ if card.burn > 0 => self.opp_life -= card.burn as i32,
                _ => {}
            }
        }
    }

    /// Own attack after playing cards. Returns the grpIds of own creatures that
    /// died in it.
    #[cfg(test)]
    pub fn my_attack(&mut self) -> Vec<GrpId> {
        self.my_attack_with(false, false, false)
    }

    /// Like [`Board::my_attack`]; `hesitant` = only lethal attacks (habit of
    /// skipping safe attacks), `opp_trick` = the opponent has a combat trick
    /// for a blocker, `own_trick` = I have one for an attacker (counts for
    /// prowess when used; see `trick_used`).
    pub fn my_attack_with(
        &mut self,
        hesitant: bool,
        opp_trick: bool,
        own_trick: bool,
    ) -> Vec<GrpId> {
        self.trick_used = false;
        if self.over() {
            return Vec::new();
        }
        let blockers: Vec<LiveCreature> = self
            .opp
            .iter()
            .filter(|u| !u.tapped)
            .map(|u| u.c.clone())
            .collect();
        let able: Vec<LiveCreature> = self
            .mine
            .iter()
            .filter(|u| u.ready && !u.tapped)
            .map(|u| self.fighter(u))
            .filter(|c| c.power > 0)
            .collect();
        let lethal = combat::unblocked_damage(&able, &blockers) >= self.opp_life;
        // Counterattack next turn: all opposing creatures attack.
        let threat: i32 = self.opp.iter().map(|u| u.c.power).sum();
        let attackers: Vec<LiveCreature> = if lethal {
            able
        } else if self.life > threat && !hesitant {
            able.into_iter()
                .filter(|a| !blockers.iter().any(|b| threatens(b, a)))
                .collect()
        } else {
            Vec::new()
        };
        if attackers.is_empty() {
            return Vec::new();
        }
        // An own trick is cast during combat and counts for prowess too.
        self.trick_used = own_trick;
        let attackers: Vec<LiveCreature> = if own_trick {
            self.spells_cast += 1;
            self.mine
                .iter()
                .filter(|u| attackers.iter().any(|a| a.grp_id == u.c.grp_id))
                .map(|u| self.fighter(u))
                .collect()
        } else {
            attackers
        };
        let tricks = Tricks {
            attacker: own_trick,
            defender: opp_trick,
        };
        let (damage, dead_att, dead_def) = battle(&attackers, &blockers, self.opp_life, tricks);
        self.opp_life -= damage;
        self.kills += dead_def.len() as u32;
        self.opp.retain(|u| !dead_def.contains(&u.c.grp_id));
        for u in &mut self.mine {
            if attackers.iter().any(|a| a.grp_id == u.c.grp_id) {
                u.tapped = true;
            }
        }
        self.remove_mine(&dead_att)
    }

    /// Removes dead own creatures and returns their grpIds.
    fn remove_mine(&mut self, dead: &[u32]) -> Vec<GrpId> {
        let died = self
            .mine
            .iter()
            .filter(|u| dead.contains(&u.c.grp_id))
            .map(|u| u.card)
            .collect();
        self.mine.retain(|u| !dead.contains(&u.c.grp_id));
        died
    }

    /// The opponent's turn: attack, block, then play the next creature.
    /// Returns the grpIds of the own creatures that died in it.
    #[cfg(test)]
    pub fn opp_turn(&mut self, turn: usize) -> Vec<GrpId> {
        let new = OPP_CURVE[(turn - 1).min(TURNS - 1)];
        self.opp_turn_with(Some(new), OppTurn::default())
    }

    /// The opponent's turn with a given new creature (power, toughness); what
    /// else happens is in `how`.
    pub fn opp_turn_with(&mut self, new: Option<(i32, i32)>, how: OppTurn) -> Vec<GrpId> {
        self.trick_used = false;
        let OppTurn {
            flying,
            removal,
            trick,
            own_trick,
        } = how;
        if self.over() {
            return Vec::new();
        }
        let mut died = Vec::new();
        if removal
            && let Some(i) = self
                .mine
                .iter()
                .enumerate()
                .max_by_key(|(_, u)| (u.c.power, u.c.toughness))
                .map(|(i, _)| i)
        {
            died.push(self.mine.remove(i).card);
        }
        // Whatever was there at the start of the turn untaps and may attack.
        for u in &mut self.opp {
            u.tapped = false;
            u.ready = true;
        }
        let attackers: Vec<LiveCreature> = self.opp.iter().map(|u| u.c.clone()).collect();
        // Whoever attacked in the own turn is still tapped.
        let blockers: Vec<LiveCreature> = self
            .mine
            .iter()
            .filter(|u| !u.tapped)
            .map(|u| u.c.clone())
            .collect();
        let tricks = Tricks {
            attacker: trick,
            defender: own_trick && !blockers.is_empty(),
        };
        self.trick_used = tricks.defender;
        let (damage, dead_opp, dead_mine) = battle(&attackers, &blockers, self.life, tricks);
        self.life -= damage;
        self.kills += dead_opp.len() as u32;
        self.opp.retain(|u| !dead_opp.contains(&u.c.grp_id));
        for u in &mut self.opp {
            u.tapped = true;
        }
        died.extend(self.remove_mine(&dead_mine));
        let Some((power, toughness)) = new else {
            return died;
        };
        let id = self.id();
        self.opp.push(Unit {
            card: GrpId::default(),
            c: LiveCreature {
                grp_id: id,
                power,
                toughness,
                flying,
                reach: flying,
                ..Default::default()
            },
            ready: false,
            tapped: false,
            prowess: false,
        });
        died
    }
}
