//! Partie-Auswertung aus der `Player.log` von MTG Arena.
//!
//! Arena schreibt mit eingeschalteten „Detaillierten Logs“ jeden Spielzustand
//! als JSON mit. Daraus werden je Partie Zugstatistiken gewonnen und dauerhaft
//! in `games.json` gespeichert – Arena überschreibt das Log bei jedem Start.
//! Aus den Statistiken leitet [`report`] konkrete Verbesserungstipps ab.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::carddb::{Card, CardDb};

/// Stand am Ende eines Zuges.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TurnStat {
    pub turn: u32,
    /// Mein Zug?
    pub mine: bool,
    pub life_me: i32,
    pub life_opp: i32,
    pub lands_me: u32,
    pub lands_opp: u32,
    pub creatures_me: u32,
    pub creatures_opp: u32,
    /// Ungetappte eigene Länder am Zugende.
    pub untapped_lands: u32,
    /// Eigene Handkarten (grpIds) am Zugende.
    pub hand: Vec<u32>,
    pub land_played: bool,
    /// Vom aktiven Spieler gewirkte Zauber.
    pub spells: u32,
    /// Hat der aktive Spieler angegriffen?
    pub attacked: bool,
    /// Eigene Kreaturen, die zu Beginn dieses Zuges schon auf dem Feld waren.
    pub creatures_at_start: u32,
    /// Mein Zug: einsatzbereite Kreaturen (grpIds), die gefahrlos hätten
    /// angreifen können, es aber nicht taten.
    pub safe_idle: Vec<u32>,
    /// Gegnerzug: habe ich geblockt?
    pub blocked: bool,
    /// Gegnerzug: meine ungetappten Kreaturen vor dem Kampf.
    pub my_untapped_creatures: u32,
    /// Von mir in diesem Zug gewirkte Karten (grpIds), auch im Gegnerzug.
    pub cast: Vec<u32>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct GameRecord {
    /// `matchId#Spielnummer`
    pub id: String,
    pub started_ms: u64,
    /// Arena-Event, z. B. „Ladder“ (Rangliste) oder „Play“.
    pub event: String,
    pub my_seat: u32,
    /// Habe ich angefangen? `None`, wenn der Anfang nicht im Log steht.
    pub on_play: Option<bool>,
    pub mulligans: u32,
    /// `Some(true)` = gewonnen.
    pub won: Option<bool>,
    /// Arena-Grund, z. B. `ResultReason_Concede`.
    pub reason: String,
    /// Wurde die Partie von Anfang an mitgeschrieben?
    pub complete: bool,
    pub turns: Vec<TurnStat>,
    /// Eigenes Deck (grpIds, Mehrfachnennung = Anzahl).
    pub deck: Vec<u32>,
    /// Vom Gegner gesehene Karten (grpId -> Anzahl Exemplare).
    pub opp_cards: HashMap<u32, u32>,
    pub discards_me: u32,
    /// Behaltene Starthand (grpIds).
    pub opening_hand: Vec<u32>,
    /// Schaden an mir je Quelle (grpId -> Schaden).
    pub damage_taken: HashMap<u32, u32>,
    /// Schaden meiner Karten am Gegner (grpId -> Schaden).
    pub damage_dealt: HashMap<u32, u32>,
}

impl GameRecord {
    /// Karten, die ich in dieser Partie gezogen bzw. gesehen habe (Starthand,
    /// Hand an einem Zugende oder gewirkt).
    pub fn seen_cards(&self) -> HashSet<u32> {
        self.opening_hand
            .iter()
            .chain(self.turns.iter().flat_map(|t| t.hand.iter().chain(&t.cast)))
            .copied()
            .collect()
    }

    pub fn cast_cards(&self) -> HashSet<u32> {
        self.turns
            .iter()
            .flat_map(|t| t.cast.iter())
            .copied()
            .collect()
    }
}

impl GameRecord {
    pub fn last_turn(&self) -> u32 {
        self.turns.last().map(|t| t.turn).unwrap_or(0)
    }

    pub fn final_life(&self) -> (i32, i32) {
        self.turns
            .last()
            .map(|t| (t.life_me, t.life_opp))
            .unwrap_or((20, 20))
    }

    pub fn conceded(&self) -> bool {
        self.reason.contains("Concede")
    }
}

