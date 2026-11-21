//! Interface (egui) and background fetches.
//!
//! Split by area: `worker` (background fetches), `chrome` (top bar, side
//! panel, settings, getting started), `lists` (tournament decks, from my
//! collection, own list), `deck_view` (deck view with replacement cards),
//! `games_tab` (my games, progress), `deck_check` (improve deck) and
//! `companion` (companion window). Logic belongs in the library's domain modules.
//!
//! Design follows Atomic Design: `theme` holds all design values (colors for
//! dark and light mode, spacing, image sizes), `atoms` the smallest building
//! blocks (hint text, colored text, rarity), `widgets` larger building blocks
//! (sample hands, wildcard plan); the tabs only put them together. Never
//! hard-code colors, use `theme::Tone`.

use compute::{DeckAnalysis, GamesAnalysis, SimTarget};
use parts::{BuiltState, GamesState, GgState, ImageCache, SimState, TourState};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use tasks::Tasks;

use eframe::egui::{self, Color32, RichText};
use egui_extras::{Column, TableBuilder};

use crate::budget;
use crate::builder::{self, Built};
use crate::carddb::{CardDb, GrpId, Rarity};
use crate::collection::{self, Collection};
use crate::deck::{self, Deck, DeckEval, LineEval, Summary};
use crate::deckcheck;
use crate::games::{self, GameFormat, GameRecord};
use crate::images;
use crate::live;
use crate::magicgg::{self, GgDeck, GgEvent};
use crate::meta::{Archetype, DeckRef, Fetcher, Format};
use crate::opponents;
use crate::paths;
use crate::present::{self, Fit, ago, cost_key, cost_tooltip, event_label, ratio};
use crate::progress;
use crate::settings::{Settings, SettingsFile};
use crate::sim;
use crate::similar::{self, Suggestion};
use crate::style;

/// Where a deck in the deck tables (`lists`, `evals`, replacement cards) comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum DeckId {
    /// Pasted list.
    Pasted,
    /// The `i`-th deck built from the collection.
    Built(u8),
    /// MTGTop8 deck (deck number).
    Top8(u32),
    /// magic.gg list (id from [`magicgg::deck_id`], always from 2³¹ on).
    Gg(u32),
}

impl DeckId {
    /// Tournament list (MTGTop8 or magic.gg)?
    pub fn is_tournament(self) -> bool {
        matches!(self, DeckId::Top8(_) | DeckId::Gg(_))
    }

    pub fn is_built(self) -> bool {
        matches!(self, DeckId::Built(_))
    }

    /// Number for `settings.json` (the former shared number range: 0 pasted,
    /// 1–99 built, above that MTGTop8, from 2³¹ magic.gg).
    pub fn to_raw(self) -> u32 {
        match self {
            DeckId::Pasted => 0,
            DeckId::Built(i) => 1 + u32::from(i),
            DeckId::Top8(n) | DeckId::Gg(n) => n,
        }
    }

    pub fn from_raw(n: u32) -> DeckId {
        match n {
            0 => DeckId::Pasted,
            1..=99 => DeckId::Built((n - 1) as u8),
            n if n >= 0x8000_0000 => DeckId::Gg(n),
            n => DeckId::Top8(n),
        }
    }
}

mod atoms;
mod card_info;
mod chrome;
mod companion;
mod compute;
mod deck_check;
mod deck_view;
mod events;
mod game_list;
mod games_overview;
mod games_tab;
mod lists;
mod parts;
mod progress_view;
mod screenshots;
mod settings_view;
mod state;
mod tasks;
mod theme;
mod widgets;
mod worker;

use atoms::*;
use progress_view::progress_view;
use theme::Tone;
use widgets::*;
use worker::{Job, Msg, worker};

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Meta,
    Events,
    Paste,
    Built,
    Games,
    DeckCheck,
}

impl Tab {
    /// Order as stored in `settings.json` (index = stored number).
    const SAVED: [Tab; 6] = [
        Tab::Meta,
        Tab::Paste,
        Tab::Built,
        Tab::Games,
        Tab::DeckCheck,
        Tab::Events,
    ];

    /// Does the tab make sense in game mode `mode`? Tournament decks, events
    /// and building from the collection need a constructed format with
    /// tournament decks; own lists and deck checks need own decks.
    fn fits(self, mode: GameFormat) -> bool {
        match self {
            Tab::Meta | Tab::Events | Tab::Built => mode.meta().is_some(),
            Tab::Paste | Tab::DeckCheck => mode.has_own_decks(),
            Tab::Games => true,
        }
    }

    fn from_saved(n: u8) -> Tab {
        Tab::SAVED.get(usize::from(n)).copied().unwrap_or(Tab::Meta)
    }

    fn saved(self) -> u8 {
        Tab::SAVED.iter().position(|t| *t == self).unwrap_or(0) as u8
    }
}

