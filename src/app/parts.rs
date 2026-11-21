//! Partial states of the app, bundled by area. `App` holds one of each; so it
//! stays visible which fields belong together.

use super::*;

/// "My games": games, filter, selection and cached analysis.
#[derive(Default)]
pub(super) struct GamesState {
    pub list: Vec<GameRecord>,
    pub loading: bool,
    /// When last read (for automatic reloading).
    pub scanned: Option<std::time::Instant>,
    /// "Detailed Logs" are off in Arena.
    pub logs_disabled: bool,
    /// Which games are analysed (mode group, deck).
    pub filter: games::GamesFilter,
    /// Filter already preset with the last game (once per session).
    pub filter_set: bool,
    pub sel: Option<usize>,
    pub view: GamesView,
    /// Page of the game list (changed while drawing).
    pub page: std::cell::Cell<usize>,
    pub cache: Option<GamesAnalysis>,
}

/// Sub page of "My games".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum GamesView {
    /// Key figures and the most important in short.
    #[default]
    Overview,
    /// Tips and all findings.
    Improve,
    /// Play style and matching decks.
    Style,
    Opponents,
    List,
    Progress,
}

impl GamesView {
    pub const ALL: [GamesView; 6] = [
        GamesView::Overview,
        GamesView::Improve,
        GamesView::Style,
        GamesView::Opponents,
        GamesView::List,
        GamesView::Progress,
    ];

    pub fn label(self) -> String {
        match self {
            GamesView::Overview => tr!("games_tab.overview"),
            GamesView::Improve => tr!("games_tab.improve"),
            GamesView::Style => tr!("games_tab.style"),
            GamesView::Opponents => tr!("games_tab.opponents"),
            GamesView::List => tr!("games_tab.game_list"),
            GamesView::Progress => tr!("games_tab.progress"),
        }
    }
}

/// "Official events" (magic.gg).
#[derive(Default)]
pub(super) struct GgState {
    pub events: Vec<GgEvent>,
    /// Format for which `events` are loaded.
    pub format: Option<Format>,
    /// Decks per event (slug) with their ids.
    pub decks: HashMap<String, Vec<(u32, GgDeck)>>,
    pub requested: HashSet<String>,
    pub sel_event: Option<String>,
    pub sel_deck: Option<u32>,
}

/// Card images: loaded textures and requests to the image thread.
pub(super) struct ImageCache {
    pub tx: Sender<String>,
    pub textures: HashMap<String, egui::TextureHandle>,
    /// Already requested (extended while drawing).
    pub requested: std::cell::RefCell<HashSet<String>>,
    /// Scryfall knows no image.
    pub missing: HashSet<String>,
    /// Fetch failed (network, throttling): when – request again only after
    /// [`IMAGE_RETRY`].
    pub failed: std::cell::RefCell<HashMap<String, std::time::Instant>>,
}

/// Waiting time before a failed card image is requested again.
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

/// Sample hands: results and running computations.
#[derive(Default)]
pub(super) struct SimState {
    /// Per deck of the deck view.
    pub decks: HashMap<DeckId, sim::Trial>,
    /// Per deck from "Improve deck" (grpIds of the list).
    pub groups: HashMap<Vec<GrpId>, sim::Trial>,
    pub running: HashSet<SimTarget>,
    /// State per deck id; rises when a new list is there.
    pub generations: HashMap<DeckId, u64>,
}

impl SimState {
    pub fn list_gen(&self, id: DeckId) -> u64 {
        self.generations.get(&id).copied().unwrap_or(0)
    }

    /// New list under `id`: old sample hands no longer apply.
    pub fn list_replaced(&mut self, id: DeckId) {
        self.decks.remove(&id);
        *self.generations.entry(id).or_insert(0) += 1;
    }
}

/// "From my collection": built decks.
#[derive(Default)]
pub(super) struct BuiltState {
    pub decks: Option<Vec<Built>>,
    pub sel: usize,
    /// Number of loaded tournament lists at the last build.
    pub lists: usize,
    pub at: Option<std::time::Instant>,
    /// Building runs in the background.
    pub running: bool,
}

/// "Tournament decks" (MTGTop8): archetypes, decklist references, selection
/// and format legality.
#[derive(Default)]
pub(super) struct TourState {
    pub archetypes: Vec<Archetype>,
    /// Decklist references per archetype.
    pub decks: HashMap<u32, Vec<DeckRef>>,
    /// Decklists already requested (don't load twice).
    pub requested: HashSet<u32>,
    pub sel_arch: Option<u32>,
    pub sel_deck: Option<u32>,
    /// Legal card keys per format.
    pub legal: HashMap<Format, HashSet<String>>,
    pub legal_requested: HashSet<Format>,
    /// Open fetches in the queue.
    pub pending: usize,
    /// "Rate all" is running (set the status to "done" at the end).
    pub rating_all: bool,
}
