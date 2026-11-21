//! Snapshot of the running game for the companion window.

use super::*;

/// Creature on the battlefield, for the companion's combat calculation.
#[derive(Clone, Debug, Default)]
pub struct LiveCreature {
    /// Identity in combat (block assignment). In the companion the card id
    /// (`GrpId::0`), in the simulation a sequential number.
    pub grp_id: u32,
    pub power: i32,
    pub toughness: i32,
    pub flying: bool,
    /// Can block flyers (flying or reach).
    pub reach: bool,
    pub menace: bool,
    pub deathtouch: bool,
    /// First strike or double strike.
    pub first_strike: bool,
    pub trample: bool,
    pub indestructible: bool,
}

/// Snapshot of a running game for the companion window.
#[derive(Clone, Debug, Default)]
pub struct LiveState {
    pub turn: u32,
    pub my_turn: bool,
    pub phase: Phase,
    pub life_me: i32,
    pub life_opp: i32,
    /// Own hand cards (grpIds).
    pub hand: Vec<GrpId>,
    /// Brawl: own commander in the command zone.
    pub command: Vec<GrpId>,
    pub lands_me: u32,
    pub lands_untapped: u32,
    /// grpIds of the own untapped lands (for the color check).
    pub untapped_lands: Vec<GrpId>,
    /// The opponent's creatures: (grpId, power).
    pub opp_creatures: Vec<(GrpId, i32)>,
    /// Own untapped non-lands that could produce mana now (creatures
    /// without summoning sickness); whether they can is known by the card database.
    pub mana_permanents: Vec<GrpId>,
    /// Own permanents except lands (for the legend rule).
    pub board: Vec<GrpId>,
    pub land_played: bool,
    pub creatures_me: u32,
    pub creatures_opp: u32,
    /// The opening hand is being offered (mulligan decision).
    pub mulligan: bool,
    /// Creatures (grpIds) that could attack safely right now.
    pub safe_attackers: Vec<GrpId>,
    /// Opponent's turn: number of attacking opposing creatures.
    pub opp_attackers: u32,
    /// The opponent's cards seen so far (grpIds).
    pub opp_cards: Vec<GrpId>,
    /// Own creatures that could attack now.
    pub attackers: Vec<LiveCreature>,
    /// Untapped creatures of the opponent (possible blockers).
    pub blockers: Vec<LiveCreature>,
    /// Opponent's turn: the opponent's attacking creatures.
    pub opp_attacking: Vec<LiveCreature>,
    /// Opponent's turn: own untapped creatures that can block.
    pub my_blockers: Vec<LiveCreature>,
    /// Untapped lands of the opponent (open mana for tricks).
    pub opp_untapped_lands: u32,
}

impl Tracker {
    pub(super) fn live(&self) -> LiveState {
        let (me, opp) = (self.seat(), self.opponent());
        let my_turn = self.active == me;
        let before = my_turn && self.before_attacks();
        LiveState {
            turn: self.turn,
            my_turn,
            phase: self.phase,
            life_me: self.life.get(&me).copied().unwrap_or(20),
            life_opp: self.life.get(&opp).copied().unwrap_or(20),
            hand: self.hand(),
            command: self.command_zone(),
            lands_me: self.count(me, |o| o.land),
            lands_untapped: self.count(me, |o| o.land && !o.tapped),
            untapped_lands: self
                .battlefield()
                .map(|(_, o)| o)
                .filter(|o| o.controller == me && o.land && !o.tapped)
                .map(|o| o.grp_id)
                .collect(),
            opp_creatures: self
                .battlefield()
                .map(|(_, o)| o)
                .filter(|o| o.controller == opp && o.creature)
                .map(|o| (o.grp_id, o.power.max(0)))
                .collect(),
            mana_permanents: self
                .battlefield()
                .map(|(_, o)| o)
                .filter(|o| {
                    o.controller == me
                        && !o.land
                        && !o.tapped
                        && !(o.creature && o.sick && !o.has(kw::HASTE))
                })
                .map(|o| o.grp_id)
                .collect(),
            board: self.my_board(),
            land_played: self.cur.land_played,
            creatures_me: self.count(me, |o| o.creature),
            creatures_opp: self.count(opp, |o| o.creature),
            mulligan: self.turn == 0 && self.mulligan_reqs > 0,
            safe_attackers: if before {
                self.safe_snapshot.iter().map(|(_, g)| *g).collect()
            } else {
                Vec::new()
            },
            opp_attackers: if my_turn {
                0
            } else {
                self.count(opp, |o| o.attacking)
            },
            opp_cards: self.seen_cards.keys().copied().collect(),
            attackers: self
                .battlefield()
                .map(|(_, o)| o)
                .filter(|o| {
                    o.controller == me
                        && o.creature
                        && !o.tapped
                        && (!o.sick || o.has(kw::HASTE))
                        && !o.has(kw::DEFENDER)
                        && o.power > 0
                })
                .map(Obj::live)
                .collect(),
            blockers: self
                .battlefield()
                .map(|(_, o)| o)
                .filter(|o| o.controller == opp && o.creature && !o.tapped)
                .map(Obj::live)
                .collect(),
            opp_attacking: if my_turn {
                Vec::new()
            } else {
                self.battlefield()
                    .map(|(_, o)| o)
                    .filter(|o| o.controller == opp && o.attacking)
                    .map(Obj::live)
                    .collect()
            },
            my_blockers: self
                .battlefield()
                .map(|(_, o)| o)
                .filter(|o| o.controller == me && o.creature && !o.tapped)
                .map(Obj::live)
                .collect(),
            opp_untapped_lands: self.count(opp, |o| o.land && !o.tapped),
        }
    }

    pub(super) fn finish(mut self, won: Option<bool>, reason: &str) -> GameRecord {
        self.close_turn();
        self.rec.won = won;
        self.rec.reason = EndReason::from_arena(reason);
        self.rec.mulligans = self.mulligan_reqs.saturating_sub(1);
        self.rec.opp_cards = self
            .seen_cards
            .iter()
            .map(|(g, ids)| (*g, ids.len().min(4) as u32))
            .collect();
        self.rec
    }
}
