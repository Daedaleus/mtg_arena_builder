//! Teilzustände der App, nach Bereich gebündelt. `App` hält davon je einen;
//! so bleibt sichtbar, welche Felder zusammengehören.

use super::*;

/// „Meine Partien“: Partien, Filter, Auswahl und zwischengespeicherte Auswertung.
#[derive(Default)]
pub(super) struct GamesState {
    pub list: Vec<GameRecord>,
    pub loading: bool,
    /// Wann zuletzt eingelesen (für das automatische Nachladen).
    pub scanned: Option<std::time::Instant>,
    /// In Arena sind „Detaillierte Logs“ aus.
    pub logs_disabled: bool,
    /// Filter nach Arena-Event (`None` = alle).
    pub event: Option<String>,
    pub sel: Option<usize>,
    /// Unterseite: 0 = Übersicht, 1 = Partienliste, 2 = Verlauf.
    pub view: u8,
    /// Seite der Partienliste (wird beim Zeichnen geändert).
    pub page: std::cell::Cell<usize>,
    pub cache: Option<GamesAnalysis>,
}

/// „Offizielle Events“ (magic.gg).
#[derive(Default)]
pub(super) struct GgState {
    pub events: Vec<GgEvent>,
    /// Format, für das `events` geladen sind.
    pub format: Option<usize>,
    /// Decks je Event (Slug) mit ihren Kennungen.
    pub decks: HashMap<String, Vec<(u32, GgDeck)>>,
    pub requested: HashSet<String>,
    pub sel_event: Option<String>,
    pub sel_deck: Option<u32>,
}

/// Kartenbilder: geladene Texturen und Anfragen an den Bild-Thread.
pub(super) struct ImageCache {
    pub tx: Sender<String>,
    pub textures: HashMap<String, egui::TextureHandle>,
    /// Schon angefragt (wird beim Zeichnen ergänzt).
    pub requested: std::cell::RefCell<HashSet<String>>,
    /// Scryfall kennt kein Bild.
    pub missing: HashSet<String>,
}

impl ImageCache {
    pub fn new(tx: Sender<String>) -> ImageCache {
        ImageCache {
            tx,
            textures: HashMap::new(),
            requested: std::cell::RefCell::new(HashSet::new()),
            missing: HashSet::new(),
        }
    }
}

/// Probehände: Ergebnisse und laufende Berechnungen.
#[derive(Default)]
pub(super) struct SimState {
    /// Je Deck der Deckansicht.
    pub decks: HashMap<u32, sim::Trial>,
    /// Je Deck aus „Deck verbessern“ (grpIds der Liste).
    pub groups: HashMap<Vec<u32>, sim::Trial>,
    pub running: HashSet<SimTarget>,
}

/// „Aus meiner Sammlung“: gebaute Decks.
#[derive(Default)]
pub(super) struct BuiltState {
    pub decks: Option<Vec<Built>>,
    pub sel: usize,
    /// Anzahl geladener Turnierlisten beim letzten Bauen.
    pub lists: usize,
    pub at: Option<std::time::Instant>,
    /// Bauen läuft im Hintergrund.
    pub running: bool,
}

/// „Turnierdecks“ (MTGTop8): Archetypen, Decklisten-Verweise, Auswahl und
/// Format-Legalität.
#[derive(Default)]
pub(super) struct TourState {
    pub archetypes: Vec<Archetype>,
    /// Decklisten-Verweise je Archetyp.
    pub decks: HashMap<u32, Vec<DeckRef>>,
    /// Schon angefragte Decklisten (nicht doppelt laden).
    pub requested: HashSet<u32>,
    pub sel_arch: Option<u32>,
    pub sel_deck: Option<u32>,
    /// Legale Kartenschlüssel je Format (Index in `FORMATS`).
    pub legal: HashMap<usize, HashSet<String>>,
    pub legal_requested: HashSet<usize>,
    /// Offene Abrufe in der Warteschlange.
    pub pending: usize,
    /// „Alle bewerten“ läuft (Status am Ende auf „fertig“ setzen).
    pub rating_all: bool,
}
