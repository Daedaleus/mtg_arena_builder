//! Gegner-Druck: ein schneller Gegner spielt jeden Zug eine Kreatur und
//! greift mit allem an, was bereit ist. Geblockt wird mit dem Blockvorschlag
//! des Begleitfensters, Entfernung trifft die stärkste gegnerische Kreatur.
//!
//! Beide Seiten greifen an (Rennen): Eigene Kreaturen greifen an, wenn es
//! tödlich ist oder gefahrlos und man den nächsten Gegenschlag überlebt.
//! Wer angegriffen hat, ist getappt und kann im nächsten Zug des Gegners
//! nicht blocken. Die Partie endet, sobald eine Seite 0 Leben hat.
//!
//! Der Gegner kann außerdem eine eigene Kreatur entfernen (zu Beginn seines
//! Zuges, die stärkste) und im Kampf einen Trick spielen: Nach den Blocks
//! bekommt eine seiner Kreaturen +2/+2.

use super::*;
use crate::carddb::keyword as kw;
use crate::carddb::{ROLE_REMOVAL, ROLE_WIPE};
use crate::combat;
use crate::games::LiveCreature;

/// Kreatur des schnellen Gegners je Zug: (Kraft, Widerstand).
/// Harter Start in den Zügen 1–6, danach nur noch kleinere Nachzügler.
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
/// Bonus eines gegnerischen Kampftricks (+X/+X bis Zugende).
pub const TRICK_BONUS: i32 = 2;

/// Wo der Gegner im Kampf einen Trick spielt.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Trick {
    None,
    /// Auf einen geblockten Angreifer (sonst Zusatzschaden).
    Attacker,
    /// Auf einen Blocker.
    Blocker,
}

/// Wie gut hält das Deck einem schnellen Gegner stand?
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pressure {
    /// Durchschnittliche eigene Lebenspunkte nach jedem Zug.
    pub life: [f32; TURNS],
    /// Durchschnittliche Lebenspunkte des Gegners nach jedem Zug.
    pub opp_life: [f32; TURNS],
    /// Anteil der Partien mit höchstens 5 Leben am Ende (besiegt eingeschlossen).
    pub low: f32,
    /// Anteil der Partien, die bis zum letzten Zug verloren sind.
    pub dead: f32,
    /// Anteil der Partien, in denen der Gegner bis zum letzten Zug besiegt ist.
    pub won: f32,
    /// Anteil der Partien, in denen man am Ende vorne liegt (selbst am
    /// Leben und der Gegner besiegt oder mit weniger Leben).
    pub ahead: f32,
    /// Durchschnittlich besiegte gegnerische Kreaturen.
    pub kills: f32,
}

/// Kreatur aus einer Karte (Werte und Schlüsselwörter), mit eindeutiger Kennung.
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

/// Eine Kreatur im Spiel.
#[derive(Clone, Debug)]
pub(super) struct Unit {
    /// grpId der Karte (für Manakreaturen; beim Gegner 0).
    pub card: GrpId,
    pub c: LiveCreature,
    /// Darf angreifen (keine Einsatzverzögerung mehr).
    pub ready: bool,
    pub tapped: bool,
}

/// Kann `blocker` den Angreifer blocken, töten und dabei selbst überleben?
/// (Ein Tausch ist im Rennen in Ordnung.)
fn threatens(blocker: &LiveCreature, attacker: &LiveCreature) -> bool {
    let (attacker_dies, blocker_dies) = combat::fight(attacker, &[blocker]);
    (!attacker.flying || blocker.reach) && attacker_dies && !blocker_dies[0]
}

