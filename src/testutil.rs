//! Hilfen für Tests: kleine Kartendatenbanken und künstliche Arena-Lognachrichten.

use serde_json::{Value, json};

use crate::carddb::{
    Card, CardDb, ROLE_COUNTER, ROLE_DRAW, ROLE_REMOVAL, ROLE_WIPE, TYPE_CREATURE, TYPE_INSTANT,
    TYPE_LAND, TYPE_SORCERY,
};

/// Bitmaske der Farben, die in Manakosten wie `{1}{U}{U}` vorkommen.
fn colors_of(cost: &str) -> u8 {
    "WUBRG"
        .chars()
        .enumerate()
        .filter(|(_, c)| cost.contains(&format!("{{{c}}}")))
        .fold(0, |m, (i, _)| m | (1 << i))
}

/// Baukasten für eine Karte.
pub struct CardBuilder(Card);

pub fn card(grp: u32, name: &str) -> CardBuilder {
    CardBuilder(Card {
        grp_id: grp,
        name: name.into(),
        rarity: 2,
        set: "TST".into(),
        collector: grp.to_string(),
        is_primary: true,
        regular: true,
        ..Default::default()
    })
}

impl CardBuilder {
    pub fn cost(mut self, cost: &str) -> Self {
        self.0.mana_cost = cost.into();
        let (generic, colored) = self.0.pips();
        self.0.cmc = generic + colored.iter().sum::<u32>();
        self.0.colors |= colors_of(cost);
        self
    }
    pub fn creature(mut self, power: i32, toughness: i32) -> Self {
        self.0.types.push(TYPE_CREATURE);
        self.0.power = power.to_string();
        self.0.toughness = toughness.to_string();
        self
    }
    pub fn instant(mut self) -> Self {
        self.0.types.push(TYPE_INSTANT);
        self
    }
    pub fn sorcery(mut self) -> Self {
        self.0.types.push(TYPE_SORCERY);
        self
    }
    /// Land, das die Farben aus `colors` (z. B. „UG“) liefert.
    pub fn land(mut self, colors: &str) -> Self {
        self.0.types.push(TYPE_LAND);
        let symbols: String = colors.chars().map(|c| format!("{{{c}}}")).collect();
        self.0.colors = colors_of(&symbols);
        self
    }
    pub fn basic(mut self) -> Self {
        self.0.basic = true;
        self.0.rarity = 1;
        self
    }
    pub fn rarity(mut self, r: u8) -> Self {
        self.0.rarity = r;
        self
    }
    pub fn removal(mut self) -> Self {
        self.0.roles |= ROLE_REMOVAL;
        self
    }
    pub fn counter(mut self) -> Self {
        self.0.roles |= ROLE_COUNTER;
        self
    }
    pub fn draw(mut self) -> Self {
        self.0.roles |= ROLE_DRAW;
        self
    }
    pub fn wipe(mut self) -> Self {
        self.0.roles |= ROLE_WIPE | ROLE_REMOVAL;
        self
    }
    pub fn burn(mut self, n: u32) -> Self {
        self.0.burn = n;
        self
    }
    pub fn tapped(mut self) -> Self {
        self.0.enters_tapped = true;
        self
    }
    /// Manafähigkeit eines Nichtlands, z. B. „G“ (leer = farblos).
    pub fn mana(mut self, colors: &str) -> Self {
        let symbols: String = colors.chars().map(|c| format!("{{{c}}}")).collect();
        self.0.mana = Some(colors_of(&symbols));
        self
    }
    pub fn abilities(mut self, ids: &[u32]) -> Self {
        self.0.abilities = ids.to_vec();
        self
    }
    pub fn subtypes(mut self, ids: &[u32]) -> Self {
        self.0.subtypes = ids.to_vec();
        self
    }
    pub fn set(mut self, set: &str, collector: &str) -> Self {
        self.0.set = set.into();
        self.0.collector = collector.into();
        self
    }
    pub fn de(mut self, name: &str) -> Self {
        self.0.name_de = Some(name.into());
        self
    }
    pub fn build(self) -> Card {
        self.0
    }
}

// grpIds der Standard-Testkarten
pub const PLAINS: u32 = 1;
pub const ISLAND: u32 = 2;
pub const SWAMP: u32 = 3;
pub const MOUNTAIN: u32 = 4;
pub const FOREST: u32 = 5;
pub const DUAL_UG: u32 = 6;
pub const TAPLAND_UR: u32 = 7;
pub const BEAR: u32 = 10;
pub const OGRE: u32 = 11;
pub const ELF: u32 = 12;
pub const GIANT: u32 = 13;
pub const SPIRIT: u32 = 14;
pub const SHOCK: u32 = 20;
pub const MURDER: u32 = 21;
pub const COUNTER: u32 = 22;
pub const DIVINATION: u32 = 23;
pub const WRATH: u32 = 24;
pub const SPIKE: u32 = 25;
pub const GROWTH: u32 = 26;
pub const DRAGON: u32 = 30;
pub const ANGEL: u32 = 31;

