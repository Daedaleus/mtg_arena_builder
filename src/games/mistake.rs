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

    pub fn title(self) -> &'static str {
        match self {
            Mistake::SkippedAttack => "Gefahrlosen Angriff ausgelassen",
            Mistake::RiskyKeep => "Riskante Starthand behalten",
            Mistake::MissedLand => "Land nicht gespielt",
            Mistake::UnusedMana => "Mana ungenutzt",
            Mistake::BetterTurn => "Besserer Zug möglich",
            Mistake::HeldCard => "Karte lange nicht gespielt",
            Mistake::NoBlock => "Bei wenig Leben nicht geblockt",
            Mistake::EmptyBoard => "Keine Kreatur auf dem Feld",
            Mistake::EarlyConcede => "Früh aufgegeben",
        }
    }

    /// Was man stattdessen tun sollte.
    pub fn advice(self) -> &'static str {
        match self {
            Mistake::SkippedAttack => {
                "Wenn keine gegnerische Kreatur deinen Angreifer blocken und dabei töten kann, greif an. \
                         Schlimmstenfalls opfert der Gegner einen Blocker, sonst verliert er Leben. \
                         (Fliegend, Reichweite, Todesberührung und Bedrohlich sind berücksichtigt, Zaubertricks des Gegners nicht.)"
            }
            Mistake::RiskyKeep => {
                "Behalte nur Starthände mit 2–4 Ländern. Mit 0–1 Land fehlt dir das Mana, mit 5 oder mehr fehlen die Zauber. \
                         Ein Mulligan auf 6 Karten ist meist besser als so eine Hand."
            }
            Mistake::MissedLand => {
                "Lege jeden Zug ein Land, wenn du eins hast – auch wenn du es gerade nicht brauchst. Später fehlt es sonst."
            }
            Mistake::UnusedMana => {
                "Nutze dein Mana in jedem Zug. Eine Kreatur auf der Hand macht keinen Schaden – spiel sie, sobald du kannst. \
                         (Die Farben prüft die App nicht; manchmal fehlte also die passende Farbe.)"
            }
            Mistake::BetterTurn => {
                "Der Zugvorschlag des Begleitfensters hätte in diesen Zügen mehr aus deinem Mana gemacht – \
                         mit Blick auf Farben und gegnerische Kreaturen. Oft lohnen zwei günstige Karten statt einer teuren \
                         oder Entfernung vor der nächsten Kreatur."
            }
            Mistake::HeldCard => {
                "Diese Karten lagen mehrere Züge bezahlbar auf deiner Hand. Spiel sie früher – \
                         oder tausch sie im Deck gegen etwas, das du wirklich spielst. (Spontanzauber zählen hier nicht mit.)"
            }
            Mistake::NoBlock => {
                "Bei wenig Leben lohnt es sich zu blocken, auch wenn deine Kreatur dabei stirbt. \
                         Dann sind deine Lebenspunkte wichtiger als die Kreatur."
            }
            Mistake::EmptyBoard => {
                "In Zug 3–6 lag keine eigene Kreatur auf dem Feld – dann kannst du keinen Druck machen und nicht blocken. \
                         Spiel früh günstige Kreaturen; ein Deck braucht genug davon für die Züge 1–3."
            }
            Mistake::EarlyConcede => {
                "Gerade auf niedrigen Rängen machen Gegner oft Fehler, und Partien drehen sich. \
                         Spiel weiter, solange du noch Leben hast."
            }
        }
    }
}