/// Kampf: `attackers` greifen an, `defenders` blocken nach dem Blockvorschlag.
/// Ein Trick des Gegners kommt erst nach den Blocks (der Blockvorschlag kennt
/// ihn nicht). Liefert (Schaden am Verteidiger, Kennungen toter Angreifer,
/// toter Blocker).
fn battle(
    attackers: &[LiveCreature],
    defenders: &[LiveCreature],
    life: i32,
    trick: Trick,
) -> (i32, Vec<u32>, Vec<u32>) {
    let plan = combat::plan_blocks(attackers, defenders, life);
    let mut damage = plan.damage;
    let pumped = match trick {
        Trick::Attacker => plan.blocks.first().map(|b| b.attacker),
        Trick::Blocker => plan
            .blocks
            .first()
            .and_then(|b| b.blockers.first().copied()),
        Trick::None => None,
    };
    if trick == Trick::Attacker && pumped.is_none() && damage > 0 {
        damage += TRICK_BONUS;
    }
    let pump = |c: &LiveCreature| {
        let mut c = c.clone();
        if pumped == Some(c.grp_id) {
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

/// Spielfeld einer simulierten Partie gegen den schnellen Gegner.
pub(super) struct Board {
    pub mine: Vec<Unit>,
    pub opp: Vec<Unit>,
    pub life: i32,
    pub opp_life: i32,
    pub kills: u32,
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
            next_id: 1,
        }
    }

    fn id(&mut self) -> u32 {
        self.next_id += 1;
        self.next_id
    }

    /// Ist die Partie entschieden?
    pub fn over(&self) -> bool {
        self.life <= 0 || self.opp_life <= 0
    }

    /// Gegnerische Kreaturen für den Zugvorschlag: (Kennung, Kraft).
    pub fn opp_creatures(&self) -> Vec<(GrpId, i32)> {
        // Die Kreaturen des Gegners haben keine Karte, nur Werte.
        self.opp.iter().map(|u| (u.card, u.c.power)).collect()
    }

    /// Beginn des eigenen Zuges: eigene Kreaturen enttappen und werden bereit.
    pub fn start_my_turn(&mut self) {
        for u in &mut self.mine {
            u.tapped = false;
            u.ready = true;
        }
    }

    /// Eine eigene Karte wurde ausgespielt.
    pub fn cast(&mut self, card: &Card) {
        if card.is_creature() {
            let id = self.id();
            self.mine.push(Unit {
                card: card.grp_id,
                c: creature_of(card, id),
                ready: card.has_ability(kw::HASTE),
                tapped: false,
            });
        } else if card.has_role(ROLE_WIPE) {
            self.kills += self.opp.len() as u32;
            self.opp.clear();
            self.mine.clear();
        } else if card.has_role(ROLE_REMOVAL) || card.burn > 0 {
            // Tödlicher Brand geht ins Gesicht.
            if card.burn > 0 && card.burn as i32 >= self.opp_life {
                self.opp_life -= card.burn as i32;
                return;
            }
            // Brandzauber treffen nur, was sie töten können.
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
                // Kein passendes Ziel: Brand ins Gesicht des Gegners.
                _ if card.burn > 0 => self.opp_life -= card.burn as i32,
                _ => {}
            }
        }
    }

    /// Eigener Angriff nach dem Ausspielen. Liefert die grpIds eigener
    /// Kreaturen, die dabei gestorben sind.
    #[cfg(test)]
    pub fn my_attack(&mut self) -> Vec<GrpId> {
        self.my_attack_with(false, false)
    }

    /// Wie [`Board::my_attack`]; `hesitant` = nur tödliche Angriffe (Gewohnheit,
    /// gefahrlose Angriffe auszulassen), `trick` = der Gegner hat einen
    /// Kampftrick für einen Blocker.
    pub fn my_attack_with(&mut self, hesitant: bool, trick: bool) -> Vec<GrpId> {
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
            .filter(|u| u.ready && !u.tapped && u.c.power > 0)
            .map(|u| u.c.clone())
            .collect();
        let lethal = combat::unblocked_damage(&able, &blockers) >= self.opp_life;
        // Gegenschlag im nächsten Zug: alle gegnerischen Kreaturen greifen an.
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
        let trick = if trick { Trick::Blocker } else { Trick::None };
        let (damage, dead_att, dead_def) = battle(&attackers, &blockers, self.opp_life, trick);
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

    /// Entfernt tote eigene Kreaturen und liefert ihre grpIds.
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

    /// Zug des Gegners: angreifen, blocken, dann die nächste Kreatur spielen.
    /// Liefert die grpIds der eigenen Kreaturen, die dabei gestorben sind.
    #[cfg(test)]
    pub fn opp_turn(&mut self, turn: usize) -> Vec<GrpId> {
        self.opp_turn_with(Some(OPP_CURVE[(turn - 1).min(TURNS - 1)]), false, false)
    }

    /// Zug des Gegners mit vorgegebener neuer Kreatur (Kraft, Widerstand).
    /// `removal`: entfernt zuerst die stärkste eigene Kreatur; `trick`:
    /// Kampftrick für einen Angreifer.
    pub fn opp_turn_with(
        &mut self,
        new: Option<(i32, i32)>,
        removal: bool,
        trick: bool,
    ) -> Vec<GrpId> {
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
        // Was schon zu Beginn des Zuges liegt, enttappt und darf angreifen.
        for u in &mut self.opp {
            u.tapped = false;
            u.ready = true;
        }
        let attackers: Vec<LiveCreature> = self.opp.iter().map(|u| u.c.clone()).collect();
        // Wer im eigenen Zug angegriffen hat, ist noch getappt.
        let blockers: Vec<LiveCreature> = self
            .mine
            .iter()
            .filter(|u| !u.tapped)
            .map(|u| u.c.clone())
            .collect();
        let trick = if trick { Trick::Attacker } else { Trick::None };
        let (damage, dead_opp, dead_mine) = battle(&attackers, &blockers, self.life, trick);
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
                ..Default::default()
            },
            ready: false,
            tapped: false,
        });
        died
    }
}