/// Kleine, aber vielseitige Kartenauswahl für Tests.
pub fn std_cards() -> Vec<Card> {
    vec![
        card(PLAINS, "Plains").land("W").basic().build(),
        card(ISLAND, "Island").land("U").basic().build(),
        card(SWAMP, "Swamp").land("B").basic().build(),
        card(MOUNTAIN, "Mountain").land("R").basic().build(),
        card(FOREST, "Forest").land("G").basic().build(),
        card(DUAL_UG, "Hedge Maze").land("UG").rarity(3).build(),
        card(TAPLAND_UR, "Riverglide Path")
            .land("UR")
            .tapped()
            .build(),
        card(BEAR, "Grizzly Bears")
            .cost("{1}{G}")
            .creature(2, 2)
            .build(),
        card(OGRE, "Hill Ogre")
            .cost("{2}{R}")
            .creature(3, 3)
            .build(),
        card(ELF, "Llanowar Elves")
            .cost("{G}")
            .creature(1, 1)
            .mana("G")
            .build(),
        card(GIANT, "Stone Giant")
            .cost("{4}{G}{G}")
            .creature(6, 6)
            .rarity(3)
            .build(),
        card(SPIRIT, "Storm Spirit")
            .cost("{1}{U}{U}")
            .creature(2, 3)
            .abilities(&[8])
            .build(),
        card(SHOCK, "Shock")
            .cost("{R}")
            .instant()
            .removal()
            .burn(2)
            .build(),
        card(MURDER, "Murder")
            .cost("{1}{B}{B}")
            .instant()
            .removal()
            .build(),
        card(COUNTER, "Cancel")
            .cost("{1}{U}{U}")
            .instant()
            .counter()
            .build(),
        card(DIVINATION, "Divination")
            .cost("{2}{U}")
            .sorcery()
            .draw()
            .build(),
        card(WRATH, "Day of Judgment")
            .cost("{2}{W}{W}")
            .sorcery()
            .wipe()
            .rarity(4)
            .build(),
        card(SPIKE, "Lava Spike")
            .cost("{R}")
            .sorcery()
            .burn(3)
            .removal()
            .build(),
        card(GROWTH, "Giant Growth").cost("{G}").instant().build(),
        card(DRAGON, "Shivan Dragon")
            .cost("{4}{R}{R}")
            .creature(5, 5)
            .abilities(&[8])
            .rarity(5)
            .build(),
        card(ANGEL, "Serra Angel")
            .cost("{3}{W}{W}")
            .creature(4, 4)
            .abilities(&[8])
            .rarity(4)
            .build(),
    ]
}

pub fn std_db() -> CardDb {
    CardDb::from_cards(std_cards(), "test")
}

// ------------------------------------------------- künstliche Logzeilen ----

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

pub fn connect(seat: u32, deck: &[u32]) -> Value {
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
    pub grp: u32,
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

pub fn obj(id: u32, grp: u32, zone: u32, owner: u32) -> Obj {
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

/// Temporäres Verzeichnis, das am Ende des Tests gelöscht wird.
pub struct TempDir(pub std::path::PathBuf);

impl TempDir {
    pub fn new(tag: &str) -> TempDir {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "mtgab_test_{tag}_{}_{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }

    pub fn path(&self, name: &str) -> std::path::PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// ------------------------------------------------- magic.gg-Seiten ----

/// Ausschnitt der Übersichtsseite (Aufbau wie auf magic.gg).
pub const GG_INDEX_HTML: &str = r#"<ul>
<li class="css-3pfjz"><a href="/decklists/arena-championship-13-standard-decklists-a-k" target="_self" rel="" class="css-Q01Pa"><div class="css-2es8F"><div class="css-vT_kr" style="padding-top:56.25%;"></div></div> <div class="css-1wd27"><div class="css-1JXy9">
      Arena Championship 13 Standard Decklists A-K
    </div> <div class="css-xkkQe"><span>October 24, 2026</span> <!----></div> <!----></div></a></li>
<li><a href="/decklists/champions-cup-final-yokohama-october-2026-top-16-modern-decklists" target="_self"><div>
      Champions Cup Final Yokohama Top 16 Modern Decklists
    </div> <div><span>October 20, 2026</span></div></a></li>
<li><a href="/decklists/traditional-standard-ranked-decklists-november-9-2026" target="_self"><div>
      Traditional Standard Ranked Decklists: November 9, 2026
    </div> <div><span>November 9, 2026</span></div></a></li>
<li><a href="/decklists/arena-championship-13-standard-decklists-a-k" target="_self"><div>Doppelt</div></a></li>
<li><a href="/news/mtg-arena-premier-play-policy">Keine Deckliste</a></li>
</ul>"#;

/// Ausschnitt einer Event-Seite mit zwei Decks und einem leeren Eintrag.
pub const GG_EVENT_HTML: &str = r#"<div>
<deck-list deck-title="Erika Muster" subtitle="1st Place" event-date="October 24, 2026" event-name="Arena Championship 13" format="Standard">
<main-deck>
20 Mountain
4 Shock
4 Hill Ogre
</main-deck>
<side-board>
2 Lava Spike
</side-board>
<companion-card>
</companion-card>
</deck-list>
<deck-list deck-title="Max O&#39;Neil" subtitle="" event-date="October 24, 2026" event-name="Arena Championship 13" format="Standard">
<main-deck>
24 Forest
4 Grizzly Bears
</main-deck>
<side-board>
</side-board>
</deck-list>
<deck-list deck-title="Leer" format="Standard">
<main-deck>
</main-deck>
</deck-list>
</div>"#;
