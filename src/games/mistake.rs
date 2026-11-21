//! Fehlerarten der Partie-Auswertung mit Titel und Rat.

/// Eine Art von Fehler, die die Auswertung in den Partien findet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mistake {
    /// Gefahrlosen Angriff nicht gemacht.
    SkippedAttack,
    /// Starthand mit zu wenigen oder zu vielen Ländern behalten.
    RiskyKeep,
    /// Land auf der Hand, aber keins gespielt.
    MissedLand,
    /// Mana frei und eine bezahlbare Karte auf der Hand (ältere Aufzeichnungen).
    UnusedMana,
    /// Der Zugvorschlag hätte deutlich mehr Mana genutzt.
    BetterTurn,
    /// Karte lag lange bezahlbar auf der Hand.
    HeldCard,
    /// Bei wenig Leben viel Schaden genommen, ohne zu blocken.
    NoBlock,
    /// In Zug 3–6 keine eigene Kreatur auf dem Feld.
    EmptyBoard,
    /// Mit noch genug Leben aufgegeben.
    EarlyConcede,
}

impl Mistake {
    /// Alle Fehlerarten; bei gleich vielen Fundstellen gilt diese Reihenfolge.
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

    /// Was man stattdessen tun sollte.
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
