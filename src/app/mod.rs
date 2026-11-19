//! Oberfläche (egui) und Hintergrund-Abrufe.
//!
//! Aufgeteilt nach Bereichen: `worker` (Hintergrund-Abrufe), `chrome`
//! (Kopfzeile, Seitenleiste, Einstellungen, Erste Schritte), `lists` (Turnierdecks,
//! Aus meiner Sammlung, Eigene Liste), `deck_view` (Deckansicht mit Ersatzkarten),
//! `games_tab` (Meine Partien, Verlauf), `deck_check` (Deck verbessern) und
//! `companion` (Begleitfenster). Logik gehört in die Fachmodule der Bibliothek.

use compute::{DeckAnalysis, GamesAnalysis, SimTarget};
use parts::{BuiltState, GamesState, GgState, ImageCache, SimState, TourState};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};

use eframe::egui::{self, Color32, RichText};
use egui_extras::{Column, TableBuilder};

use crate::budget;
use crate::builder::{self, Built};
use crate::carddb::{CardDb, Rarity};
use crate::collection::{self, Collection};
use crate::deck::{self, Deck, DeckEval, LineEval, Summary};
use crate::deckcheck;
use crate::games::{self, GameRecord};
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

const GREEN: Color32 = Color32::from_rgb(110, 200, 120);
const YELLOW: Color32 = Color32::from_rgb(235, 195, 80);
const RED: Color32 = Color32::from_rgb(235, 115, 105);
/// Woher ein Deck in den Deck-Tabellen (`lists`, `evals`, Ersatzkarten) stammt.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum DeckId {
    /// Selbst eingefügte Liste.
    Pasted,
    /// Das `i`-te aus der Sammlung gebaute Deck.
    Built(u8),
    /// MTGTop8-Deck (Decknummer).
    Top8(u32),
    /// magic.gg-Liste (Kennung aus [`magicgg::deck_id`], immer ab 2³¹).
    Gg(u32),
}

impl DeckId {
    /// Turnierliste (MTGTop8 oder magic.gg)?
    pub fn is_tournament(self) -> bool {
        matches!(self, DeckId::Top8(_) | DeckId::Gg(_))
    }

    pub fn is_built(self) -> bool {
        matches!(self, DeckId::Built(_))
    }

    /// Zahl für `settings.json` (der frühere gemeinsame Zahlenraum: 0 eingefügt,
    /// 1–99 gebaut, darüber MTGTop8, ab 2³¹ magic.gg).
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

fn rarity_color(r: Rarity) -> Color32 {
    match r {
        Rarity::Uncommon => Color32::from_rgb(170, 200, 220),
        Rarity::Rare => Color32::from_rgb(230, 195, 100),
        Rarity::Mythic => Color32::from_rgb(240, 130, 60),
        _ => Color32::GRAY,
    }
}

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
mod settings_view;
mod state;
mod widgets;
mod worker;

use game_list::game_detail;
use progress_view::progress_view;
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
    /// Reihenfolge wie in `settings.json` gespeichert (Index = gespeicherte Zahl).
    const SAVED: [Tab; 6] = [
        Tab::Meta,
        Tab::Paste,
        Tab::Built,
        Tab::Games,
        Tab::DeckCheck,
        Tab::Events,
    ];

    fn from_saved(n: u8) -> Tab {
        Tab::SAVED.get(usize::from(n)).copied().unwrap_or(Tab::Meta)
    }

    fn saved(self) -> u8 {
        Tab::SAVED.iter().position(|t| *t == self).unwrap_or(0) as u8
    }
}

pub struct App {
    data_dir: PathBuf,
    /// Gemeinsamer Netzabruf (siehe `http::shared`).
    net: std::sync::Arc<dyn crate::http::Http>,
    settings: Settings,
    db: Option<std::sync::Arc<CardDb>>,
    db_status: String,
    collection: Option<Collection>,
    owned: HashMap<String, u32>,
    tx_job: Sender<Job>,
    tx_msg: Sender<Msg>,
    rx_msg: Receiver<Msg>,
    lists: HashMap<DeckId, Deck>,
    evals: HashMap<DeckId, DeckEval>,
    tab: Tab,
    paste_text: String,
    /// (Deck, Kartenschlüssel) -> gewählte Ersatzkarte
    repl: HashMap<(DeckId, String), String>,
    subs: HashMap<(DeckId, String), Vec<Suggestion>>,
    status: String,
    syncing: bool,
    sort_by_fit: bool,
    settings_file: SettingsFile,
    ctx: egui::Context,
    /// Gewähltes Deck im Reiter „Deck verbessern“.
    sel_deck_group: usize,
    /// Steigt mit jeder eingetroffenen Nachricht (siehe `compute`).
    data_gen: u64,
    deck_cache: Option<DeckAnalysis>,
    /// Geteilter Zustand des Begleitfensters (läuft, sobald es einmal geöffnet wurde).
    live: Option<std::sync::Arc<live::Shared>>,
    /// Anzahl Turnierlisten beim letzten Abgleich mit dem Begleitfenster.
    live_meta_lists: usize,
    show_settings: bool,
    games: GamesState,
    gg: GgState,
    images: ImageCache,
    sims: SimState,
    built: BuiltState,
    tour: TourState,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> App {
        let data_dir = paths::data_dir();
        let (settings_file, settings) = SettingsFile::load(&data_dir);
        let load_problem = settings_file.problem.clone();
        crate::i18n::set(settings.language.resolve());

        // Etwas großzügigere Abstände und die gewählte Textgröße.
        cc.egui_ctx.style_mut(|style| {
            style.spacing.item_spacing = egui::vec2(8.0, 6.0);
            style.spacing.button_padding = egui::vec2(8.0, 4.0);
        });
        cc.egui_ctx.set_zoom_factor(settings.zoom());

        // Ein Netzabruf für alle Threads: Mindestabstände gelten gemeinsam.
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
            tab: Tab::from_saved(settings.tab),
            paste_text: settings.paste_text.clone(),
            repl,
            collection: Collection::load(&data_dir.join("collection.json")),
            data_dir,
            net,
            settings,
            db: None,
            db_status: String::new(),
            owned: HashMap::new(),
            tx_job,
            tx_msg,
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
        };
        app.load_db();
        app.load_format(false);
        // Auswahl und eingefügte Liste der letzten Sitzung wiederherstellen.
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
    /// Wird von eframe regelmäßig und beim Beenden aufgerufen.
    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        self.save_settings();
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_messages();
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
                ui.label(RichText::new(&self.db_status).color(RED));
                ui.label(tr!("app.arena_ordner_kannst_unter"));
                ui.separator();
            }
            self.onboarding(ui, ctx);
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
