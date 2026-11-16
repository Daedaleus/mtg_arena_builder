//! Oberfläche (egui) und Hintergrund-Abrufe.
//!
//! Aufgeteilt nach Bereichen: `worker` (Hintergrund-Abrufe), `chrome`
//! (Kopfzeile, Seitenleiste, Einstellungen, Erste Schritte), `lists` (Turnierdecks,
//! Aus meiner Sammlung, Eigene Liste), `deck_view` (Deckansicht mit Ersatzkarten),
//! `games_tab` (Meine Partien, Verlauf), `deck_check` (Deck verbessern) und
//! `companion` (Begleitfenster). Logik gehört in die Fachmodule der Bibliothek.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};

use eframe::egui::{self, Color32, RichText};
use egui_extras::{Column, TableBuilder};

use crate::budget;
use crate::builder::{self, Built};
use crate::carddb::{CardDb, RARITY_COMMON, rarity_label};
use crate::collection::{self, Collection};
use crate::deck::{self, Deck, DeckEval, LineEval, Summary};
use crate::deckcheck;
use crate::games::{self, GameRecord};
use crate::images;
use crate::live;
use crate::magicgg::{self, GgDeck, GgEvent};
use crate::meta::{Archetype, DeckRef, FORMATS, Fetcher};
use crate::opponents;
use crate::paths;
use crate::present::{self, Fit, ago, cost_key, cost_tooltip, event_label, ratio};
use crate::progress;
use crate::settings::{Settings, SettingsFile};
use crate::sim;
use crate::similar::{self, Suggestion};

const GREEN: Color32 = Color32::from_rgb(110, 200, 120);
const YELLOW: Color32 = Color32::from_rgb(235, 195, 80);
const RED: Color32 = Color32::from_rgb(235, 115, 105);
const RARITY_NAMES: [&str; 4] = ["Common", "Uncommon", "Rare", "Mythic"];
/// Schlüssel der selbst eingefügten Liste in den Deck-Tabellen.
const PASTED: u32 = 0;
/// Schlüsselbereich der aus der Sammlung gebauten Decks (MTGTop8-IDs sind viel größer).
const BUILT_BASE: u32 = 1;
const BUILT_MAX: u32 = 99;

fn rarity_color(r: u8) -> Color32 {
    match r {
        3 => Color32::from_rgb(170, 200, 220),
        4 => Color32::from_rgb(230, 195, 100),
        5 => Color32::from_rgb(240, 130, 60),
        _ => Color32::GRAY,
    }
}

mod chrome;
mod companion;
mod deck_check;
mod deck_view;
mod events;
mod games_tab;
mod lists;
mod worker;

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

