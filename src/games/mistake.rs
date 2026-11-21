//! Kinds of mistakes in the game analysis with title and advice.

/// A kind of mistake that the analysis finds in the games.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mistake {
    /// Safe attack not made.
    SkippedAttack,
    /// Opening hand with too few or too many lands kept.
    RiskyKeep,
    /// Land in hand, but none played.
    MissedLand,
    /// Mana free and an affordable card in hand (older recordings).
    UnusedMana,
    /// The turn suggestion would have used clearly more mana.
    BetterTurn,
    /// Card was affordable in hand for a long time.
    HeldCard,
    /// Took a lot of damage at low life without blocking.
    NoBlock,
    /// No own creature on the battlefield in turns 3–6.
    EmptyBoard,
    /// Conceded with enough life left.
    EarlyConcede,
}

impl Mistake {
    /// All kinds of mistakes; with equally many findings this order applies.
    pub const ALL: [Mistake; 9] = [
        Mistake::SkippedAttack,
        Mistake::RiskyKeep,
        Mistake::MissedLand,
        Mistake::UnusedMana,
        Mistake::BetterTurn,
        Mistake::HeldCard,
        Mistake::NoBlock,
        Mistake::EmptyBoard,
        Mistake::EarlyConcede,
    ];

    pub fn title(self) -> String {
        match self {
            Mistake::SkippedAttack => tr!("games_mistake.safe_attack_skipped"),
            Mistake::RiskyKeep => tr!("games_mistake.risky_opening_hand_kept"),
            Mistake::MissedLand => tr!("games_mistake.land_played"),
            Mistake::UnusedMana => tr!("games_mistake.mana_unused"),
            Mistake::BetterTurn => tr!("games_mistake.better_play_possible"),
            Mistake::HeldCard => tr!("games_mistake.card_played_long_time"),
            Mistake::NoBlock => tr!("games_mistake.did_block_low_life"),
            Mistake::EmptyBoard => tr!("games_mistake.no_creature_battlefield"),
            Mistake::EarlyConcede => tr!("games_mistake.conceded_early"),
        }
    }

    /// What to do instead.
    pub fn advice(self) -> String {
        match self {
            Mistake::SkippedAttack => {
                tr!("games_mistake.no_opposing_creature_block")
            }
            Mistake::RiskyKeep => {
                tr!("games_mistake.only_keep_opening_hands")
            }
            Mistake::MissedLand => {
                tr!("games_mistake.play_land_every_turn")
            }
            Mistake::UnusedMana => {
                tr!("games_mistake.use_mana_every_turn")
            }
            Mistake::BetterTurn => {
                tr!("games_mistake.companions_turn_suggestion_would")
            }
            Mistake::HeldCard => {
                tr!("games_mistake.these_cards_castable_hand")
            }
            Mistake::NoBlock => {
                tr!("games_mistake.low_life_pays_off")
            }
            Mistake::EmptyBoard => {
                tr!("games_mistake.turns_3_6_had")
            }
            Mistake::EarlyConcede => {
                tr!("games_mistake.especially_low_ranks_opponents")
            }
        }
    }
}
