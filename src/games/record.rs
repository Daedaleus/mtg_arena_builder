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
    /// Meine Länder im Spiel am Zugende (grpIds, für die Farben).
    pub lands: Vec<u32>,
    /// Kreaturen des Gegners am Zugende: (grpId, Kraft).
    pub opp_creatures: Vec<(u32, i32)>,
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
    /// Constructed-Rang, den Arena nach dieser Partie gemeldet hat.
    pub rank_after: Option<Rank>,
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
        self.reason.contains("Concede")
    }
}