pub struct App {
    data_dir: PathBuf,
    settings: Settings,
    db: Option<std::sync::Arc<CardDb>>,
    db_status: String,
    collection: Option<Collection>,
    owned: HashMap<String, u32>,
    tx_job: Sender<Job>,
    tx_msg: Sender<Msg>,
    rx_msg: Receiver<Msg>,
    archetypes: Vec<Archetype>,
    decks: HashMap<u32, Vec<DeckRef>>,
    lists: HashMap<u32, Deck>,
    evals: HashMap<u32, DeckEval>,
    legal: HashMap<usize, HashSet<String>>,
    legal_requested: HashSet<usize>,
    /// Decklisten, deren Abruf schon eingereiht wurde.
    list_requested: HashSet<u32>,
    sel_arch: Option<u32>,
    sel_deck: Option<u32>,
    tab: Tab,
    paste_text: String,
    /// (Deck, Kartenschlüssel) -> gewählte Ersatzkarte
    repl: HashMap<(u32, String), String>,
    subs: HashMap<(u32, String), Vec<Suggestion>>,
    status: String,
    pending: usize,
    syncing: bool,
    sort_by_fit: bool,
    settings_file: SettingsFile,
    /// Aus der Sammlung gebaute Deckvorschläge (None = noch nicht berechnet).
    built: Option<Vec<Built>>,
    sel_built: usize,
    /// Anzahl Turnierlisten und Zeitpunkt bei der letzten Berechnung der Vorschläge.
    built_lists: usize,
    built_at: Option<std::time::Instant>,
    ctx: egui::Context,
    games: Vec<GameRecord>,
    games_loading: bool,
    games_scanned: Option<std::time::Instant>,
    logs_disabled: bool,
    /// Filter nach Arena-Event (None = alle).
    games_event: Option<String>,
    sel_game: Option<usize>,
    /// Gewähltes Deck im Reiter „Deck verbessern“.
    sel_deck_group: usize,
    img_tx: Sender<String>,
    textures: HashMap<String, egui::TextureHandle>,
    img_requested: std::cell::RefCell<HashSet<String>>,
    img_missing: HashSet<String>,
    /// magic.gg: Events des Formats, für das sie geladen wurden.
    gg_events: Vec<GgEvent>,
    gg_format: Option<usize>,
    /// magic.gg: Decks je Event (Kennung, Deck).
    gg_decks: HashMap<String, Vec<(u32, GgDeck)>>,
    gg_requested: HashSet<String>,
    sel_gg_event: Option<String>,
    sel_gg_deck: Option<u32>,
    /// Ergebnisse der Probehände je Deck (Turnierdeck, Vorschlag, eigene Liste).
    sims: HashMap<u32, sim::SimReport>,
    /// Probehände für gespielte Decks („Deck verbessern“), nach Deck-Bezeichnung.
    group_sims: HashMap<String, sim::SimReport>,
    /// Geteilter Zustand des Begleitfensters (läuft, sobald es einmal geöffnet wurde).
    live: Option<std::sync::Arc<live::Shared>>,
    /// Anzahl Turnierlisten beim letzten Abgleich mit dem Begleitfenster.
    live_meta_lists: usize,
    show_settings: bool,
    /// Unterseite von „Meine Partien“: 0 = Übersicht, 1 = Partienliste.
    games_view: u8,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> App {
        let data_dir = paths::data_dir();
        let (settings_file, settings) = SettingsFile::load(&data_dir);

        // Etwas großzügigere Abstände und die gewählte Textgröße.
        cc.egui_ctx.style_mut(|style| {
            style.spacing.item_spacing = egui::vec2(8.0, 6.0);
            style.spacing.button_padding = egui::vec2(8.0, 4.0);
        });
        cc.egui_ctx.set_zoom_factor(settings.zoom());

        let (tx_job, rx_job) = channel();
        let (tx_msg, rx_msg) = channel();
        {
            let (tx, ctx, cache) = (tx_msg.clone(), cc.egui_ctx.clone(), data_dir.join("cache"));
            std::thread::spawn(move || worker(rx_job, tx, ctx, cache));
        }
        let img_tx = {
            let (tx, ctx) = (tx_msg.clone(), cc.egui_ctx.clone());
            images::spawn(data_dir.join("cache").join("img"), move |name, img| {
                let _ = tx.send(Msg::Image(name, img));
                ctx.request_repaint();
            })
        };

        let repl = settings.replacement_map();
        let mut app = App {
            sort_by_fit: settings.sort_by_fit,
            tab: match settings.tab {
                1 => Tab::Paste,
                2 => Tab::Built,
                3 => Tab::Games,
                4 => Tab::DeckCheck,
                5 => Tab::Events,
                _ => Tab::Meta,
            },
            paste_text: settings.paste_text.clone(),
            repl,
            collection: Collection::load(&data_dir.join("collection.json")),
            data_dir,
            settings,
            db: None,
            db_status: String::new(),
            owned: HashMap::new(),
            tx_job,
            tx_msg,
            rx_msg,
            archetypes: Vec::new(),
            decks: HashMap::new(),
            lists: HashMap::new(),
            evals: HashMap::new(),
            legal: HashMap::new(),
            legal_requested: HashSet::new(),
            list_requested: HashSet::new(),
            sel_arch: None,
            sel_deck: None,
            subs: HashMap::new(),
            status: String::new(),
            pending: 0,
            syncing: false,
            settings_file,
            built: None,
            sel_built: 0,
            built_lists: 0,
            built_at: None,
            ctx: cc.egui_ctx.clone(),
            games: Vec::new(),
            games_loading: false,
            games_scanned: None,
            logs_disabled: false,
            games_event: None,
            sel_game: None,
            sel_deck_group: 0,
            img_tx,
            textures: HashMap::new(),
            img_requested: std::cell::RefCell::new(HashSet::new()),
            img_missing: HashSet::new(),
            gg_events: Vec::new(),
            gg_format: None,
            gg_decks: HashMap::new(),
            gg_requested: HashSet::new(),
            sel_gg_event: None,
            sel_gg_deck: None,
            sims: HashMap::new(),
            group_sims: HashMap::new(),
            live: None,
            live_meta_lists: usize::MAX,
            show_settings: false,
            games_view: 0,
        };
        app.settings.format = app.settings.format.min(FORMATS.len() - 1);
        app.load_db();
        app.load_format(false);
        // Auswahl und eingefügte Liste der letzten Sitzung wiederherstellen.
        app.sel_arch = app.settings.sel_arch;
        app.sel_deck = app.settings.sel_deck;
        if !app.paste_text.trim().is_empty() {
            app.set_list(PASTED, deck::parse(&app.paste_text));
        }
        app.scan_games();
        app
    }