// Arena-Kennungen von Schlüsselwort-Fähigkeiten (aus der Kartendatenbank).
const KW_DEATHTOUCH: u32 = 1;
const KW_DEFENDER: u32 = 2;
const KW_FLYING: u32 = 8;
const KW_HASTE: u32 = 9;
const KW_REACH: u32 = 13;
const KW_MENACE: u32 = 142;

#[derive(Clone, Default)]
struct Obj {
    grp_id: u32,
    zone: u32,
    owner: u32,
    controller: u32,
    land: bool,
    creature: bool,
    tapped: bool,
    attacking: bool,
    blocking: bool,
    sick: bool,
    power: i32,
    toughness: i32,
    keywords: Vec<u32>,
}

impl Obj {
    fn live(&self) -> LiveCreature {
        LiveCreature {
            power: self.power.max(0),
            flying: self.has(KW_FLYING),
            reach: self.has(KW_FLYING) || self.has(KW_REACH),
            menace: self.has(KW_MENACE),
        }
    }

    fn has(&self, kw: u32) -> bool {
        self.keywords.contains(&kw)
    }

    fn can_block(&self, attacker: &Obj) -> bool {
        !attacker.has(KW_FLYING) || self.has(KW_FLYING) || self.has(KW_REACH)
    }

    fn kills(&self, other: &Obj) -> bool {
        self.power > 0 && (self.power >= other.toughness || self.has(KW_DEATHTOUCH))
    }
}

