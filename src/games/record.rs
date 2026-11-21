//! Stored data of a game: turns, result and rank.

use super::*;

/// Life at the start of a game.
pub const START_LIFE: i32 = 20;

/// State at the end of a turn.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TurnStat {
    pub turn: u32,
    /// My turn?
    pub mine: bool,
    pub life_me: i32,
    pub life_opp: i32,
    pub lands_me: u32,
    pub lands_opp: u32,
    pub creatures_me: u32,
    pub creatures_opp: u32,
    /// Own untapped lands at the end of the turn.
    pub untapped_lands: u32,
    /// Own hand cards (grpIds) at the end of the turn.
    pub hand: Vec<GrpId>,
    pub land_played: bool,
    /// Spells cast by the active player.
    pub spells: u32,
    /// Did the active player attack?
    pub attacked: bool,
    /// Own creatures that were already on the battlefield at the start of this turn.
    pub creatures_at_start: u32,
    /// My turn: ready creatures (grpIds) that could have attacked safely but
    /// didn't.
    pub safe_idle: Vec<GrpId>,
    /// Opponent's turn: did I block?
    pub blocked: bool,
    /// Opponent's turn: my untapped creatures before combat.
    pub my_untapped_creatures: u32,
    /// Cards cast by me this turn (grpIds), also in the opponent's turn.
    pub cast: Vec<GrpId>,
    /// My lands in play at the end of the turn (grpIds, for the colors).
    pub lands: Vec<GrpId>,
    /// The opponent's creatures at the end of the turn: (grpId, power).
    pub opp_creatures: Vec<(GrpId, i32)>,
    /// Own permanents except lands at the end of the turn (for the legend
    /// rule).
    #[serde(default)]
    pub board: Vec<GrpId>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct GameRecord {
    /// `matchId#game number`
    pub id: String,
    pub started_ms: u64,
    /// Arena event, e.g. "Ladder" (ranked) or "Play".
    pub event: String,
    pub my_seat: u32,
    /// Did I start? `None` if the start isn't in the log.
    pub on_play: Option<bool>,
    pub mulligans: u32,
    /// `Some(true)` = won.
    pub won: Option<bool>,
    /// Why the game ended (in `games.json` as the Arena value).
    pub reason: EndReason,
    /// Was the game recorded from the start?
    pub complete: bool,
    pub turns: Vec<TurnStat>,
    /// Own deck (grpIds, repeated = copies).
    pub deck: Vec<GrpId>,
    /// Cards seen from the opponent (grpId -> copies).
    pub opp_cards: HashMap<GrpId, u32>,
    pub discards_me: u32,
    /// Kept opening hand (grpIds).
    pub opening_hand: Vec<GrpId>,
    /// Damage to me per source (grpId -> damage).
    pub damage_taken: HashMap<GrpId, u32>,
    /// Damage of my cards to the opponent (grpId -> damage).
    pub damage_dealt: HashMap<GrpId, u32>,
    /// Constructed rank that Arena reported after this game.
    pub rank_after: Option<Rank>,
    /// Version of the analysis that created the game (see `PARSER_VERSION`);
    /// 0 = unknown (older file).
    pub parser: u32,
}

/// Why a game ended. `games.json` holds the Arena value
/// (e.g. "ResultReason_Concede"), so older files stay valid.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum EndReason {
    #[default]
    Unknown,
    /// Decided normally (life at 0, empty library …).
    Game,
    Concede,
    Timeout,
    /// Other Arena value, kept unchanged.
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

/// Constructed rank according to Arena (`RankGetCombinedRankInfo`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Rank {
    pub season: u32,
    /// Class like "Bronze", "Silver" … "Mythic".
    pub class: String,
    /// Tier within the class: 4 (entry) to 1.
    pub level: u32,
    /// Progress within the tier.
    pub step: u32,
    pub won: u32,
    pub lost: u32,
    /// Limited rank (draft, sealed) – same structure as above.
    pub limited_class: String,
    pub limited_level: u32,
    pub limited_step: u32,
}

impl Rank {
    /// Reads the answer of `RankGetCombinedRankInfo`. Arena leaves out the
    /// class as long as it has the default value (Bronze).
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
    /// Cards I drew or saw in this game (opening hand, hand at the end of a
    /// turn, or cast).
    pub fn seen_cards(&self) -> HashSet<GrpId> {
        self.opening_hand
            .iter()
            .chain(self.turns.iter().flat_map(|t| t.hand.iter().chain(&t.cast)))
            .copied()
            .collect()
    }

    pub fn cast_cards(&self) -> HashSet<GrpId> {
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
