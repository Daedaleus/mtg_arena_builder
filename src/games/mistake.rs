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
            Mistake::SkippedAttack => tr!("games_mistake.gefahrlosen_angriff_ausgelassen"),
            Mistake::RiskyKeep => tr!("games_mistake.riskante_starthand_behalten"),
            Mistake::MissedLand => tr!("games_mistake.land_nicht_gespielt"),
            Mistake::UnusedMana => tr!("games_mistake.mana_ungenutzt"),
            Mistake::BetterTurn => tr!("games_mistake.besserer_zug_moeglich"),
            Mistake::HeldCard => tr!("games_mistake.karte_lange_nicht_gespielt"),
            Mistake::NoBlock => tr!("games_mistake.bei_wenig_leben_nicht"),
            Mistake::EmptyBoard => tr!("games_mistake.keine_kreatur_feld"),
            Mistake::EarlyConcede => tr!("games_mistake.frueh_aufgegeben"),
        }
    }

    /// Was man stattdessen tun sollte.
    pub fn advice(self) -> String {
        match self {
            Mistake::SkippedAttack => {
                tr!("games_mistake.wenn_keine_gegnerische_kreatur")
            }
            Mistake::RiskyKeep => {
                tr!("games_mistake.behalte_nur_starthaende_2")
            }
            Mistake::MissedLand => {
                tr!("games_mistake.lege_jeden_zug_land")
            }
            Mistake::UnusedMana => {
                tr!("games_mistake.nutze_mana_jedem_zug")
            }
            Mistake::BetterTurn => {
                tr!("games_mistake.zugvorschlag_begleitfensters_haette_diesen")
            }
            Mistake::HeldCard => {
                tr!("games_mistake.diese_karten_lagen_mehrere")
            }
            Mistake::NoBlock => {
                tr!("games_mistake.bei_wenig_leben_lohnt")
            }
            Mistake::EmptyBoard => {
                tr!("games_mistake.zug_3_6_lag")
            }
            Mistake::EarlyConcede => {
                tr!("games_mistake.gerade_niedrigen_raengen_machen")
            }
        }
    }
}
