//! Verfolgt den Spielzustand einer laufenden Partie aus den Lognachrichten.

use super::*;

// Arena-Kennungen von Schlüsselwort-Fähigkeiten (aus der Kartendatenbank).
pub(super) const KW_DEATHTOUCH: u32 = 1;
pub(super) const KW_DEFENDER: u32 = 2;
pub(super) const KW_DOUBLE_STRIKE: u32 = 3;
pub(super) const KW_FIRST_STRIKE: u32 = 6;
pub(super) const KW_TRAMPLE: u32 = 14;
pub(super) const KW_INDESTRUCTIBLE: u32 = 104;
pub(super) const KW_FLYING: u32 = 8;
pub(super) const KW_HASTE: u32 = 9;
pub(super) const KW_REACH: u32 = 13;
pub(super) const KW_MENACE: u32 = 142;

#[derive(Clone, Default)]
pub(super) struct Obj {
    pub(super) grp_id: u32,
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
            grp_id: self.grp_id,
            power: self.power.max(0),
            toughness: self.toughness,
            flying: self.has(KW_FLYING),
            reach: self.has(KW_FLYING) || self.has(KW_REACH),
            menace: self.has(KW_MENACE),
            deathtouch: self.has(KW_DEATHTOUCH),
            first_strike: self.has(KW_FIRST_STRIKE) || self.has(KW_DOUBLE_STRIKE),
            trample: self.has(KW_TRAMPLE),
            indestructible: self.has(KW_INDESTRUCTIBLE),
        }
    }

    pub(super) fn has(&self, kw: u32) -> bool {
        self.keywords.contains(&kw)
    }

    pub(super) fn can_block(&self, attacker: &Obj) -> bool {
        !attacker.has(KW_FLYING) || self.has(KW_FLYING) || self.has(KW_REACH)
    }

    pub(super) fn kills(&self, other: &Obj) -> bool {
        self.power > 0 && (self.power >= other.toughness || self.has(KW_DEATHTOUCH))
    }
}

/// Kann `attacker` angreifen, ohne dass ein möglicher Block ihn tötet?
pub(super) fn safe_attack(attacker: &Obj, blockers: &[&Obj]) -> bool {
    let eligible: Vec<&&Obj> = blockers.iter().filter(|b| b.can_block(attacker)).collect();
    if attacker.has(KW_MENACE) {
        if eligible.len() < 2 {
            return true;
        }
        let mut powers: Vec<i32> = eligible.iter().map(|b| b.power.max(0)).collect();
        powers.sort_unstable_by(|a, b| b.cmp(a));
        let deathtouch = eligible.iter().any(|b| b.has(KW_DEATHTOUCH) && b.power > 0);
        return !deathtouch && powers[0] + powers[1] < attacker.toughness;
    }
    eligible.iter().all(|b| !b.kills(attacker))
}

/// Verfolgt den Spielzustand einer laufenden Partie.
#[derive(Default)]
pub(super) struct Tracker {
    pub(super) rec: GameRecord,
    pub(super) objects: HashMap<u32, Obj>,
    /// zoneId -> (Typ, Besitzer, Objekte)
    pub(super) zones: HashMap<u32, (String, u32, Vec<u32>)>,
    pub(super) life: HashMap<u32, i32>,
    pub(super) turn: u32,
    pub(super) active: u32,
    pub(super) phase: String,
    pub(super) step: String,
    pub(super) last_state_id: u64,
    pub(super) seen_annotations: HashSet<u64>,
    pub(super) cur: TurnStat,
    pub(super) mulligan_reqs: u32,
    pub(super) seen_cards: HashMap<u32, HashSet<u32>>,
    /// Instanzen, die in diesem Zug angegriffen haben.
    pub(super) attacked_ids: HashSet<u32>,
    /// Vor dem Kampf: (Instanz, grpId) meiner gefahrlos angreifbaren Kreaturen.
    pub(super) safe_snapshot: Vec<(u32, u32)>,
    pub(super) finished: bool,
}

