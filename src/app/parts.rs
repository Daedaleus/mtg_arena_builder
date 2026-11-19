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
    pub format: Option<Format>,
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
    /// Abruf gescheitert (Netz, Drosselung): wann – erst nach
    /// [`IMAGE_RETRY`] erneut anfragen.
    pub failed: std::cell::RefCell<HashMap<String, std::time::Instant>>,
}

/// Wartezeit, bevor ein gescheitertes Kartenbild erneut angefragt wird.
pub(super) const IMAGE_RETRY: std::time::Duration = std::time::Duration::from_secs(30);

impl ImageCache {
    pub fn new(tx: Sender<String>) -> ImageCache {
        ImageCache {
            tx,
            textures: HashMap::new(),
            requested: std::cell::RefCell::new(HashSet::new()),
            missing: HashSet::new(),
            failed: std::cell::RefCell::new(HashMap::new()),
        }
    }
}

/// Probehände: Ergebnisse und laufende Berechnungen.
#[derive(Default)]
pub(super) struct SimState {
    /// Je Deck der Deckansicht.
    pub decks: HashMap<DeckId, sim::Trial>,
    /// Je Deck aus „Deck verbessern“ (grpIds der Liste).
    pub groups: HashMap<Vec<u32>, sim::Trial>,
    pub running: HashSet<SimTarget>,
    /// Stand je Deck-Kennung; steigt, wenn dort eine neue Liste liegt.
    pub generations: HashMap<DeckId, u64>,
}

impl SimState {
    pub fn list_gen(&self, id: DeckId) -> u64 {
        self.generations.get(&id).copied().unwrap_or(0)
    }

    /// Neue Liste unter `id`: alte Probehände gelten nicht mehr.
    pub fn list_replaced(&mut self, id: DeckId) {
        self.decks.remove(&id);
        *self.generations.entry(id).or_insert(0) += 1;
    }
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
    /// Legale Kartenschlüssel je Format.
    pub legal: HashMap<Format, HashSet<String>>,
    pub legal_requested: HashSet<Format>,
    /// Offene Abrufe in der Warteschlange.
    pub pending: usize,
    /// „Alle bewerten“ läuft (Status am Ende auf „fertig“ setzen).
    pub rating_all: bool,
}
