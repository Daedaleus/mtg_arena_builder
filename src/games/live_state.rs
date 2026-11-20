//! Momentaufnahme der laufenden Partie für das Begleitfenster.

use super::*;

/// Kreatur im Spiel, für die Angriffsrechnung des Begleitfensters.
#[derive(Clone, Debug, Default)]
pub struct LiveCreature {
    /// Kennung im Kampf (Block-Zuordnung). Im Begleitfenster die Karten-ID
    /// (`GrpId::0`), in der Simulation eine fortlaufende Nummer.
    pub grp_id: u32,
    pub power: i32,
    pub toughness: i32,
    pub flying: bool,
    /// Kann Flieger blocken (Fliegend oder Reichweite).
    pub reach: bool,
    pub menace: bool,
    pub deathtouch: bool,
    /// Erstschlag oder Doppelschlag.
    pub first_strike: bool,
    pub trample: bool,
    pub indestructible: bool,
}

/// Momentaufnahme einer laufenden Partie für das Begleitfenster.
#[derive(Clone, Debug, Default)]
pub struct LiveState {
    pub turn: u32,
    pub my_turn: bool,
    pub phase: Phase,
    pub life_me: i32,
    pub life_opp: i32,
    /// Eigene Handkarten (grpIds).
    pub hand: Vec<GrpId>,
    pub lands_me: u32,
    pub lands_untapped: u32,
    /// grpIds der eigenen ungetappten Länder (für die Farbprüfung).
    pub untapped_lands: Vec<GrpId>,
    /// Kreaturen des Gegners: (grpId, Kraft).
    pub opp_creatures: Vec<(GrpId, i32)>,
    /// Eigene ungetappte Nichtländer, die jetzt Mana erzeugen könnten
    /// (Kreaturen ohne Einsatzverzögerung); ob sie es können, weiß die Kartendatenbank.
    pub mana_permanents: Vec<GrpId>,
    /// Eigene bleibende Karten außer Ländern (für die Legendary-Regel).
    pub board: Vec<GrpId>,
    pub land_played: bool,
    pub creatures_me: u32,
    pub creatures_opp: u32,
    /// Starthand wird gerade angeboten (Mulligan-Entscheidung).
    pub mulligan: bool,
    /// Kreaturen (grpIds), die gerade gefahrlos angreifen könnten.
    pub safe_attackers: Vec<GrpId>,
    /// Gegnerzug: Anzahl angreifender gegnerischer Kreaturen.
    pub opp_attackers: u32,
    /// Bisher gesehene Karten des Gegners (grpIds).
    pub opp_cards: Vec<GrpId>,
    /// Eigene Kreaturen, die jetzt angreifen könnten.
    pub attackers: Vec<LiveCreature>,
    /// Ungetappte Kreaturen des Gegners (mögliche Blocker).
    pub blockers: Vec<LiveCreature>,
    /// Gegnerzug: angreifende Kreaturen des Gegners.
    pub opp_attacking: Vec<LiveCreature>,
    /// Gegnerzug: eigene ungetappte Kreaturen, die blocken können.
    pub my_blockers: Vec<LiveCreature>,
    /// Ungetappte Länder des Gegners (offenes Mana für Tricks).
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
