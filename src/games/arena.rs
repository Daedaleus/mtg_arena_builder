//! Values from the Arena log (messages of the game engine, "GRE") in one
//! place: the turn phase as an enum, everything else as named constants.
//! So no protocol value is scattered through the code as a string.

/// Phase of a turn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    /// No phase yet or an unknown one.
    #[default]
    Unknown,
    Beginning,
    Main1,
    Combat,
    Main2,
    Ending,
}

impl Phase {
    /// Phase from the log value, e.g. "Phase_Main1".
    pub fn from_arena(value: &str) -> Phase {
        match value {
            "Phase_Beginning" => Phase::Beginning,
            "Phase_Main1" => Phase::Main1,
            "Phase_Combat" => Phase::Combat,
            "Phase_Main2" => Phase::Main2,
            "Phase_Ending" => Phase::Ending,
            _ => Phase::Unknown,
        }
    }
}

/// Steps within a phase (only those the app needs).
pub mod step {
    pub const BEGIN_COMBAT: &str = "Step_BeginCombat";
    pub const DECLARE_ATTACK: &str = "Step_DeclareAttack";
}

/// Zones (where a card is).
pub mod zone {
    pub const BATTLEFIELD: &str = "ZoneType_Battlefield";
    pub const HAND: &str = "ZoneType_Hand";
    pub const STACK: &str = "ZoneType_Stack";
    pub const GRAVEYARD: &str = "ZoneType_Graveyard";
    pub const EXILE: &str = "ZoneType_Exile";
    pub const REVEALED: &str = "ZoneType_Revealed";
}

/// Kinds of messages and game states.
pub mod message {
    pub const CONNECT_RESP: &str = "GREMessageType_ConnectResp";
    pub const MULLIGAN_REQ: &str = "GREMessageType_MulliganReq";
    pub const FULL_STATE: &str = "GameStateType_Full";
    pub const GAME_OVER: &str = "GameStage_GameOver";
    pub const SCOPE_GAME: &str = "MatchScope_Game";
    pub const RESULT_WIN_LOSS: &str = "ResultType_WinLoss";
    pub const OBJECT_CARD: &str = "GameObjectType_Card";
}

/// Card types and combat states of a game object.
pub mod card_state {
    pub const LAND: &str = "CardType_Land";
    pub const CREATURE: &str = "CardType_Creature";
    pub const ATTACKING: [&str; 2] = ["AttackState_Attacking", "AttackState_Declared"];
    pub const BLOCKING: [&str; 2] = ["BlockState_Blocking", "BlockState_Declared"];
}

/// Arena events (game modes) of a game.
pub mod event {
    pub const LADDER: &str = "Ladder";
    pub const LADDER_BO3: &str = "Traditional_Ladder";
    pub const PLAY: &str = "Play";
    pub const PLAY_BO3: &str = "Traditional_Play";
    pub const STARTER_DECKS: &str = "DualColorPrecons";
    /// Prefixes of events with a fixed deck.
    pub const WELCOME_DUELS_PREFIX: &str = "WelcomeDeckDuels";
    pub const COLOR_CHALLENGE_PREFIX: &str = "ColorChallenge";

    /// Does the event fix the deck?
    pub fn fixed_deck(event: &str) -> bool {
        event.starts_with(WELCOME_DUELS_PREFIX)
            || event.starts_with(COLOR_CHALLENGE_PREFIX)
            || event == STARTER_DECKS
    }
}

/// Annotations and their categories.
pub mod annotation {
    pub const DAMAGE_DEALT: &str = "AnnotationType_DamageDealt";
    pub const ZONE_TRANSFER: &str = "AnnotationType_ZoneTransfer";
    pub const CAST_SPELL: &str = "CastSpell";
    pub const PLAY_LAND: &str = "PlayLand";
    pub const DISCARD: &str = "Discard";
}
