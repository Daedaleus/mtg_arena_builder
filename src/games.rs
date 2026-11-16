//! Partie-Auswertung aus der `Player.log` von MTG Arena.
//!
//! Arena schreibt mit eingeschalteten „Detaillierten Logs“ jeden Spielzustand
//! als JSON mit. Daraus werden je Partie Zugstatistiken gewonnen und dauerhaft
//! in `games.json` gespeichert – Arena überschreibt das Log bei jedem Start.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

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
    last_state_id: u64,
    seen_annotations: HashSet<u64>,
    cur: TurnStat,
    mulligan_reqs: u32,
    seen_cards: HashMap<u32, HashSet<u32>>,
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

    fn zone_type(&self, zone: u32) -> &str {
        self.zones.get(&zone).map(|z| z.0.as_str()).unwrap_or("")
    }

    fn on_battlefield(&self) -> impl Iterator<Item = &Obj> {
        self.objects
            .values()
            .filter(|o| self.zone_type(o.zone) == "ZoneType_Battlefield")
    }

    fn count(&self, controller: u32, f: impl Fn(&Obj) -> bool) -> u32 {
        self.on_battlefield()
            .filter(|o| o.controller == controller && f(o))
            .count() as u32
    }

    fn opponent(&self) -> u32 {
        if self.seat() == 1 { 2 } else { 1 }
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
        t.hand = self
            .zones
            .values()
            .find(|z| z.0 == "ZoneType_Hand" && z.1 == me)
            .map(|z| {
                z.2.iter()
                    .filter_map(|id| self.objects.get(id))
                    .map(|o| o.grp_id)
                    .filter(|g| *g > 0)
                    .collect()
            })
            .unwrap_or_default();
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
        self.cur = TurnStat {
            creatures_at_start: self.count(self.seat(), |o| o.creature),
            ..Default::default()
        };
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
        for o in g["gameObjects"].as_array().into_iter().flatten() {
            if o["type"].as_str() != Some("GameObjectType_Card") {
                continue;
            }
            let obj = Obj {
                grp_id: u(&o["grpId"]),
                zone: u(&o["zoneId"]),
                owner: u(&o["ownerSeatId"]),
                controller: u(&o["controllerSeatId"]),
                land: has_type(o, "CardType_Land"),
                creature: has_type(o, "CardType_Creature"),
                tapped: o["isTapped"].as_bool().unwrap_or(false),
                attacking: o["attackState"]
                    .as_str()
                    .is_some_and(|s| s == "AttackState_Attacking" || s == "AttackState_Declared"),
            };
            if obj.attacking && obj.controller == self.active {
                self.cur.attacked = true;
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
            if public && obj.owner == self.opponent() && obj.grp_id > 0 {
                self.seen_cards
                    .entry(obj.grp_id)
                    .or_default()
                    .insert(u(&o["instanceId"]));
            }
            self.objects.insert(u(&o["instanceId"]), obj);
        }
        for id in g["diffDeletedInstanceIds"].as_array().into_iter().flatten() {
            self.objects.remove(&u(id));
        }
        for a in g["annotations"].as_array().into_iter().flatten() {
            let id = a["id"].as_u64().unwrap_or(0);
            if !self.seen_annotations.insert(id) {
                continue;
            }
            let is_transfer = a["type"].as_array().is_some_and(|t| {
                t.iter()
                    .any(|x| x.as_str() == Some("AnnotationType_ZoneTransfer"))
            });
            if !is_transfer {
                continue;
            }
            let category = a["details"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|d| d["key"].as_str() == Some("category"))
                .and_then(|d| d["valueString"][0].as_str())
                .unwrap_or("");
            for target in a["affectedIds"].as_array().into_iter().flatten() {
                let controller = self
                    .objects
                    .get(&u(target))
                    .map(|o| o.controller)
                    .unwrap_or(0);
                match category {
                    "PlayLand" if controller == self.active => self.cur.land_played = true,
                    "CastSpell" if controller == self.active => self.cur.spells += 1,
                    "Discard" if controller == self.seat() => self.rec.discards_me += 1,
                    _ => {}
                }
            }
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

/// Liest alle (abgeschlossenen) Partien aus einer Logdatei.
pub fn parse_log(path: &Path) -> Vec<GameRecord> {
    let Ok(file) = File::open(path) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut events: HashMap<String, String> = HashMap::new();
    let mut deck: Vec<u32> = Vec::new();
    let mut cur: Option<Tracker> = None;

    for line in BufReader::new(file).split(b'\n').map_while(Result::ok) {
        if line.first() != Some(&b'{') {
            continue;
        }
        let text = String::from_utf8_lossy(&line);
        let is_gre = text.contains("\"greToClientEvent\"");
        let is_room = text.contains("\"matchGameRoomStateChangedEvent\"");
        if !is_gre && !is_room {
            continue;
        }
        let Ok(o) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        let ts = o["timestamp"]
            .as_str()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0u64);

        if is_room {
            let cfg = &o["matchGameRoomStateChangedEvent"]["gameRoomInfo"]["gameRoomConfig"];
            if let Some(match_id) = cfg["matchId"].as_str() {
                let seat = cur.as_ref().map(|t| t.seat()).unwrap_or(1);
                for p in cfg["reservedPlayers"].as_array().into_iter().flatten() {
                    if u(&p["systemSeatId"]) == seat
                        && let Some(ev) = p["eventId"].as_str()
                    {
                        events.insert(match_id.to_string(), ev.to_string());
                    }
                }
            }
            continue;
        }

        for m in o["greToClientEvent"]["greToClientMessages"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let seat = m["systemSeatIds"][0].as_u64().unwrap_or(1) as u32;
            match m["type"].as_str().unwrap_or("") {
                "GREMessageType_ConnectResp" => {
                    deck = m["connectResp"]["deckMessage"]["deckCards"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(u)
                        .collect();
                }
                "GREMessageType_MulliganReq" => {
                    if let Some(t) = cur.as_mut() {
                        t.mulligan_reqs += 1;
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
            let new_game = match (&cur, &known) {
                (None, _) => true,
                (Some(t), Some(id)) => !t.rec.id.is_empty() && &t.rec.id != id,
                (Some(t), None) => t.seat() != seat,
            };
            if new_game {
                // Eine vorige Partie ohne Ergebnis wird verworfen.
                let mut t = Tracker::new(seat, ts);
                t.rec.deck = std::mem::take(&mut deck);
                cur = Some(t);
            }
            if let (Some(t), Some(id)) = (cur.as_mut(), known) {
                t.rec.id = id;
            }
            let Some(t) = cur.as_mut() else { continue };
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
                    let done = cur.take().unwrap();
                    if !done.rec.id.is_empty() {
                        out.push(done.finish(won, &reason));
                    }
                }
            }
        }
    }
    for rec in &mut out {
        let match_id = rec.id.split('#').next().unwrap_or("");
        rec.event = events.get(match_id).cloned().unwrap_or_default();
    }
    out
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

/// Gespeicherte Partien laden, neue aus den Logs dazunehmen und speichern.
pub fn sync(store: &Path, logs: &[PathBuf]) -> Vec<GameRecord> {
    let mut games: Vec<GameRecord> = crate::paths::read_json(store).unwrap_or_default();
    let before = serde_json::to_vec(&games).unwrap_or_default();
    for log in logs {
        for rec in parse_log(log) {
            match games.iter_mut().find(|g| g.id == rec.id) {
                // Vollständigere Aufzeichnung gewinnt.
                Some(old) if (rec.complete, rec.turns.len()) > (old.complete, old.turns.len()) => {
                    *old = rec
                }
                Some(_) => {}
                None => games.push(rec),
            }
        }
    }
    games.sort_by_key(|g| std::cmp::Reverse(g.started_ms));
    if let Ok(json) = serde_json::to_vec(&games)
        && json != before
    {
        let _ = crate::paths::write_atomic(store, &json);
    }
    games
}

// ------------------------------------------------------------ Auswertung ----

use crate::carddb::CardDb;

const TYPE_INSTANT: u8 = 4;

/// Ein auffälliger Zug, z. B. „Land nicht gespielt“.
#[derive(Clone, Debug)]
pub struct Finding {
    pub game: usize,
    pub turn: u32,
    pub text: String,
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
    pub missed_lands: Vec<Finding>,
    pub unused_mana: Vec<Finding>,
    pub no_attack: Vec<Finding>,
    pub empty_board: Vec<Finding>,
    pub discards: u32,
    /// (Kartenname, Anzahl Partien)
    pub opp_cards: Vec<(String, usize)>,
}

/// Wertet die Partien aus. `games` ist neueste zuerst; Nummern zählen ab 1 in dieser Reihenfolge.
pub fn report(games: &[GameRecord], db: &CardDb) -> Report {
    let mut r = Report {
        games: games.len(),
        ..Default::default()
    };
    let mut turn_sum = 0;
    let mut opp: HashMap<String, usize> = HashMap::new();
    for (gi, g) in games.iter().enumerate() {
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
        r.conceded += (g.conceded() && g.won == Some(false)) as usize;
        r.discards += g.discards_me;
        turn_sum += g.last_turn();
        let mut names: HashSet<String> = HashSet::new();
        for grp in g.opp_cards.keys() {
            if let Some(c) = db.cards.get(grp)
                && !c.basic
            {
                names.insert(c.name.clone());
            }
        }
        for n in names {
            *opp.entry(n).or_insert(0) += 1;
        }

        // Teil-Mitschnitte kennen Hand und Feld nicht vollständig -> nur zur Bilanz.
        for t in g.turns.iter().filter(|t| t.mine && g.complete) {
            r.my_turns += 1;
            let hand: Vec<&crate::carddb::Card> =
                t.hand.iter().filter_map(|grp| db.cards.get(grp)).collect();
            let find = |text: String| Finding {
                game: gi + 1,
                turn: t.turn,
                text,
            };
            if !t.land_played
                && let Some(land) = hand.iter().find(|c| c.is_land())
            {
                r.missed_lands
                    .push(find(format!("{} lag auf der Hand", land.name)));
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
                    r.unused_mana.push(find(format!(
                        "{} Länder ungenutzt, {} ({}) auf der Hand",
                        t.untapped_lands, c.name, c.mana_cost
                    )));
                }
            }
            if t.turn >= 3 && t.creatures_at_start > 0 && !t.attacked {
                r.no_attack.push(find(format!(
                    "{} Kreatur(en) bereit, Gegner hatte {}",
                    t.creatures_at_start, t.creatures_opp
                )));
            }
            if (3..=6).contains(&t.turn) && t.creatures_me == 0 {
                r.empty_board
                    .push(find("keine eigene Kreatur auf dem Feld".into()));
            }
        }
    }
    if !games.is_empty() {
        r.avg_turns = turn_sum as f32 / games.len() as f32;
    }
    let mut opp: Vec<(String, usize)> = opp.into_iter().collect();
    opp.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    opp.truncate(15);
    r.opp_cards = opp;
    r
}

/// Farben eines Decks als Buchstaben (z. B. „UR“).
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
