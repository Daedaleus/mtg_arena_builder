//! Gespeicherte Daten einer Partie: Züge, Ergebnis und Rang.

use super::*;

/// Lebenspunkte zu Beginn einer Partie.
pub const START_LIFE: i32 = 20;

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
    pub hand: Vec<GrpId>,
    pub land_played: bool,
    /// Vom aktiven Spieler gewirkte Zauber.
    pub spells: u32,
    /// Hat der aktive Spieler angegriffen?
    pub attacked: bool,
    /// Eigene Kreaturen, die zu Beginn dieses Zuges schon auf dem Feld waren.
    pub creatures_at_start: u32,
    /// Mein Zug: einsatzbereite Kreaturen (grpIds), die gefahrlos hätten
    /// angreifen können, es aber nicht taten.
    pub safe_idle: Vec<GrpId>,
    /// Gegnerzug: habe ich geblockt?
    pub blocked: bool,
    /// Gegnerzug: meine ungetappten Kreaturen vor dem Kampf.
    pub my_untapped_creatures: u32,
    /// Von mir in diesem Zug gewirkte Karten (grpIds), auch im Gegnerzug.
    pub cast: Vec<GrpId>,
    /// Meine Länder im Spiel am Zugende (grpIds, für die Farben).
    pub lands: Vec<GrpId>,
    /// Kreaturen des Gegners am Zugende: (grpId, Kraft).
    pub opp_creatures: Vec<(GrpId, i32)>,
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
    /// Warum die Partie endete (in `games.json` als Arena-Wert).
    pub reason: EndReason,
    /// Wurde die Partie von Anfang an mitgeschrieben?
    pub complete: bool,
    pub turns: Vec<TurnStat>,
    /// Eigenes Deck (grpIds, Mehrfachnennung = Anzahl).
    pub deck: Vec<GrpId>,
    /// Vom Gegner gesehene Karten (grpId -> Anzahl Exemplare).
    pub opp_cards: HashMap<GrpId, u32>,
    pub discards_me: u32,
    /// Behaltene Starthand (grpIds).
    pub opening_hand: Vec<GrpId>,
    /// Schaden an mir je Quelle (grpId -> Schaden).
    pub damage_taken: HashMap<GrpId, u32>,
    /// Schaden meiner Karten am Gegner (grpId -> Schaden).
    pub damage_dealt: HashMap<GrpId, u32>,
    /// Constructed-Rang, den Arena nach dieser Partie gemeldet hat.
    pub rank_after: Option<Rank>,
    /// Version der Auswertung, mit der die Partie erstellt wurde (siehe
    /// `PARSER_VERSION`); 0 = unbekannt (ältere Datei).
    pub parser: u32,
}

/// Warum eine Partie endete. In `games.json` steht der Arena-Wert
/// (z. B. „ResultReason_Concede“), damit ältere Dateien gültig bleiben.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum EndReason {
    #[default]
    Unknown,
    /// Regulär entschieden (Leben auf 0, Bibliothek leer …).
    Game,
    Concede,
    Timeout,
    /// Anderer Arena-Wert, unverändert aufbewahrt.
    Other(String),
}

impl EndReason {
    pub fn from_arena(value: &str) -> EndReason {
        match value {
            "" => EndReason::Unknown,
            "ResultReason_Game" => EndReason::Game,
            "ResultReason_Concede" => EndReason::Concede,
            "ResultReason_Timeout" => EndReason::Timeout,
            other => EndReason::Other(other.to_string()),
        }
    }

    pub fn to_arena(&self) -> &str {
        match self {
            EndReason::Unknown => "",
            EndReason::Game => "ResultReason_Game",
            EndReason::Concede => "ResultReason_Concede",
            EndReason::Timeout => "ResultReason_Timeout",
            EndReason::Other(v) => v,
        }
    }
}

impl From<String> for EndReason {
    fn from(value: String) -> EndReason {
        EndReason::from_arena(&value)
    }
}

impl From<EndReason> for String {
    fn from(r: EndReason) -> String {
        r.to_arena().to_string()
    }
}

/// Constructed-Rang laut Arena (`RankGetCombinedRankInfo`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Rank {
    pub season: u32,
    /// Klasse wie „Bronze“, „Silver“ … „Mythic“.
    pub class: String,
    /// Stufe innerhalb der Klasse: 4 (Einstieg) bis 1.
    pub level: u32,
    /// Fortschritt innerhalb der Stufe.
    pub step: u32,
    pub won: u32,
    pub lost: u32,
    /// Limited-Rang (Draft, Sealed) – gleiche Einteilung wie oben.
    pub limited_class: String,
    pub limited_level: u32,
    pub limited_step: u32,
}

impl Rank {
    /// Liest die Antwort von `RankGetCombinedRankInfo`. Arena lässt die Klasse
    /// weg, solange sie den Standardwert hat (Bronze).
    pub fn parse(text: &str) -> Option<Rank> {
        let v: Value = serde_json::from_str(text.trim()).ok()?;
        let season = v["constructedSeasonOrdinal"].as_u64()? as u32;
        Some(Rank {
            season,
            class: v["constructedClass"]
                .as_str()
                .unwrap_or("Bronze")
                .to_string(),
            level: v["constructedLevel"].as_u64().unwrap_or(4) as u32,
            step: v["constructedStep"].as_u64().unwrap_or(0) as u32,
            won: v["constructedMatchesWon"].as_u64().unwrap_or(0) as u32,
            lost: v["constructedMatchesLost"].as_u64().unwrap_or(0) as u32,
            limited_class: v["limitedClass"].as_str().unwrap_or("Bronze").to_string(),
            limited_level: v["limitedLevel"].as_u64().unwrap_or(4) as u32,
            limited_step: v["limitedStep"].as_u64().unwrap_or(0) as u32,
        })
    }
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
            .unwrap_or((START_LIFE, START_LIFE))
    }

    pub fn conceded(&self) -> bool {
        self.reason == EndReason::Concede
    }
}
