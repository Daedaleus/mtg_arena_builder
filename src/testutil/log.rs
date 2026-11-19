//! Künstliche Arena-Lognachrichten (Spielzustände, Annotationen, Räume).

use crate::carddb::GrpId;
use serde_json::{Value, json};

/// Eine `greToClientEvent`-Zeile mit den angegebenen Nachrichten.
pub fn gre(ts: u64, messages: Vec<Value>) -> String {
    json!({
        "transactionId": "t",
        "timestamp": ts.to_string(),
        "greToClientEvent": { "greToClientMessages": messages }
    })
    .to_string()
}

/// Spielzustand (`GameStateMessage`) für Sitz `seat`.
pub fn state(seat: u32, state: Value) -> Value {
    json!({
        "type": "GREMessageType_GameStateMessage",
        "systemSeatIds": [seat],
        "gameStateMessage": state
    })
}

pub fn connect(seat: u32, deck: &[GrpId]) -> Value {
    json!({
        "type": "GREMessageType_ConnectResp",
        "systemSeatIds": [seat],
        "connectResp": { "deckMessage": { "deckCards": deck } }
    })
}

pub fn mulligan(seat: u32) -> Value {
    json!({ "type": "GREMessageType_MulliganReq", "systemSeatIds": [seat] })
}

/// Raum-Nachricht mit Event-Kennung für den Spieler auf `seat`.
pub fn room(match_id: &str, seat: u32, event: &str) -> String {
    json!({
        "matchGameRoomStateChangedEvent": {
            "gameRoomInfo": {
                "gameRoomConfig": {
                    "matchId": match_id,
                    "reservedPlayers": [ { "systemSeatId": seat, "eventId": event } ]
                }
            }
        }
    })
    .to_string()
}

/// Kartenobjekt im Spiel.
#[derive(Clone)]
pub struct Obj {
    pub id: u32,
    pub grp: GrpId,
    pub zone: u32,
    pub owner: u32,
    pub kind: &'static str,
    pub tapped: bool,
    pub power: i32,
    pub toughness: i32,
    pub attacking: bool,
    pub blocking: bool,
    pub sick: bool,
    pub keywords: Vec<u32>,
}

pub fn obj(id: u32, grp: GrpId, zone: u32, owner: u32) -> Obj {
    Obj {
        id,
        grp,
        zone,
        owner,
        kind: "",
        tapped: false,
        power: 0,
        toughness: 0,
        attacking: false,
        blocking: false,
        sick: false,
        keywords: Vec::new(),
    }
}

impl Obj {
    pub fn land(mut self) -> Self {
        self.kind = "CardType_Land";
        self
    }
    pub fn creature(mut self, power: i32, toughness: i32) -> Self {
        self.kind = "CardType_Creature";
        self.power = power;
        self.toughness = toughness;
        self
    }
    pub fn tapped(mut self) -> Self {
        self.tapped = true;
        self
    }
    pub fn attacking(mut self) -> Self {
        self.attacking = true;
        self
    }
    pub fn blocking(mut self) -> Self {
        self.blocking = true;
        self
    }
    pub fn sick(mut self) -> Self {
        self.sick = true;
        self
    }
    pub fn keywords(mut self, k: &[u32]) -> Self {
        self.keywords = k.to_vec();
        self
    }
    pub fn json(&self) -> Value {
        let types: Vec<&str> = if self.kind.is_empty() {
            Vec::new()
        } else {
            vec![self.kind]
        };
        let abilities: Vec<Value> = self
            .keywords
            .iter()
            .map(|k| json!({ "grpId": k }))
            .collect();
        let mut v = json!({
            "type": "GameObjectType_Card",
            "instanceId": self.id,
            "grpId": self.grp,
            "zoneId": self.zone,
            "ownerSeatId": self.owner,
            "controllerSeatId": self.owner,
            "cardTypes": types,
            "isTapped": self.tapped,
            "hasSummoningSickness": self.sick,
            "power": { "value": self.power },
            "toughness": { "value": self.toughness },
            "uniqueAbilities": abilities
        });
        if self.attacking {
            v["attackState"] = json!("AttackState_Attacking");
        }
        if self.blocking {
            v["blockState"] = json!("BlockState_Blocking");
        }
        v
    }
}

// Zonen der künstlichen Partie
pub const Z_BATTLEFIELD: u32 = 28;
pub const Z_STACK: u32 = 27;
pub const Z_HAND_1: u32 = 31;
pub const Z_HAND_2: u32 = 35;
pub const Z_GRAVE_2: u32 = 37;

/// Zonenliste: Spielfeld, Stapel, beide Hände und gegnerischer Friedhof.
pub fn zones(hand1: &[u32], hand2: &[u32], battlefield: &[u32]) -> Value {
    json!([
        { "zoneId": Z_BATTLEFIELD, "type": "ZoneType_Battlefield", "ownerSeatId": 0, "objectInstanceIds": battlefield },
        { "zoneId": Z_STACK, "type": "ZoneType_Stack", "ownerSeatId": 0, "objectInstanceIds": [] },
        { "zoneId": Z_HAND_1, "type": "ZoneType_Hand", "ownerSeatId": 1, "objectInstanceIds": hand1 },
        { "zoneId": Z_HAND_2, "type": "ZoneType_Hand", "ownerSeatId": 2, "objectInstanceIds": hand2 },
        { "zoneId": Z_GRAVE_2, "type": "ZoneType_Graveyard", "ownerSeatId": 2, "objectInstanceIds": [] }
    ])
}

pub fn turn_info(turn: u32, active: u32, phase: &str, step: &str) -> Value {
    json!({ "turnNumber": turn, "activePlayer": active, "phase": phase, "step": step })
}

pub fn players(life1: i32, life2: i32) -> Value {
    json!([
        { "systemSeatNumber": 1, "lifeTotal": life1, "teamId": 1 },
        { "systemSeatNumber": 2, "lifeTotal": life2, "teamId": 2 }
    ])
}

/// Zonenwechsel-Annotation (z. B. `PlayLand`, `CastSpell`, `Discard`).
pub fn zone_transfer(id: u64, instance: u32, category: &str) -> Value {
    json!({
        "id": id,
        "type": ["AnnotationType_ZoneTransfer"],
        "affectedIds": [instance],
        "details": [ { "key": "category", "valueString": [category] } ]
    })
}

pub fn damage(id: u64, source: u32, target_seat: u32, amount: u32) -> Value {
    json!({
        "id": id,
        "type": ["AnnotationType_DamageDealt"],
        "affectorId": source,
        "affectedIds": [target_seat],
        "details": [ { "key": "damage", "valueInt32": [amount] } ]
    })
}

pub fn game_info(match_id: &str, game: u32) -> Value {
    json!({ "matchID": match_id, "gameNumber": game, "stage": "GameStage_Play" })
}

/// Spielende mit Sieger-Team `winner`.
pub fn game_over(match_id: &str, game: u32, winner: u32, reason: &str) -> Value {
    json!({
        "matchID": match_id,
        "gameNumber": game,
        "stage": "GameStage_GameOver",
        "results": [ {
            "scope": "MatchScope_Game",
            "result": "ResultType_WinLoss",
            "winningTeamId": winner,
            "reason": reason
        } ]
    })
}