pub(super) fn u(v: &Value) -> u32 {
    v.as_u64().unwrap_or(0) as u32
}

pub(super) fn has_type(o: &Value, t: &str) -> bool {
    o["cardTypes"]
        .as_array()
        .is_some_and(|a| a.iter().any(|x| x.as_str() == Some(t)))
}

impl Tracker {
    pub(super) fn new(seat: u32, ts: u64) -> Tracker {
        let mut t = Tracker::default();
        t.rec.my_seat = seat;
        t.rec.started_ms = ts;
        t
    }

    pub(super) fn seat(&self) -> u32 {
        self.rec.my_seat
    }

    pub(super) fn opponent(&self) -> u32 {
        if self.seat() == 1 { 2 } else { 1 }
    }

    pub(super) fn zone_type(&self, zone: u32) -> &str {
        self.zones.get(&zone).map(|z| z.0.as_str()).unwrap_or("")
    }

    pub(super) fn battlefield(&self) -> impl Iterator<Item = (&u32, &Obj)> {
        self.objects
            .iter()
            .filter(|(_, o)| self.zone_type(o.zone) == "ZoneType_Battlefield")
    }

    pub(super) fn count(&self, controller: u32, f: impl Fn(&Obj) -> bool) -> u32 {
        self.battlefield()
            .filter(|(_, o)| o.controller == controller && f(o))
            .count() as u32
    }