    /// Liest die Arena-Logs im Hintergrund ein und ergänzt `games.json`.
    fn scan_games(&mut self) {
        if self.games_loading {
            return;
        }
        self.games_loading = true;
        self.games_scanned = Some(std::time::Instant::now());
        let (tx, ctx, store) = (
            self.tx_msg.clone(),
            self.ctx.clone(),
            self.data_dir.join("games.json"),
        );
        std::thread::spawn(move || {
            let logs = paths::player_logs();
            let games = games::sync(&store, &logs);
            let disabled = logs
                .iter()
                .find(|p| p.file_name().is_some_and(|n| n == "Player.log"))
                .is_some_and(|p| games::detailed_logs_disabled(p));
            let _ = tx.send(Msg::Games(games, disabled));
            ctx.request_repaint();
        });
    }

    /// Schreibt Einstellungen und Sitzungszustand nach `settings.json`
    /// (nur wenn sich etwas geändert hat).
    fn save_settings(&mut self) {
        self.settings.sort_by_fit = self.sort_by_fit;
        self.settings.tab = match self.tab {
            Tab::Meta => 0,
            Tab::Paste => 1,
            Tab::Built => 2,
            Tab::Games => 3,
            Tab::DeckCheck => 4,
            Tab::Events => 5,
        };
        self.settings.paste_text = self.paste_text.clone();
        self.settings.sel_arch = self.sel_arch;
        self.settings.sel_deck = self.sel_deck;
        self.settings.set_replacements(&self.repl);
        self.settings_file.save(&self.settings);
    }

    fn load_db(&mut self) {
        let found =
            paths::find_card_db(Some(&self.settings.mtga_dir)).and_then(|p| CardDb::load(&p));
        match found {
            Ok(db) => {
                self.db_status = format!(
                    "Kartendaten: {} Karten (Arena {})",
                    db.cards.len(),
                    db.version
                );
                self.db = Some(std::sync::Arc::new(db));
            }
            Err(e) => {
                self.db_status = format!("{e:#}");
                self.db = None;
            }
        }
        self.refresh_owned();
    }

    /// Nach Änderungen an Sammlung/Kartendaten alles neu bewerten.
    fn refresh_owned(&mut self) {
        self.owned = match (&self.collection, &self.db) {
            (Some(c), Some(db)) => c.owned_by_name(db),
            _ => HashMap::new(),
        };
        self.subs.clear();
        self.built = None;
        self.evals.clear();
        if let Some(db) = &self.db {
            for (id, list) in &self.lists {
                self.evals
                    .insert(*id, deck::evaluate(list, db, &self.owned));
            }
        }
    }

    fn load_format(&mut self, force: bool) {
        let format = self.settings.format;
        let _ = self.tx_job.send(Job::Clear);
        let _ = self.tx_job.send(Job::Archetypes { format, force });
        self.archetypes.clear();
        self.decks.clear();
        self.list_requested.clear();
        self.sel_arch = None;
        self.sel_deck = None;
        self.subs.clear();
        self.built = None;
        // magic.gg-Events beim nächsten Öffnen für das neue Format laden.
        self.gg_events.clear();
        self.gg_format = None;
        self.sel_gg_event = None;
        self.sel_gg_deck = None;
        self.status = format!("Lade {}-Metagame von MTGTop8 …", FORMATS[format].1);
        if self.legal_requested.insert(format) {
            let (tx, cache) = (self.tx_msg.clone(), self.data_dir.join("cache"));
            std::thread::spawn(move || {
                let r = Fetcher::new(cache)
                    .legal_names(FORMATS[format].2)
                    .map_err(|e| format!("{e:#}"));
                let _ = tx.send(Msg::Legal(format, r));
            });
        }
    }

    fn sync_collection(&mut self, ctx: &egui::Context) {
        self.syncing = true;
        self.status = "Lese Sammlung aus MTG Arena …".into();
        let (tx, ctx, path) = (
            self.tx_msg.clone(),
            ctx.clone(),
            self.data_dir.join("collection.json"),
        );
        std::thread::spawn(move || {
            let running = cfg!(target_os = "linux") || mtga_reader::api::find_process("MTGA");
            let result = if !running {
                Err(
                    "MTG Arena läuft nicht. Bitte das Spiel starten und bis ins Hauptmenü gehen."
                        .to_string(),
                )
            } else {
                collection::read_from_game().or_else(|first| {
                    if cfg!(windows) && !collection::is_admin() {
                        // Arena läuft mit höheren Rechten -> einmalig per UAC nachfragen.
                        collection::read_elevated(&path)
                            .map_err(|e| format!("{e} (ohne Adminrechte: {first})"))
                    } else {
                        Err(first)
                    }
                })
            };
            if let Ok(c) = &result {
                let _ = c.save(&path);
            }
            let _ = tx.send(Msg::Collection(result));
            ctx.request_repaint();
        });
    }
    /// Reiht alle noch nicht geladenen Listen eines Archetyps ein, damit die
    /// günstigste Variante gefunden wird (eine Anfrage alle 5 s, dauerhaft gecacht).
    fn queue_variants(&mut self, arch: u32) {
        let ids: Vec<u32> = self
            .decks
            .get(&arch)
            .into_iter()
            .flatten()
            .map(|d| d.id)
            .collect();
        for deck_id in ids {
            if !self.lists.contains_key(&deck_id) && self.list_requested.insert(deck_id) {
                let _ = self.tx_job.send(Job::List {
                    deck_id,
                    urgent: false,
                });
            }
        }
    }