/// Kann `attacker` angreifen, ohne dass ein möglicher Block ihn tötet?
fn safe_attack(attacker: &Obj, blockers: &[&Obj]) -> bool {
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
struct Tracker {
    rec: GameRecord,
    objects: HashMap<u32, Obj>,
    /// zoneId -> (Typ, Besitzer, Objekte)
    zones: HashMap<u32, (String, u32, Vec<u32>)>,
    life: HashMap<u32, i32>,
    turn: u32,
    active: u32,
    phase: String,
    step: String,
    last_state_id: u64,
    seen_annotations: HashSet<u64>,
    cur: TurnStat,
    mulligan_reqs: u32,
    seen_cards: HashMap<u32, HashSet<u32>>,
    /// Instanzen, die in diesem Zug angegriffen haben.
    attacked_ids: HashSet<u32>,
    /// Vor dem Kampf: (Instanz, grpId) meiner gefahrlos angreifbaren Kreaturen.
    safe_snapshot: Vec<(u32, u32)>,
    finished: bool,
}

fn u(v: &Value) -> u32 {
    v.as_u64().unwrap_or(0) as u32
}

fn has_type(o: &Value, t: &str) -> bool {
    o["cardTypes"]
        .as_array()
        .is_some_and(|a| a.iter().any(|x| x.as_str() == Some(t)))
}

impl Tracker {
    fn new(seat: u32, ts: u64) -> Tracker {
        let mut t = Tracker::default();
        t.rec.my_seat = seat;
        t.rec.started_ms = ts;
        t
    }

    fn seat(&self) -> u32 {
        self.rec.my_seat
    }

    fn opponent(&self) -> u32 {
        if self.seat() == 1 { 2 } else { 1 }
    }

    fn zone_type(&self, zone: u32) -> &str {
        self.zones.get(&zone).map(|z| z.0.as_str()).unwrap_or("")
    }

    fn battlefield(&self) -> impl Iterator<Item = (&u32, &Obj)> {
        self.objects
            .iter()
            .filter(|(_, o)| self.zone_type(o.zone) == "ZoneType_Battlefield")
    }

    fn count(&self, controller: u32, f: impl Fn(&Obj) -> bool) -> u32 {
        self.battlefield()
            .filter(|(_, o)| o.controller == controller && f(o))
            .count() as u32
    }

    fn hand(&self) -> Vec<u32> {
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
    fn snapshot_attack(&mut self) {
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
    fn close_turn(&mut self) {
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

    fn start_turn(&mut self, turn: u32, active: u32) {
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
    fn before_attacks(&self) -> bool {
        self.attacked_ids.is_empty()
            && (self.phase == "Phase_Main1"
                || (self.phase == "Phase_Combat"
                    && matches!(
                        self.step.as_str(),
                        "Step_BeginCombat" | "Step_DeclareAttack" | ""
                    )))
    }

    fn read_object(o: &Value) -> Obj {
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

    fn apply(&mut self, g: &Value) {
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

    fn annotation(&mut self, a: &Value) {
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

    fn live(&self) -> LiveState {
        let (me, opp) = (self.seat(), self.opponent());
        let my_turn = self.active == me;
        let before = my_turn && self.before_attacks();
        LiveState {
            turn: self.turn,
            my_turn,
            phase: self.phase.clone(),
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
            opp_attack_power: if my_turn {
                0
            } else {
                self.battlefield()
                    .filter(|(_, o)| o.controller == opp && o.attacking)
                    .map(|(_, o)| o.power.max(0))
                    .sum()
            },
            my_untapped_creatures: self.count(me, |o| o.creature && !o.tapped),
            opp_cards: self.seen_cards.keys().copied().collect(),
            attackers: self
                .battlefield()
                .map(|(_, o)| o)
                .filter(|o| {
                    o.controller == me
                        && o.creature
                        && !o.tapped
                        && (!o.sick || o.has(KW_HASTE))
                        && !o.has(KW_DEFENDER)
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
        }
    }

    fn finish(mut self, won: Option<bool>, reason: &str) -> GameRecord {
        self.close_turn();
        self.rec.won = won;
        self.rec.reason = reason.to_string();
        self.rec.mulligans = self.mulligan_reqs.saturating_sub(1);
        self.rec.opp_cards = self
            .seen_cards
            .iter()
            .map(|(g, ids)| (*g, ids.len().min(4) as u32))
            .collect();
        self.rec
    }
}

/// Kreatur im Spiel, für die Angriffsrechnung des Begleitfensters.
#[derive(Clone, Debug, Default)]
pub struct LiveCreature {
    pub power: i32,
    pub flying: bool,
    /// Kann Flieger blocken (Fliegend oder Reichweite).
    pub reach: bool,
    pub menace: bool,
}

/// Momentaufnahme einer laufenden Partie für das Begleitfenster.
#[derive(Clone, Debug, Default)]
pub struct LiveState {
    pub turn: u32,
    pub my_turn: bool,
    pub phase: String,
    pub life_me: i32,
    pub life_opp: i32,
    /// Eigene Handkarten (grpIds).
    pub hand: Vec<u32>,
    pub lands_me: u32,
    pub lands_untapped: u32,
    /// grpIds der eigenen ungetappten Länder (für die Farbprüfung).
    pub untapped_lands: Vec<u32>,
    /// Kreaturen des Gegners: (grpId, Kraft).
    pub opp_creatures: Vec<(u32, i32)>,
    pub land_played: bool,
    pub creatures_me: u32,
    pub creatures_opp: u32,
    /// Starthand wird gerade angeboten (Mulligan-Entscheidung).
    pub mulligan: bool,
    /// Kreaturen (grpIds), die gerade gefahrlos angreifen könnten.
    pub safe_attackers: Vec<u32>,
    /// Gegnerzug: Anzahl angreifender gegnerischer Kreaturen und ihre Kraft.
    pub opp_attackers: u32,
    pub opp_attack_power: i32,
    pub my_untapped_creatures: u32,
    /// Bisher gesehene Karten des Gegners (grpIds).
    pub opp_cards: Vec<u32>,
    /// Eigene Kreaturen, die jetzt angreifen könnten.
    pub attackers: Vec<LiveCreature>,
    /// Ungetappte Kreaturen des Gegners (mögliche Blocker).
    pub blockers: Vec<LiveCreature>,
}

/// Liest eine Logdatei Zeile für Zeile – für das komplette Einlesen ebenso
/// wie für das Mitlesen während einer laufenden Partie.
#[derive(Default)]
pub struct LogParser {
    out: Vec<GameRecord>,
    events: HashMap<String, String>,
    deck: Vec<u32>,
    cur: Option<Tracker>,
    /// Zuletzt beendete Partie (für die Anzeige „Partie vorbei“).
    pub last_finished: Option<GameRecord>,
}

impl LogParser {
    pub fn feed(&mut self, line: &[u8]) {
        if line.first() != Some(&b'{') {
            return;
        }
        let text = String::from_utf8_lossy(line);
        let is_gre = text.contains("\"greToClientEvent\"");
        let is_room = text.contains("\"matchGameRoomStateChangedEvent\"");
        if !is_gre && !is_room {
            return;
        }
        let Ok(o) = serde_json::from_str::<Value>(&text) else {
            return;
        };
        let ts = o["timestamp"]
            .as_str()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0u64);

        if is_room {
            let cfg = &o["matchGameRoomStateChangedEvent"]["gameRoomInfo"]["gameRoomConfig"];
            if let Some(match_id) = cfg["matchId"].as_str() {
                let seat = self.cur.as_ref().map(|t| t.seat()).unwrap_or(1);
                for p in cfg["reservedPlayers"].as_array().into_iter().flatten() {
                    if u(&p["systemSeatId"]) == seat
                        && let Some(ev) = p["eventId"].as_str()
                    {
                        self.events.insert(match_id.to_string(), ev.to_string());
                    }
                }
            }
            return;
        }

        for m in o["greToClientEvent"]["greToClientMessages"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let seat = m["systemSeatIds"][0].as_u64().unwrap_or(1) as u32;
            match m["type"].as_str().unwrap_or("") {
                "GREMessageType_ConnectResp" => {
                    self.deck = m["connectResp"]["deckMessage"]["deckCards"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(u)
                        .collect();
                }
                "GREMessageType_MulliganReq" => {
                    if let Some(t) = self.cur.as_mut() {
                        t.mulligan_reqs += 1;
                        // Die zuletzt angebotene Hand ist die behaltene.
                        t.rec.opening_hand = t.hand();
                    }
                }
                _ => {}
            }
            let g = &m["gameStateMessage"];
            if g.is_null() {
                continue;
            }
            let info = &g["gameInfo"];
            let known = info["matchID"]
                .as_str()
                .map(|m| format!("{m}#{}", info["gameNumber"].as_u64().unwrap_or(1)));
            // Arena schickt die Partie-Kennung nicht in jeder Nachricht mit. Beginnt
            // das Log mitten in einer Partie, steht sie erst am Ende fest.
            let new_game = match (&self.cur, &known) {
                (None, _) => true,
                (Some(t), Some(id)) => !t.rec.id.is_empty() && &t.rec.id != id,
                (Some(t), None) => t.seat() != seat,
            };
            if new_game {
                // Eine vorige Partie ohne Ergebnis wird verworfen.
                let mut t = Tracker::new(seat, ts);
                t.rec.deck = std::mem::take(&mut self.deck);
                self.cur = Some(t);
            }
            if let (Some(t), Some(id)) = (self.cur.as_mut(), known) {
                t.rec.id = id;
            }
            let Some(t) = self.cur.as_mut() else { continue };
            if t.finished {
                continue;
            }
            t.apply(g);
            if info["stage"].as_str() == Some("GameStage_GameOver") {
                let result = info["results"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|r| r["scope"].as_str() == Some("MatchScope_Game"));
                if let Some(r) = result {
                    t.finished = true;
                    let my_team = g["players"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .find(|p| u(&p["systemSeatNumber"]) == t.seat())
                        .map(|p| u(&p["teamId"]))
                        .filter(|team| *team > 0)
                        .unwrap_or(t.seat());
                    let won = match r["result"].as_str() {
                        Some("ResultType_WinLoss") => Some(u(&r["winningTeamId"]) == my_team),
                        _ => None,
                    };
                    let reason = r["reason"].as_str().unwrap_or("").to_string();
                    let done = self.cur.take().unwrap();
                    if !done.rec.id.is_empty() {
                        let mut rec = done.finish(won, &reason);
                        let match_id = rec.id.split('#').next().unwrap_or("");
                        rec.event = self.events.get(match_id).cloned().unwrap_or_default();
                        self.last_finished = Some(rec.clone());
                        self.out.push(rec);
                    }
                }
            }
        }
    }

    /// Zustand der laufenden Partie, falls gerade eine läuft.
    pub fn live(&self) -> Option<LiveState> {
        self.cur
            .as_ref()
            .filter(|t| !t.finished && t.turn + t.mulligan_reqs > 0)
            .map(|t| t.live())
    }

    /// Bisher beendete Partien abholen (das Mitlesen braucht sie nicht aufzuheben).
    pub fn take_finished(&mut self) -> Vec<GameRecord> {
        std::mem::take(&mut self.out)
    }

    /// Alle beendeten Partien, je Partie nur die beste Aufzeichnung.
    pub fn finish(mut self) -> Vec<GameRecord> {
        for rec in &mut self.out {
            let match_id = rec.id.split('#').next().unwrap_or("");
            rec.event = self.events.get(match_id).cloned().unwrap_or_default();
        }
        // Arena meldet das Spielende oft doppelt; die zweite Meldung erzeugt eine
        // leere Kopie derselben Partie. Pro Partie nur die beste Aufzeichnung behalten.
        let mut best: Vec<GameRecord> = Vec::new();
        for rec in self.out {
            match best.iter_mut().find(|b| b.id == rec.id) {
                Some(b) => {
                    let deck = if b.deck.is_empty() {
                        rec.deck.clone()
                    } else {
                        Vec::new()
                    };
                    if quality(&rec) > quality(b) {
                        *b = rec;
                    }
                    if b.deck.is_empty() {
                        b.deck = deck;
                    }
                }
                None => best.push(rec),
            }
        }
        best
    }
}

/// Liest alle (abgeschlossenen) Partien aus einer Logdatei.
pub fn parse_log(path: &Path) -> Vec<GameRecord> {
    let Ok(file) = File::open(path) else {
        return Vec::new();
    };
    let mut parser = LogParser::default();
    for line in BufReader::new(file).split(b'\n').map_while(Result::ok) {
        parser.feed(&line);
    }
    parser.finish()
}

/// Wie vollständig ist eine Aufzeichnung? Höher = besser.
fn quality(g: &GameRecord) -> (bool, usize, bool, bool) {
    (
        g.complete,
        g.turns.len(),
        !g.opening_hand.is_empty(),
        !g.deck.is_empty(),
    )
}

/// Steht im Log, dass „Detaillierte Logs“ ausgeschaltet sind?
pub fn detailed_logs_disabled(path: &Path) -> bool {
    let Ok(file) = File::open(path) else {
        return false;
    };
    BufReader::new(file)
        .lines()
        .take(2000)
        .map_while(Result::ok)
        .any(|l| l.contains("DETAILED LOGS: DISABLED"))
}

/// Version der Auswertung. Wird sie erhöht, werden gespeicherte Partien beim
/// nächsten Einlesen aus noch vorhandenen Logs neu berechnet.
const PARSER_VERSION: u32 = 4;

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Store {
    version: u32,
    games: Vec<GameRecord>,
}

/// Gespeicherte Partien laden, neue aus den Logs dazunehmen und speichern.
pub fn sync(store: &Path, logs: &[PathBuf]) -> Vec<GameRecord> {
    // Ältere Dateien enthielten nur die Liste der Partien.
    let saved: Store = crate::paths::read_json::<Store>(store)
        .or_else(|| {
            crate::paths::read_json::<Vec<GameRecord>>(store)
                .map(|games| Store { version: 1, games })
        })
        .unwrap_or_default();
    let outdated = saved.version < PARSER_VERSION;
    let before = serde_json::to_vec(&saved.games).unwrap_or_default();
    let mut games = saved.games;
    for log in logs {
        for rec in parse_log(log) {
            match games.iter_mut().find(|g| g.id == rec.id) {
                // Vollständigere oder mit neuerer Auswertung erzeugte Aufzeichnung gewinnt.
                // Bessere Aufzeichnung gewinnt; nach einem Update der Auswertung
                // auch eine gleich gute (dann mit den neuen Daten), nie eine schlechtere.
                Some(old)
                    if quality(&rec) > quality(old)
                        || (outdated && quality(&rec) >= quality(old)) =>
                {
                    *old = rec
                }
                Some(_) => {}
                None => games.push(rec),
            }
        }
    }
    games.sort_by_key(|g| std::cmp::Reverse(g.started_ms));
    let changed = outdated || serde_json::to_vec(&games).unwrap_or_default() != before;
    if changed {
        let out = Store {
            version: PARSER_VERSION,
            games,
        };
        if let Ok(json) = serde_json::to_vec(&out) {
            let _ = crate::paths::write_atomic(store, &json);
        }
        return out.games;
    }
    games
}

// ------------------------------------------------------------ Auswertung ----

const TYPE_INSTANT: u8 = 4;

/// Ein auffälliger Moment in einer Partie.
#[derive(Clone, Debug)]
pub struct Finding {
    /// Partie-Nummer (1 = erste im übergebenen Slice).
    pub game: usize,
    /// Zug; 0 = vor dem ersten Zug (Starthand).
    pub turn: u32,
    pub text: String,
}

/// Eine Art von Fehler mit Erklärung und allen Fundstellen.
#[derive(Clone, Debug)]
pub struct Category {
    pub title: &'static str,
    /// Was man stattdessen tun sollte.
    pub advice: &'static str,
    pub items: Vec<Finding>,
}

#[derive(Clone, Debug, Default)]
pub struct Report {
    pub games: usize,
    pub wins: usize,
    pub losses: usize,
    pub on_play: (usize, usize),
    pub on_draw: (usize, usize),
    pub conceded: usize,
    pub avg_turns: f32,
    pub my_turns: u32,
    pub discards: u32,
    /// Starthand mit 2–4 Ländern: (Siege, Partien); übrige Starthände ebenso.
    pub good_keeps: (usize, usize),
    pub bad_keeps: (usize, usize),
    /// Fehlerarten, häufigste zuerst.
    pub categories: Vec<Category>,
    /// (Kartenname, Anzahl Partien)
    pub opp_cards: Vec<(String, usize)>,
    /// Was mir am meisten Schaden gemacht hat: (Kartenname, Schaden).
    pub damage_sources: Vec<(String, u32)>,
}

fn card(db: &CardDb, grp: u32) -> Option<&Card> {
    db.cards.get(&grp)
}

fn names(db: &CardDb, grps: &[u32]) -> String {
    let mut list: Vec<String> = grps
        .iter()
        .filter_map(|g| card(db, *g))
        .map(|c| c.name.clone())
        .collect();
    list.sort();
    list.dedup();
    list.join(", ")
}

/// Wertet die Partien aus. Partie-Nummern zählen ab 1 in der übergebenen Reihenfolge.
pub fn report(games: &[GameRecord], db: &CardDb) -> Report {
    let mut r = Report {
        games: games.len(),
        ..Default::default()
    };
    let mut keeps = Vec::new();
    let mut missed_lands = Vec::new();
    let mut safe_attacks = Vec::new();
    let mut unused_mana = Vec::new();
    let mut held = Vec::new();
    let mut no_block = Vec::new();
    let mut empty_board = Vec::new();
    let mut concedes = Vec::new();
    let mut turn_sum = 0;
    let mut opp: HashMap<String, usize> = HashMap::new();
    let mut damage: HashMap<String, u32> = HashMap::new();

    for (gi, g) in games.iter().enumerate() {
        let nr = gi + 1;
        match g.won {
            Some(true) => r.wins += 1,
            Some(false) => r.losses += 1,
            None => {}
        }
        if let (Some(play), Some(won)) = (g.on_play, g.won) {
            let slot = if play { &mut r.on_play } else { &mut r.on_draw };
            slot.0 += won as usize;
            slot.1 += 1;
        }
        r.discards += g.discards_me;
        turn_sum += g.last_turn();
        let seen: HashSet<String> = g
            .opp_cards
            .keys()
            .filter_map(|grp| card(db, *grp))
            .filter(|c| !c.basic)
            .map(|c| c.name.clone())
            .collect();
        for n in seen {
            *opp.entry(n).or_insert(0) += 1;
        }
        for (grp, dmg) in &g.damage_taken {
            if let Some(c) = card(db, *grp) {
                *damage.entry(c.name.clone()).or_insert(0) += dmg;
            }
        }

        // Aufgegeben, obwohl noch genug Leben übrig war
        let (life_me, life_opp) = g.final_life();
        if g.won == Some(false) && g.conceded() {
            r.conceded += 1;
            if life_me >= 5 && !g.turns.is_empty() {
                concedes.push(Finding {
                    game: nr,
                    turn: g.last_turn(),
                    text: format!("aufgegeben mit {life_me} Leben (Gegner {life_opp})"),
                });
            }
        }

        // Starthand
        let hand: Vec<&Card> = g
            .opening_hand
            .iter()
            .filter_map(|grp| card(db, *grp))
            .collect();
        if hand.len() >= 6 {
            let lands = hand.iter().filter(|c| c.is_land()).count();
            let good = (2..=4).contains(&lands);
            let slot = if good {
                &mut r.good_keeps
            } else {
                &mut r.bad_keeps
            };
            slot.0 += (g.won == Some(true)) as usize;
            slot.1 += 1;
            if !good {
                keeps.push(Finding {
                    game: nr,
                    turn: 0,
                    text: format!("{} Karten mit {lands} Ländern behalten", hand.len()),
                });
            }
        }

        // Zug-Auswertungen nur für vollständig mitgeschriebene Partien:
        // Teil-Mitschnitte kennen Hand und Feld nicht vollständig.
        if !g.complete {
            continue;
        }
        let mut castable_turns: HashMap<u32, (u32, u32)> = HashMap::new();
        let mut prev_life = 20;
        for t in &g.turns {
            let lost = prev_life - t.life_me;
            prev_life = t.life_me;
            if !t.mine {
                let before = t.life_me + lost;
                if !t.blocked && t.my_untapped_creatures > 0 && lost >= 3 && before <= 10 {
                    no_block.push(Finding {
                        game: nr,
                        turn: t.turn,
                        text: format!(
                            "{lost} Schaden bei {before} Leben genommen, {} eigene Kreatur(en) hätten blocken können",
                            t.my_untapped_creatures
                        ),
                    });
                }
                continue;
            }
            r.my_turns += 1;
            let hand: Vec<&Card> = t.hand.iter().filter_map(|grp| card(db, *grp)).collect();
            let find = |text: String| Finding {
                game: nr,
                turn: t.turn,
                text,
            };
            if !t.land_played
                && let Some(land) = hand.iter().find(|c| c.is_land())
            {
                missed_lands.push(find(format!("{} lag auf der Hand", land.name)));
            }
            if !t.safe_idle.is_empty() {
                safe_attacks.push(find(format!(
                    "{} hätte(n) gefahrlos angreifen können",
                    names(db, &t.safe_idle)
                )));
            }
            // Mana übrig, obwohl eine Kreatur/Hexerei bezahlbar gewesen wäre (Farben nicht geprüft).
            if t.untapped_lands >= 1 {
                let castable = hand
                    .iter()
                    .filter(|c| {
                        !c.is_land()
                            && c.cmc > 0
                            && c.cmc <= t.untapped_lands
                            && !c.types.contains(&TYPE_INSTANT)
                    })
                    .max_by_key(|c| c.cmc);
                if let Some(c) = castable {
                    unused_mana.push(find(format!(
                        "{} Länder frei, {} ({}) auf der Hand",
                        t.untapped_lands, c.name, c.mana_cost
                    )));
                }
            }
            if (3..=6).contains(&t.turn) && t.creatures_me == 0 {
                empty_board.push(find("keine eigene Kreatur auf dem Feld".into()));
            }
            // Wie lange lag eine bezahlbare Karte auf der Hand? Spontanzauber
            // (z. B. Schutz-Tricks) hält man oft bewusst zurück – die zählen nicht.
            let mut counted = HashSet::new();
            for c in &hand {
                if !c.is_land()
                    && !c.types.contains(&TYPE_INSTANT)
                    && c.cmc > 0
                    && c.cmc <= t.lands_me
                    && counted.insert(c.grp_id)
                {
                    let e = castable_turns.entry(c.grp_id).or_insert((0, t.turn));
                    e.0 += 1;
                    e.1 = t.turn;
                }
            }
        }
        for (grp, (n, last)) in castable_turns {
            if n >= 3
                && let Some(c) = card(db, grp)
            {
                held.push(Finding {
                    game: nr,
                    turn: last,
                    text: format!("{} lag {n} eigene Züge bezahlbar auf der Hand", c.name),
                });
            }
        }
    }
    if !games.is_empty() {
        r.avg_turns = turn_sum as f32 / games.len() as f32;
    }

    let mut cats = vec![
        Category {
            title: "Gefahrlosen Angriff ausgelassen",
            advice: "Wenn keine gegnerische Kreatur deinen Angreifer blocken und dabei töten kann, greif an. \
                     Schlimmstenfalls opfert der Gegner einen Blocker, sonst verliert er Leben. \
                     (Fliegend, Reichweite, Todesberührung und Bedrohlich sind berücksichtigt, Zaubertricks des Gegners nicht.)",
            items: safe_attacks,
        },
        Category {
            title: "Riskante Starthand behalten",
            advice: "Behalte nur Starthände mit 2–4 Ländern. Mit 0–1 Land fehlt dir das Mana, mit 5 oder mehr fehlen die Zauber. \
                     Ein Mulligan auf 6 Karten ist meist besser als so eine Hand.",
            items: keeps,
        },
        Category {
            title: "Land nicht gespielt",
            advice: "Lege jeden Zug ein Land, wenn du eins hast – auch wenn du es gerade nicht brauchst. Später fehlt es sonst.",
            items: missed_lands,
        },
        Category {
            title: "Mana ungenutzt",
            advice: "Nutze dein Mana in jedem Zug. Eine Kreatur auf der Hand macht keinen Schaden – spiel sie, sobald du kannst. \
                     (Die Farben prüft die App nicht; manchmal fehlte also die passende Farbe.)",
            items: unused_mana,
        },
        Category {
            title: "Karte lange nicht gespielt",
            advice: "Diese Karten lagen mehrere Züge bezahlbar auf deiner Hand. Spiel sie früher – \
                     oder tausch sie im Deck gegen etwas, das du wirklich spielst. (Spontanzauber zählen hier nicht mit.)",
            items: held,
        },
        Category {
            title: "Bei wenig Leben nicht geblockt",
            advice: "Bei wenig Leben lohnt es sich zu blocken, auch wenn deine Kreatur dabei stirbt. \
                     Dann sind deine Lebenspunkte wichtiger als die Kreatur.",
            items: no_block,
        },
        Category {
            title: "Keine Kreatur auf dem Feld",
            advice: "In Zug 3–6 lag keine eigene Kreatur auf dem Feld – dann kannst du keinen Druck machen und nicht blocken. \
                     Spiel früh günstige Kreaturen; ein Deck braucht genug davon für die Züge 1–3.",
            items: empty_board,
        },
        Category {
            title: "Früh aufgegeben",
            advice: "Gerade auf niedrigen Rängen machen Gegner oft Fehler, und Partien drehen sich. \
                     Spiel weiter, solange du noch Leben hast.",
            items: concedes,
        },
    ];
    for c in &mut cats {
        c.items.sort_by_key(|f| (f.game, f.turn));
    }
    cats.sort_by_key(|c| std::cmp::Reverse(c.items.len()));
    r.categories = cats;

    let mut opp: Vec<(String, usize)> = opp.into_iter().collect();
    opp.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    opp.truncate(15);
    r.opp_cards = opp;
    let mut damage: Vec<(String, u32)> = damage.into_iter().collect();
    damage.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    damage.truncate(10);
    r.damage_sources = damage;
    r
}

/// Farben eines Decks als Buchstaben (z. B. „UR“), aus den Manakosten.
pub fn deck_colors(deck: &[u32], db: &CardDb) -> String {
    let costs: String = deck
        .iter()
        .filter_map(|g| db.cards.get(g))
        .map(|c| c.mana_cost.as_str())
        .collect();
    "WUBRG".chars().filter(|c| costs.contains(*c)).collect()
}

/// Gegnerfarben aus den gesehenen Karten.
pub fn opp_colors(g: &GameRecord, db: &CardDb) -> String {
    let ids: Vec<u32> = g.opp_cards.keys().copied().collect();
    deck_colors(&ids, db)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn creature(power: i32, toughness: i32, keywords: &[u32]) -> Obj {
        Obj {
            creature: true,
            power,
            toughness,
            keywords: keywords.to_vec(),
            ..Default::default()
        }
    }

    #[test]
    fn safe_attack_rules() {
        let bear = creature(2, 2, &[]);
        let wall = creature(0, 4, &[]);
        let ogre = creature(3, 3, &[]);
        let bird = creature(1, 1, &[KW_FLYING]);
        // Kein Blocker -> gefahrlos
        assert!(safe_attack(&bear, &[]));
        // Blocker ohne Kraft kann nicht töten
        assert!(safe_attack(&bear, &[&wall]));
        // 3/3 tötet 2/2
        assert!(!safe_attack(&bear, &[&ogre]));
        // Fliegender Angreifer kann vom Oger nicht geblockt werden
        assert!(safe_attack(&bird, &[&ogre]));
        // Todesberührung tötet immer
        assert!(!safe_attack(&ogre, &[&creature(1, 1, &[KW_DEATHTOUCH])]));
        // Bedrohlich: ein einzelner Blocker reicht nicht
        assert!(safe_attack(&creature(2, 2, &[KW_MENACE]), &[&ogre]));
    }
}