    pub(super) fn hand(&self) -> Vec<u32> {
        let me = self.seat();
        self.zones
            .values()
            .find(|z| z.0 == "ZoneType_Hand" && z.1 == me)
            .map(|z| {
                z.2.iter()
                    .filter_map(|id| self.objects.get(id))
                    .map(|o| o.grp_id)
                    .filter(|g| *g > 0)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Vor dem Angriff: Welche meiner Kreaturen könnten gefahrlos angreifen?
    pub(super) fn snapshot_attack(&mut self) {
        let (me, opp) = (self.seat(), self.opponent());
        let blockers: Vec<&Obj> = self
            .battlefield()
            .map(|(_, o)| o)
            .filter(|o| o.controller == opp && o.creature && !o.tapped)
            .collect();
        self.safe_snapshot = self
            .battlefield()
            .filter(|(_, o)| {
                o.controller == me
                    && o.creature
                    && !o.tapped
                    && (!o.sick || o.has(KW_HASTE))
                    && !o.has(KW_DEFENDER)
                    && o.power > 0
            })
            .filter(|(_, o)| safe_attack(o, &blockers))
            .map(|(id, o)| (*id, o.grp_id))
            .collect();
    }

    /// Schließt den laufenden Zug mit dem aktuellen Zustand ab.
    pub(super) fn close_turn(&mut self) {
        if self.turn == 0 {
            return;
        }
        let (me, opp) = (self.seat(), self.opponent());
        let mut t = std::mem::take(&mut self.cur);
        t.turn = self.turn;
        t.mine = self.active == me;
        t.life_me = self.life.get(&me).copied().unwrap_or(20);
        t.life_opp = self.life.get(&opp).copied().unwrap_or(20);
        t.lands_me = self.count(me, |o| o.land);
        t.lands_opp = self.count(opp, |o| o.land);
        t.creatures_me = self.count(me, |o| o.creature);
        t.creatures_opp = self.count(opp, |o| o.creature);
        t.untapped_lands = self.count(me, |o| o.land && !o.tapped);
        t.hand = self.hand();
        t.lands = self
            .battlefield()
            .map(|(_, o)| o)
            .filter(|o| o.controller == me && o.land)
            .map(|o| o.grp_id)
            .collect();
        t.opp_creatures = self
            .battlefield()
            .map(|(_, o)| o)
            .filter(|o| o.controller == opp && o.creature)
            .map(|o| (o.grp_id, o.power.max(0)))
            .collect();
        if t.mine {
            t.safe_idle = self
                .safe_snapshot
                .iter()
                .filter(|(id, _)| !self.attacked_ids.contains(id))
                .map(|(_, g)| *g)
                .collect();
        }
        self.rec.turns.push(t);
    }

    pub(super) fn start_turn(&mut self, turn: u32, active: u32) {
        self.close_turn();
        if self.rec.turns.is_empty() {
            self.rec.complete = turn <= 2;
            if turn == 1 {
                self.rec.on_play = Some(active == self.seat());
            }
        }
        self.turn = turn;
        self.active = active;
        self.attacked_ids.clear();
        self.safe_snapshot.clear();
        self.cur = TurnStat {
            creatures_at_start: self.count(self.seat(), |o| o.creature),
            ..Default::default()
        };
    }

    /// Liegt der Zug noch vor der Angriffserklärung?
    pub(super) fn before_attacks(&self) -> bool {
        self.attacked_ids.is_empty()
            && (self.phase == "Phase_Main1"
                || (self.phase == "Phase_Combat"
                    && matches!(
                        self.step.as_str(),
                        "Step_BeginCombat" | "Step_DeclareAttack" | ""
                    )))
    }

    pub(super) fn read_object(o: &Value) -> Obj {
        let state =
            |key: &str, values: &[&str]| o[key].as_str().is_some_and(|s| values.contains(&s));
        Obj {
            grp_id: u(&o["grpId"]),
            zone: u(&o["zoneId"]),
            owner: u(&o["ownerSeatId"]),
            controller: u(&o["controllerSeatId"]),
            land: has_type(o, "CardType_Land"),
            creature: has_type(o, "CardType_Creature"),
            tapped: o["isTapped"].as_bool().unwrap_or(false),
            attacking: state(
                "attackState",
                &["AttackState_Attacking", "AttackState_Declared"],
            ),
            blocking: state(
                "blockState",
                &["BlockState_Blocking", "BlockState_Declared"],
            ),
            sick: o["hasSummoningSickness"].as_bool().unwrap_or(false),
            power: o["power"]["value"].as_i64().unwrap_or(0) as i32,
            toughness: o["toughness"]["value"].as_i64().unwrap_or(0) as i32,
            keywords: o["uniqueAbilities"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|a| u(&a["grpId"]))
                .collect(),
        }
    }

    pub(super) fn apply(&mut self, g: &Value) {
        let state_id = g["gameStateId"].as_u64().unwrap_or(0);
        if state_id < self.last_state_id {
            return; // veraltete (nachgereichte) Nachricht
        }
        self.last_state_id = state_id;

        if g["type"].as_str() == Some("GameStateType_Full") {
            self.objects.clear();
            self.zones.clear();
        }
        // Zugwechsel zuerst: der alte Zug wird mit dem bisherigen Zustand abgeschlossen.
        let ti = &g["turnInfo"];
        if let Some(turn) = ti["turnNumber"].as_u64().map(|t| t as u32)
            && turn != self.turn
        {
            self.start_turn(turn, u(&ti["activePlayer"]));
        }
        if let Some(phase) = ti["phase"].as_str() {
            if phase != self.phase {
                self.step.clear();
            }
            self.phase = phase.to_string();
        }
        if let Some(step) = ti["step"].as_str() {
            self.step = step.to_string();
        }
        for p in g["players"].as_array().into_iter().flatten() {
            if let Some(life) = p["lifeTotal"].as_i64() {
                self.life.insert(u(&p["systemSeatNumber"]), life as i32);
            }
        }
        for z in g["zones"].as_array().into_iter().flatten() {
            let ids = z["objectInstanceIds"]
                .as_array()
                .into_iter()
                .flatten()
                .map(u)
                .collect();
            self.zones.insert(
                u(&z["zoneId"]),
                (
                    z["type"].as_str().unwrap_or("").to_string(),
                    u(&z["ownerSeatId"]),
                    ids,
                ),
            );
        }
        let (me, opp) = (self.seat(), self.opponent());
        for o in g["gameObjects"].as_array().into_iter().flatten() {
            if o["type"].as_str() != Some("GameObjectType_Card") {
                continue;
            }
            let id = u(&o["instanceId"]);
            let obj = Tracker::read_object(o);
            if obj.attacking && obj.controller == self.active {
                self.cur.attacked = true;
                self.attacked_ids.insert(id);
            }
            if obj.blocking && obj.controller == me && self.active == opp {
                self.cur.blocked = true;
            }
            // Gegnerische Karten merken, sobald sie öffentlich sind.
            let public = matches!(
                self.zone_type(obj.zone),
                "ZoneType_Battlefield"
                    | "ZoneType_Stack"
                    | "ZoneType_Graveyard"
                    | "ZoneType_Exile"
                    | "ZoneType_Revealed"
            );
            if public && obj.owner == opp && obj.grp_id > 0 {
                self.seen_cards.entry(obj.grp_id).or_default().insert(id);
            }
            self.objects.insert(id, obj);
        }
        for id in g["diffDeletedInstanceIds"].as_array().into_iter().flatten() {
            self.objects.remove(&u(id));
        }
        for a in g["annotations"].as_array().into_iter().flatten() {
            self.annotation(a);
        }
        // Stand vor dem Kampf festhalten.
        if self.before_attacks() {
            if self.active == me {
                self.snapshot_attack();
            } else if self.active == opp {
                self.cur.my_untapped_creatures = self.count(me, |o| o.creature && !o.tapped);
            }
        }
    }

    pub(super) fn annotation(&mut self, a: &Value) {
        let id = a["id"].as_u64().unwrap_or(0);
        if !self.seen_annotations.insert(id) {
            return;
        }
        let types = a["type"].as_array().cloned().unwrap_or_default();
        let is = |t: &str| types.iter().any(|x| x.as_str() == Some(t));
        let detail = |key: &str| {
            a["details"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|d| d["key"].as_str() == Some(key))
                .cloned()
                .unwrap_or(Value::Null)
        };
        if is("AnnotationType_DamageDealt") {
            let hits = |seat: u32| {
                a["affectedIds"]
                    .as_array()
                    .is_some_and(|t| t.iter().any(|x| u(x) == seat))
            };
            let (hits_me, hits_opp) = (hits(self.seat()), hits(self.opponent()));
            let (source, source_ctrl) = self
                .objects
                .get(&u(&a["affectorId"]))
                .map(|o| (o.grp_id, o.controller))
                .unwrap_or((0, 0));
            let damage = u(&detail("damage")["valueInt32"][0]);
            if source > 0 && damage > 0 {
                if hits_me {
                    *self.rec.damage_taken.entry(source).or_insert(0) += damage;
                }
                if hits_opp && source_ctrl == self.seat() {
                    *self.rec.damage_dealt.entry(source).or_insert(0) += damage;
                }
            }
        }
        if !is("AnnotationType_ZoneTransfer") {
            return;
        }
        let category = detail("category")["valueString"][0]
            .as_str()
            .unwrap_or("")
            .to_string();
        for target in a["affectedIds"].as_array().into_iter().flatten() {
            let (controller, grp) = self
                .objects
                .get(&u(target))
                .map(|o| (o.controller, o.grp_id))
                .unwrap_or((0, 0));
            // Eigene Zauber merken – auch Spontanzauber im gegnerischen Zug.
            if category == "CastSpell" && controller == self.seat() && grp > 0 {
                self.cur.cast.push(grp);
            }
            match category.as_str() {
                "PlayLand" if controller == self.active => self.cur.land_played = true,
                "CastSpell" if controller == self.active => self.cur.spells += 1,
                "Discard" if controller == self.seat() => self.rec.discards_me += 1,
                _ => {}
            }
        }
    }
}