    fn set_list(&mut self, deck_id: u32, list: Deck) {
        if let Some(db) = &self.db {
            self.evals
                .insert(deck_id, deck::evaluate(&list, db, &self.owned));
        }
        self.lists.insert(deck_id, list);
    }

    fn replaced_qty(&self, deck_id: u32, line: &LineEval) -> u32 {
        let Some(key) = &line.key else { return 0 };
        let Some(rep) = self.repl.get(&(deck_id, key.clone())) else {
            return 0;
        };
        match &self.db {
            Some(db) => budget::replaced_qty(db, &self.owned, rep, line.missing),
            None => 0,
        }
    }

    /// Ersetzt fehlende Karten automatisch so, dass die Wildcards reichen.
    fn apply_budget(&mut self, deck_id: u32, eval: &DeckEval) {
        let Some(db) = &self.db else { return };
        let legal = self.legal.get(&self.settings.format);
        let plan = budget::plan(
            eval,
            db,
            &self.owned,
            legal,
            self.wildcards(),
            self.settings.include_side,
        );
        // Gewählter Ersatz muss in der Auswahlliste stehen – sonst ergänzen.
        for (key, rep) in &plan.replacements {
            let slot = (deck_id, key.clone());
            let listed = self
                .subs
                .get(&slot)
                .is_some_and(|s| s.iter().any(|o| &o.key == rep));
            if !listed && let Some(c) = db.representative(rep) {
                self.subs.entry(slot).or_default().push(Suggestion {
                    key: rep.clone(),
                    name: c.name.clone(),
                    owned: self.owned.get(rep).copied().unwrap_or(0),
                    score: 0.0,
                    info: if c.basic {
                        "Standardland".into()
                    } else {
                        c.mana_cost.clone()
                    },
                });
            }
        }
        self.repl.retain(|k, _| k.0 != deck_id);
        for (key, rep) in &plan.replacements {
            self.repl.insert((deck_id, key.clone()), rep.clone());
        }
        let text = budget::plan_text(&plan);
        self.status = text;
    }

    fn summary(&self, deck_id: u32) -> Option<Summary> {
        let eval = self.evals.get(&deck_id)?;
        Some(eval.summary(self.settings.include_side, |l| {
            self.replaced_qty(deck_id, l)
        }))
    }

    fn wildcards(&self) -> [u32; 4] {
        self.collection
            .as_ref()
            .map(|c| c.wildcards.as_array())
            .unwrap_or_default()
    }

    /// Farbe + Kurztext: was das Deck an Wildcards kostet.
    fn fit_text(&self, s: &Summary) -> (Color32, String) {
        let (fit, text) = present::fit(s, self.collection.is_some(), self.wildcards());
        let color = match fit {
            Fit::Unknown => Color32::GRAY,
            Fit::Complete => GREEN,
            Fit::Craftable => YELLOW,
            Fit::TooExpensive => RED,
        };
        (color, text)
    }

    /// Günstigstes bereits bewertetes Deck eines Archetyps.
    fn best_of_arch(&self, arch: u32) -> Option<Summary> {
        self.decks
            .get(&arch)?
            .iter()
            .filter_map(|d| self.summary(d.id))
            .min_by_key(|s| cost_key(s, self.wildcards()))
    }

    /// Alle geladenen Turnierlisten des aktuellen Formats, nach Archetyp.
    fn meta_archetypes(&self) -> Vec<deckcheck::MetaArchetype<'_>> {
        self.archetypes
            .iter()
            .map(|a| deckcheck::MetaArchetype {
                id: a.id,
                name: a.name.clone(),
                lists: self
                    .decks
                    .get(&a.id)
                    .into_iter()
                    .flatten()
                    .filter_map(|d| self.lists.get(&d.id))
                    .collect(),
            })
            .collect()
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
                ui.label("Den Arena-Ordner kannst du unter „⚙ Einstellungen“ angeben.");
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
