//! Gegner-Druck: ein schneller Gegner spielt jeden Zug eine Kreatur und
//! greift mit allem an, was bereit ist. Geblockt wird mit dem Blockvorschlag
//! des Begleitfensters, Entfernung trifft die stärkste gegnerische Kreatur.

use super::*;
use crate::carddb::{ROLE_REMOVAL, ROLE_WIPE};
use crate::combat;
use crate::games::LiveCreature;

/// Kreatur des schnellen Gegners je Zug: (Kraft, Widerstand).
pub const OPP_CURVE: [(i32, i32); TURNS] = [(2, 1), (2, 2), (3, 2), (3, 3), (4, 3), (4, 4)];
pub const START_LIFE: i32 = 20;

// Schlüsselwort-Fähigkeiten in der Kartendatenbank
const KW_DEATHTOUCH: u32 = 1;
const KW_DOUBLE_STRIKE: u32 = 3;
const KW_FIRST_STRIKE: u32 = 6;
const KW_FLYING: u32 = 8;
const KW_REACH: u32 = 13;
const KW_TRAMPLE: u32 = 14;
const KW_INDESTRUCTIBLE: u32 = 104;
const KW_MENACE: u32 = 142;

/// Wie gut hält das Deck einem schnellen Gegner stand?
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pressure {
    /// Durchschnittliche Lebenspunkte nach dem Gegnerzug (Zug 1–6).
    pub life: [f32; TURNS],
    /// Anteil der Partien mit höchstens 5 Leben nach Zug 6 (besiegt eingeschlossen).
    pub low: f32,
    /// Anteil der Partien, die bis Zug 6 verloren sind.
    pub dead: f32,
    /// Durchschnittlich besiegte gegnerische Kreaturen.
    pub kills: f32,
}

/// Kreatur aus einer Karte (Werte und Schlüsselwörter), mit eindeutiger Kennung.
pub(super) fn creature_of(card: &Card, id: u32) -> LiveCreature {
    let has = |k: u32| card.abilities.contains(&k);
    LiveCreature {
        grp_id: id,
        power: card.power.parse().unwrap_or(0),
        toughness: card.toughness.parse().unwrap_or(1),
        flying: has(KW_FLYING),
        reach: has(KW_FLYING) || has(KW_REACH),
        menace: has(KW_MENACE),
        deathtouch: has(KW_DEATHTOUCH),
        first_strike: has(KW_FIRST_STRIKE) || has(KW_DOUBLE_STRIKE),
        trample: has(KW_TRAMPLE),
        indestructible: has(KW_INDESTRUCTIBLE),
    }
}

/// Spielfeld einer simulierten Partie gegen den schnellen Gegner.
pub(super) struct Board {
    /// Eigene Kreaturen: (grpId der Karte, Kreatur mit eindeutiger Kennung).
    pub mine: Vec<(u32, LiveCreature)>,
    /// Gegnerische Kreaturen und ob sie schon angreifen dürfen.
    pub opp: Vec<(LiveCreature, bool)>,
    pub life: i32,
    pub kills: u32,
    next_id: u32,
}

impl Board {
    pub fn new() -> Board {
        Board {
            mine: Vec::new(),
            opp: Vec::new(),
            life: START_LIFE,
            kills: 0,
            next_id: 1,
        }
    }

    fn id(&mut self) -> u32 {
        self.next_id += 1;
        self.next_id
    }

    /// Gegnerische Kreaturen für den Zugvorschlag: (Kennung, Kraft).
    pub fn opp_creatures(&self) -> Vec<(u32, i32)> {
        self.opp.iter().map(|(c, _)| (c.grp_id, c.power)).collect()
    }

    /// Eine eigene Karte wurde ausgespielt.
    pub fn cast(&mut self, card: &Card) {
        if card.is_creature() {
            let id = self.id();
            self.mine.push((card.grp_id, creature_of(card, id)));
        } else if card.has_role(ROLE_WIPE) {
            self.kills += self.opp.len() as u32;
            self.opp.clear();
            self.mine.clear();
        } else if card.has_role(ROLE_REMOVAL) {
            // Brandzauber treffen nur, was sie töten können.
            let target = self
                .opp
                .iter()
                .enumerate()
                .filter(|(_, (c, _))| card.burn == 0 || c.toughness <= card.burn as i32)
                .max_by_key(|(_, (c, _))| c.power)
                .map(|(i, _)| i);
            if let Some(i) = target {
                self.opp.remove(i);
                self.kills += 1;
            }
        }
    }

    /// Zug des Gegners: angreifen, blocken, dann die nächste Kreatur spielen.
    /// Liefert die grpIds der eigenen Kreaturen, die dabei gestorben sind.
    pub fn opp_turn(&mut self, turn: usize) -> Vec<u32> {
        // Was schon zu Beginn des Zuges liegt, darf angreifen.
        for (_, ready) in &mut self.opp {
            *ready = true;
        }
        let attackers: Vec<LiveCreature> = self
            .opp
            .iter()
            .filter(|(_, ready)| *ready)
            .map(|(c, _)| c.clone())
            .collect();
        let blockers: Vec<LiveCreature> = self.mine.iter().map(|(_, c)| c.clone()).collect();
        let plan = combat::plan_blocks(&attackers, &blockers, self.life);
        self.life -= plan.damage;
        let mut dead_mine: Vec<u32> = Vec::new();
        let mut dead_opp: Vec<u32> = Vec::new();
        for b in &plan.blocks {
            let Some(att) = attackers.iter().find(|a| a.grp_id == b.attacker) else {
                continue;
            };
            let by: Vec<&LiveCreature> = blockers
                .iter()
                .filter(|c| b.blockers.contains(&c.grp_id))
                .collect();
            let (att_dies, dies) = combat::fight(att, &by);
            if att_dies {
                dead_opp.push(att.grp_id);
            }
            dead_mine.extend(
                by.iter()
                    .zip(dies)
                    .filter(|(_, d)| *d)
                    .map(|(c, _)| c.grp_id),
            );
        }
        self.kills += dead_opp.len() as u32;
        self.opp.retain(|(c, _)| !dead_opp.contains(&c.grp_id));
        let died: Vec<u32> = self
            .mine
            .iter()
            .filter(|(_, c)| dead_mine.contains(&c.grp_id))
            .map(|(g, _)| *g)
            .collect();
        self.mine.retain(|(_, c)| !dead_mine.contains(&c.grp_id));
        let (power, toughness) = OPP_CURVE[(turn - 1).min(TURNS - 1)];
        let id = self.id();
        self.opp.push((
            LiveCreature {
                grp_id: id,
                power,
                toughness,
                ..Default::default()
            },
            false,
        ));
        died
    }
}