pub struct App {
    data_dir: PathBuf,
    /// Background tasks (one thread, see `tasks`).
    tasks: Tasks,
    /// Shared network access (see `http::shared`).
    net: std::sync::Arc<dyn crate::http::Http>,
    settings: Settings,
    db: Option<std::sync::Arc<CardDb>>,
    /// Error while loading the card database (text for the display).
    db_error: String,
    collection: Option<Collection>,
    owned: HashMap<String, u32>,
    tx_job: Sender<Job>,
    rx_msg: Receiver<Msg>,
    lists: HashMap<DeckId, Deck>,
    evals: HashMap<DeckId, DeckEval>,
    tab: Tab,
    paste_text: String,
    /// (deck, card key) -> chosen replacement card
    repl: HashMap<(DeckId, String), String>,
    subs: HashMap<(DeckId, String), Vec<Suggestion>>,
    status: String,
    syncing: bool,
    sort_by_fit: bool,
    settings_file: SettingsFile,
    ctx: egui::Context,
    /// Chosen deck in the tab "Improve deck".
    sel_deck_group: usize,
    /// Rises with every incoming message (see `compute`).
    data_gen: u64,
    deck_cache: Option<DeckAnalysis>,
    /// Shared state of the companion (running once it was opened).
    live: Option<std::sync::Arc<live::Shared>>,
    /// Number of tournament lists at the last sync with the companion.
    live_meta_lists: usize,
    show_settings: bool,
    games: GamesState,
    gg: GgState,
    images: ImageCache,
    sims: SimState,
    built: BuiltState,
    tour: TourState,
    /// Screenshot mode for the README (`--screenshots`), otherwise `None`.
    shooter: Option<screenshots::Shooter>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> App {
        App::start(cc, None)
    }

    /// Starts in screenshot mode: English interface, images into `dir`, then
    /// the window closes (see `screenshots`).
    pub fn for_screenshots(cc: &eframe::CreationContext<'_>, dir: PathBuf) -> App {
        App::start(cc, Some(dir))
    }

    fn start(cc: &eframe::CreationContext<'_>, shots: Option<PathBuf>) -> App {
        let data_dir = paths::data_dir();
        let (settings_file, settings) = SettingsFile::load(&data_dir);
        let load_problem = settings_file.problem.clone();
        crate::i18n::set(match shots {
            // Before everything else, so background computations are English too.
            Some(_) => crate::i18n::Locale::English,
            None => settings.language.resolve(),
        });

        // Spacing and the chosen text size (see `theme`).
        theme::apply(&cc.egui_ctx, settings.zoom());

        // One network access for all threads: minimum gaps apply jointly.
        let net = crate::http::shared();
        let (tx_job, rx_job) = channel();
        let (tx_msg, rx_msg) = channel();
        {
            let (tx, ctx, cache) = (tx_msg.clone(), cc.egui_ctx.clone(), data_dir.join("cache"));
            let net = net.clone();
            std::thread::spawn(move || worker(rx_job, tx, ctx, Fetcher::with_http(cache, net)));
        }
        let img_tx = {
            let (tx, ctx) = (tx_msg.clone(), cc.egui_ctx.clone());
            images::spawn(
                data_dir.join("cache").join("img"),
                net.clone(),
                move |name, img| {
                    let _ = tx.send(Msg::Image(name, img));
                    ctx.request_repaint();
                },
            )
        };

        let repl = settings
            .replacement_map()
            .into_iter()
            .map(|((id, key), rep)| ((DeckId::from_raw(id), key), rep))
            .collect();
        let mut app = App {
            sort_by_fit: settings.sort_by_fit,
            tab: Some(Tab::from_saved(settings.tab))
                .filter(|t| t.fits(settings.mode))
                .unwrap_or(Tab::Games),
            paste_text: settings.paste_text.clone(),
            repl,
            collection: Collection::load(&data_dir.join("collection.json")),
            data_dir,
            net,
            tasks: Tasks::spawn(tx_msg.clone(), cc.egui_ctx.clone()),
            settings,
            db: None,
            db_error: String::new(),
            owned: HashMap::new(),
            tx_job,
            rx_msg,
            lists: HashMap::new(),
            evals: HashMap::new(),
            subs: HashMap::new(),
            status: String::new(),
            syncing: false,
            settings_file,
            ctx: cc.egui_ctx.clone(),
            sel_deck_group: 0,
            data_gen: 0,
            deck_cache: None,
            live: None,
            live_meta_lists: usize::MAX,
            show_settings: false,

            games: GamesState::default(),
            gg: GgState::default(),
            images: ImageCache::new(img_tx),
            sims: SimState::default(),
            built: BuiltState::default(),

            tour: TourState::default(),
            shooter: shots.map(screenshots::Shooter::new),
        };
        app.load_db();
        app.load_format(false);
        // Restore the selection and pasted list of the last session.
        app.tour.sel_arch = app.settings.sel_arch;
        app.tour.sel_deck = app.settings.sel_deck;
        if !app.paste_text.trim().is_empty() {
            app.set_list(DeckId::Pasted, deck::parse(&app.paste_text));
        }
        if let Some(p) = load_problem {
            app.status = p;
        }
        app.scan_games();
        app
    }
}

impl eframe::App for App {
    /// Called by eframe regularly and on exit.
    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        self.save_settings();
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_messages();
        self.drive_screenshots(ctx);
        self.companion(ctx);
        self.top_bar(ctx);
        self.settings_window(ctx);
        match self.tab {
            Tab::Meta => self.side_panel(ctx),
            Tab::Events => self.events_panel(ctx),
            _ => {}
        }
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.db.is_none() {
                toned(ui, Tone::Bad, self.db_status());
                ui.label(tr!("app.set_arena_folder_under"));
                ui.separator();
            }
            self.onboarding(ui);
            match self.tab {
                Tab::Meta => self.meta_tab(ui),
                Tab::Paste => self.paste_tab(ui),
                Tab::Built => self.built_tab(ui),
                Tab::Games => self.games_tab(ui),
                Tab::DeckCheck => self.deck_check_tab(ui),
                Tab::Events => self.events_tab(ui),
            }
        });
    }
}

#[cfg(test)]
#[path = "../tests/app.rs"]
mod tests;
