//! Oberfläche (egui) und Hintergrund-Abrufe.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};

use eframe::egui::{self, Color32, RichText};
use egui_extras::{Column, TableBuilder};
use serde::{Deserialize, Serialize};

use crate::budget;
use crate::builder::{self, Built};
use crate::carddb::{CardDb, RARITY_COMMON, rarity_label};
use crate::collection::{self, Collection};
use crate::deck::{self, Deck, DeckEval, LineEval, Summary};
use crate::deckcheck;
use crate::games::{self, GameRecord};
use crate::images;
use crate::live;
use crate::meta::{Archetype, DeckRef, FORMATS, Fetcher};
use crate::opponents;
use crate::paths;
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

/// Sortierschlüssel „wie teuer ist das Deck für mich“: erst nicht verfügbare
/// Karten, dann ob die Wildcards reichen, dann Rares/Mythics (Mythic zählt doppelt).
fn cost_key(s: &Summary, wildcards: [u32; 4]) -> (u32, bool, u32, u32, u32) {
    (
        s.unavailable,
        !s.craftable(wildcards),
        s.need[2] + 2 * s.need[3],
        s.need[1],
        s.need[0],
    )
}

fn cost_tooltip(s: &Summary) -> String {
    let mut text = format!(
        "{:.0} % vorhanden ({} von {} Karten)\nFehlt: {} Common, {} Uncommon, {} Rare, {} Mythic",
        s.percent().floor(),
        s.total - s.missing,
        s.total,
        s.need[0],
        s.need[1],
        s.need[2],
        s.need[3]
    );
    if s.unavailable > 0 {
        text.push_str(&format!(
            "\n{} Karten gibt es in Arena nicht",
            s.unavailable
        ));
    }
    text
}

/// Alles, was einen Neustart überleben soll (`settings.json`).
#[derive(Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    mtga_dir: String,
    /// Begleitfenster geöffnet?
    companion: bool,
    format: usize,
    include_side: bool,
    with_sets: bool,
    sort_by_fit: bool,
    /// Aktiver Reiter: 0 = Turnierdecks, 1 = eigene Liste, 2 = aus Sammlung.
    tab: u8,
    paste_text: String,
    sel_arch: Option<u32>,
    sel_deck: Option<u32>,
    /// Gewählte Ersatzkarten: (Deck, Kartenschlüssel, Ersatz).
    replacements: Vec<(u32, String, String)>,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            mtga_dir: String::new(),
            companion: false,
            format: 0,
            include_side: true,
            with_sets: true,
            sort_by_fit: false,
            tab: 0,
            paste_text: String::new(),
            sel_arch: None,
            sel_deck: None,
            replacements: Vec::new(),
        }
    }
}

enum Job {
    Archetypes {
        format: usize,
        force: bool,
    },
    Decks {
        format: usize,
        arch: Archetype,
        urgent: bool,
    },
    List {
        deck_id: u32,
        urgent: bool,
    },
    /// Bereits heruntergeladene Decks aus dem Cache wiederherstellen (ohne Netz).
    Restore {
        format: usize,
        archetypes: Vec<Archetype>,
    },
    Clear,
}

enum Msg {
    Archetypes(usize, Result<Vec<Archetype>, String>),
    Decks(usize, u32, Result<Vec<DeckRef>, String>),
    List(u32, Result<String, String>),
    Legal(usize, Result<HashSet<String>, String>),
    Collection(Result<Collection, String>),
    /// Wiederherstellung aus dem Cache ist abgeschlossen.
    Restored(usize),
    /// Partien aus den Arena-Logs; `bool` = „Detaillierte Logs“ ist aus.
    Games(Vec<GameRecord>, bool),
    /// Kartenbild von Scryfall (None = keins gefunden).
    Image(String, Option<egui::ColorImage>),
    Pending(usize),
}

/// Arbeitet Web-Abrufe nacheinander ab (MTGTop8 erlaubt nur eine Anfrage alle 5 s).
fn worker(rx: Receiver<Job>, tx: Sender<Msg>, ctx: egui::Context, cache: PathBuf) {
    let mut fetcher = Fetcher::new(cache);
    let mut queue: VecDeque<Job> = VecDeque::new();
    loop {
        // „Clear“ wirkt sofort beim Eintreffen – es darf nie in der Warteschlange
        // landen, sonst löscht es später eingereihte Aufträge.
        let intake = |job: Job, queue: &mut VecDeque<Job>| match job {
            Job::Clear => queue.clear(),
            Job::Decks { urgent: true, .. } | Job::List { urgent: true, .. } => {
                queue.push_front(job)
            }
            other => queue.push_back(other),
        };
        if queue.is_empty() {
            match rx.recv() {
                Ok(job) => intake(job, &mut queue),
                Err(_) => return,
            }
        }
        while let Ok(job) = rx.try_recv() {
            intake(job, &mut queue);
        }
        let Some(job) = queue.pop_front() else {
            continue;
        };
        let err = |e: anyhow::Error| format!("{e:#}");
        let send = |m: Msg| {
            let _ = tx.send(m);
            ctx.request_repaint();
        };
        match job {
            Job::Clear => queue.clear(),
            Job::Archetypes { format, force } => {
                send(Msg::Archetypes(
                    format,
                    fetcher.archetypes(FORMATS[format].0, force).map_err(err),
                ));
            }
            Job::Decks { format, arch, .. } => {
                let decks = fetcher.decks(FORMATS[format].0, &arch).map_err(err);
                let top = decks.as_ref().ok().and_then(|d| d.first()).map(|d| d.id);
                send(Msg::Decks(format, arch.id, decks));
                // Das neueste Deck gleich mitladen, damit der Archetyp bewertet werden kann.
                if let Some(deck_id) = top {
                    send(Msg::List(deck_id, fetcher.decklist(deck_id).map_err(err)));
                }
            }
            Job::List { deck_id, .. } => {
                send(Msg::List(deck_id, fetcher.decklist(deck_id).map_err(err)));
            }
            Job::Restore { format, archetypes } => {
                for arch in archetypes {
                    let Some(decks) = fetcher.cached_decks(FORMATS[format].0, &arch) else {
                        continue;
                    };
                    let lists: Vec<(u32, String)> = decks
                        .iter()
                        .filter_map(|d| Some((d.id, fetcher.cached_decklist(d.id)?)))
                        .collect();
                    send(Msg::Decks(format, arch.id, Ok(decks)));
                    for (deck_id, text) in lists {
                        send(Msg::List(deck_id, Ok(text)));
                    }
                }
                send(Msg::Restored(format));
            }
        }
        send(Msg::Pending(queue.len()));
    }
}

#[derive(PartialEq)]
enum Tab {
    Meta,
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
    /// Zuletzt geschriebener Stand von `settings.json`.
    saved_json: Vec<u8>,
    /// Aus der Sammlung gebaute Deckvorschläge (None = noch nicht berechnet).
    built: Option<Vec<Built>>,
    sel_built: usize,
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
    /// Geteilter Zustand des Begleitfensters (läuft, sobald es einmal geöffnet wurde).
    live: Option<std::sync::Arc<live::Shared>>,
    /// Anzahl Turnierlisten beim letzten Abgleich mit dem Begleitfenster.
    live_meta_lists: usize,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> App {
        let data_dir = paths::data_dir();
        let settings: Settings =
            paths::read_json(&data_dir.join("settings.json")).unwrap_or_default();

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

        let repl = settings
            .replacements
            .iter()
            .cloned()
            .map(|(deck, key, rep)| ((deck, key), rep))
            .collect();
        let mut app = App {
            sort_by_fit: settings.sort_by_fit,
            tab: match settings.tab {
                1 => Tab::Paste,
                2 => Tab::Built,
                3 => Tab::Games,
                4 => Tab::DeckCheck,
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
            saved_json: Vec::new(),
            built: None,
            sel_built: 0,
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
            live: None,
            live_meta_lists: usize::MAX,
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
        };
        self.settings.paste_text = self.paste_text.clone();
        self.settings.sel_arch = self.sel_arch;
        self.settings.sel_deck = self.sel_deck;
        let mut replacements: Vec<(u32, String, String)> = self
            .repl
            .iter()
            .map(|((deck, key), rep)| (*deck, key.clone(), rep.clone()))
            .collect();
        replacements.sort();
        self.settings.replacements = replacements;
        if let Ok(json) = serde_json::to_vec_pretty(&self.settings)
            && json != self.saved_json
            && paths::write_atomic(&self.data_dir.join("settings.json"), &json).is_ok()
        {
            self.saved_json = json;
        }
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

    fn handle_messages(&mut self) {
        while let Ok(msg) = self.rx_msg.try_recv() {
            match msg {
                Msg::Archetypes(format, result) if format == self.settings.format => match result {
                    Ok(list) => {
                        self.status = format!("{} Archetypen geladen.", list.len());
                        let _ = self.tx_job.send(Job::Restore {
                            format,
                            archetypes: list.clone(),
                        });
                        self.archetypes = list;
                    }
                    Err(e) => self.status = e,
                },
                Msg::Decks(format, arch, result) if format == self.settings.format => {
                    match result {
                        Ok(list) => {
                            if self.sel_arch == Some(arch) && self.sel_deck.is_none() {
                                self.sel_deck = list.first().map(|d| d.id);
                            }
                            self.decks.insert(arch, list);
                            if self.sel_arch == Some(arch) {
                                self.queue_variants(arch);
                            }
                        }
                        Err(e) => self.status = e,
                    }
                }
                Msg::List(deck_id, result) => match result {
                    Ok(text) => self.set_list(deck_id, deck::parse(&text)),
                    Err(e) => self.status = e,
                },
                Msg::Legal(format, result) => match result {
                    Ok(names) => {
                        self.legal.insert(format, names);
                        self.subs.clear();
                        self.built = None;
                    }
                    Err(e) => {
                        self.status = format!(
                            "Format-Legalität nicht verfügbar ({e}) – Ersatzvorschläge sind ungefiltert."
                        )
                    }
                },
                Msg::Collection(result) => {
                    self.syncing = false;
                    match result {
                        Ok(c) => {
                            self.status = format!("Sammlung gelesen: {} Karten.", c.total_cards());
                            self.collection = Some(c);
                            self.refresh_owned();
                        }
                        Err(e) => {
                            self.status = format!("Sammlung konnte nicht gelesen werden: {e}")
                        }
                    }
                }
                Msg::Restored(format) if format == self.settings.format => {
                    // Gemerkte Auswahl, deren Decks nicht (mehr) im Cache liegen, nachladen.
                    if let Some(id) = self.sel_arch.filter(|id| !self.decks.contains_key(id)) {
                        match self.archetypes.iter().find(|a| a.id == id).cloned() {
                            Some(arch) => {
                                let _ = self.tx_job.send(Job::Decks {
                                    format,
                                    arch,
                                    urgent: true,
                                });
                            }
                            None => {
                                self.sel_arch = None;
                                self.sel_deck = None;
                            }
                        }
                    }
                }
                Msg::Image(name, img) => match img {
                    Some(img) => {
                        let tex =
                            self.ctx
                                .load_texture(format!("card:{name}"), img, Default::default());
                        self.textures.insert(name, tex);
                    }
                    None => {
                        self.img_missing.insert(name);
                    }
                },
                Msg::Games(games, disabled) => {
                    self.games_loading = false;
                    self.logs_disabled = disabled;
                    if games.len() != self.games.len() {
                        self.sel_game = None;
                    }
                    self.games = games;
                }
                Msg::Pending(n) => {
                    self.pending = n;
                    if n == 0 && self.status.starts_with("Bewerte") {
                        self.status = "Alle Archetypen bewertet.".into();
                    }
                }
                _ => {}
            }
        }
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
        // Standardländer gibt es in Arena unbegrenzt.
        let basic = self
            .db
            .as_ref()
            .and_then(|db| db.representative(rep))
            .is_some_and(|c| c.basic);
        if basic {
            line.missing
        } else {
            self.owned.get(rep).copied().unwrap_or(0).min(line.missing)
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
        for (key, rep) in plan.replacements {
            self.repl.insert((deck_id, key), rep);
        }
        let c = plan.crafted;
        let mut text = format!(
            "Budget-Version: {} Karten durch eigene ersetzt, mit Wildcards herzustellen: {} Common, {} Uncommon, {} Rare, {} Mythic.",
            plan.replaced, c[0], c[1], c[2], c[3]
        );
        if !plan.unresolved.is_empty() {
            text.push_str(&format!(
                " Ohne passenden Ersatz: {}.",
                plan.unresolved.join(", ")
            ));
        }
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
        if self.collection.is_none() {
            return (Color32::GRAY, "–".into());
        }
        if s.complete() {
            return (GREEN, "komplett".into());
        }
        let mut text = if s.need[2] + s.need[3] > 0 {
            format!("{} R / {} M", s.need[2], s.need[3])
        } else {
            format!("nur {} C/U", s.need[0] + s.need[1])
        };
        if s.unavailable > 0 {
            text.push_str(&format!(" (+{} n. v.)", s.unavailable));
        }
        (
            if s.craftable(self.wildcards()) {
                YELLOW
            } else {
                RED
            },
            text,
        )
    }

    /// Günstigstes bereits bewertetes Deck eines Archetyps.
    fn best_of_arch(&self, arch: u32) -> Option<Summary> {
        self.decks
            .get(&arch)?
            .iter()
            .filter_map(|d| self.summary(d.id))
            .min_by_key(|s| cost_key(s, self.wildcards()))
    }

    // ---------------------------------------------------------------- UI ----

    fn top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.heading("MTG Arena Builder");
                ui.separator();
                match &self.collection {
                    Some(c) => {
                        ui.label(format!("Sammlung: {} Karten ({})", c.total_cards(), c.age_text()));
                        ui.separator();
                        ui.label("Wildcards:");
                        for (i, n) in c.wildcards.as_array().iter().enumerate() {
                            ui.label(
                                RichText::new(format!("{} {n}", RARITY_NAMES[i]))
                                    .color(rarity_color(i as u8 + RARITY_COMMON))
                                    .strong(),
                            );
                        }
                    }
                    None => {
                        ui.label(RichText::new("Sammlung noch nicht gelesen").color(YELLOW));
                    }
                }
                ui.separator();
                let button = egui::Button::new(if self.syncing { "Lese …" } else { "Sammlung aus Arena lesen" });
                let hint = "MTG Arena muss laufen und im Hauptmenü sein. Falls Windows nach Administratorrechten fragt: Die werden nur gebraucht, um den Speicher des Spiels zu lesen.";
                if ui.add_enabled(!self.syncing, button).on_hover_text(hint).clicked() {
                    self.sync_collection(ctx);
                }
                ui.separator();
                if ui
                    .toggle_value(&mut self.settings.companion, "Begleitfenster")
                    .on_hover_text(
                        "Kleines Fenster, das während einer Partie live Hinweise gibt (Land legen, gefahrlos angreifen, blocken …). \
                         Es liest nur das Arena-Log. Arena dafür im Fenstermodus oder randlosen Vollbild spielen.",
                    )
                    .changed()
                {
                    self.save_settings();
                }
            });
            ui.add_space(4.0);
        });
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if self.pending > 0 || self.syncing {
                    ui.spinner();
                }
                if self.pending > 0 {
                    ui.label(format!("{} Abrufe in der Warteschlange ·", self.pending));
                }
                ui.label(&self.status);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(&self.db_status).weak());
                });
            });
        });
    }

    fn side_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("archetypes").default_width(310.0).show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label("Format:");
                let before = self.settings.format;
                egui::ComboBox::from_id_salt("format")
                    .selected_text(FORMATS[self.settings.format].1)
                    .show_ui(ui, |ui| {
                        for (i, f) in FORMATS.iter().enumerate() {
                            ui.selectable_value(&mut self.settings.format, i, f.1);
                        }
                    });
                if before != self.settings.format {
                    self.save_settings();
                    self.load_format(false);
                }
                if ui.button("Aktualisieren").clicked() {
                    self.load_format(true);
                }
            });
            ui.horizontal(|ui| {
                let todo: Vec<Archetype> =
                    self.archetypes.iter().filter(|a| !self.decks.contains_key(&a.id)).cloned().collect();
                let hint = "Lädt zu jedem Archetyp das neueste Turnierdeck und prüft es gegen deine Sammlung. Dauert ca. 10 s pro Archetyp (MTGTop8 erlaubt nur eine Anfrage alle 5 s); Ergebnisse werden zwischengespeichert.";
                if ui.add_enabled(!todo.is_empty(), egui::Button::new("Alle bewerten")).on_hover_text(hint).clicked() {
                    self.status = format!("Bewerte {} Archetypen …", todo.len());
                    for arch in todo {
                        let _ = self.tx_job.send(Job::Decks { format: self.settings.format, arch, urgent: false });
                    }
                }
                ui.checkbox(&mut self.sort_by_fit, "Günstigste zuerst")
                    .on_hover_text("Sortiert nach benötigten Wildcards (Rares und Mythics zuerst).");
            });
            ui.separator();

            let mut rows: Vec<(Archetype, Option<Summary>)> =
                self.archetypes.iter().map(|a| (a.clone(), self.best_of_arch(a.id))).collect();
            if self.sort_by_fit {
                let wc = self.wildcards();
                rows.sort_by_key(|(a, s)| (s.is_none(), s.map(|s| cost_key(&s, wc)), std::cmp::Reverse(a.share)));
            }
            let mut clicked = None;
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                if rows.is_empty() {
                    ui.label(RichText::new("Noch keine Archetypen geladen.").weak());
                }
                for (arch, best) in &rows {
                    ui.horizontal(|ui| {
                        let text = format!("{:>2} %  {}", arch.share, arch.name);
                        if ui.selectable_label(self.sel_arch == Some(arch.id), text).clicked() {
                            clicked = Some(arch.clone());
                        }
                        if let Some(s) = best {
                            let (color, text) = self.fit_text(s);
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(RichText::new(text).color(color)).on_hover_text(format!(
                                    "Günstigste bisher geladene Liste\n{}",
                                    cost_tooltip(s)
                                ));
                            });
                        }
                    });
                }
                ui.add_space(10.0);
                ui.collapsing("Einstellungen", |ui| {
                    ui.label("MTGA-Installationsordner (leer = automatisch):");
                    ui.text_edit_singleline(&mut self.settings.mtga_dir);
                    if ui.button("Kartendaten neu laden").clicked() {
                        self.save_settings();
                        self.load_db();
                    }
                });
            });
            if let Some(arch) = clicked {
                self.tab = Tab::Meta;
                self.sel_arch = Some(arch.id);
                self.sel_deck = self.decks.get(&arch.id).and_then(|d| d.first()).map(|d| d.id);
                if self.decks.contains_key(&arch.id) {
                    self.queue_variants(arch.id);
                } else {
                    self.status = format!("Lade Decks für {} …", arch.name);
                    let _ = self.tx_job.send(Job::Decks { format: self.settings.format, arch, urgent: true });
                }
            }
        });
    }

    fn meta_tab(&mut self, ui: &mut egui::Ui) {
        let Some(arch_id) = self.sel_arch else {
            ui.add_space(20.0);
            ui.label("Links einen Archetyp auswählen – oder „Alle bewerten“, um zu sehen, welche Turnierdecks du schon (fast) bauen kannst.");
            return;
        };
        let name = self
            .archetypes
            .iter()
            .find(|a| a.id == arch_id)
            .map(|a| a.name.clone())
            .unwrap_or_default();
        ui.heading(name);
        let Some(decks) = self.decks.get(&arch_id).cloned() else {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Lade Turnierdecks …");
            });
            return;
        };
        if decks.is_empty() {
            ui.label("Für diesen Archetyp wurden keine Decks gefunden.");
            return;
        }

        let wc = self.wildcards();
        let summaries: HashMap<u32, Summary> = decks
            .iter()
            .filter_map(|d| Some((d.id, self.summary(d.id)?)))
            .collect();
        let cheapest = summaries
            .iter()
            .min_by_key(|(id, s)| (cost_key(s, wc), std::cmp::Reverse(**id)))
            .map(|(id, _)| *id);
        if summaries.len() < decks.len() {
            ui.label(
                RichText::new(format!(
                    "{} von {} Listen bewertet – die übrigen werden im Hintergrund geladen.",
                    summaries.len(),
                    decks.len()
                ))
                .weak(),
            );
        }
        let mut clicked = None;
        ui.push_id("decks", |ui| {
            TableBuilder::new(ui)
                .striped(true)
                .max_scroll_height(150.0)
                .column(Column::auto().at_least(170.0))
                .column(Column::remainder().at_least(180.0).clip(true))
                .column(Column::auto().at_least(50.0))
                .column(Column::auto().at_least(70.0))
                .column(Column::auto().at_least(190.0))
                .header(20.0, |mut h| {
                    for title in ["Spieler", "Turnier", "Platz", "Datum", "Wildcards nötig"] {
                        h.col(|ui| {
                            ui.strong(title);
                        });
                    }
                })
                .body(|mut body| {
                    for d in &decks {
                        body.row(20.0, |mut row| {
                            row.col(|ui| {
                                if ui
                                    .selectable_label(self.sel_deck == Some(d.id), &d.player)
                                    .clicked()
                                {
                                    clicked = Some(d.id);
                                }
                            });
                            row.col(|ui| {
                                ui.label(&d.event);
                            });
                            row.col(|ui| {
                                ui.label(&d.rank);
                            });
                            row.col(|ui| {
                                ui.label(&d.date);
                            });
                            row.col(|ui| match summaries.get(&d.id) {
                                Some(s) => {
                                    let (color, mut text) = self.fit_text(s);
                                    if cheapest == Some(d.id) && summaries.len() > 1 {
                                        text.push_str("  · am günstigsten");
                                    }
                                    ui.label(RichText::new(text).color(color))
                                        .on_hover_text(cost_tooltip(s));
                                }
                                None => {
                                    ui.label(RichText::new("nicht geladen").weak());
                                }
                            });
                        });
                    }
                });
        });
        if let Some(id) = clicked {
            self.sel_deck = Some(id);
            if !self.lists.contains_key(&id) {
                self.list_requested.insert(id);
                let _ = self.tx_job.send(Job::List {
                    deck_id: id,
                    urgent: true,
                });
            }
        }
        ui.separator();
        match self.sel_deck {
            Some(id) if self.evals.contains_key(&id) => self.deck_view(ui, id),
            Some(_) => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Lade Deckliste …");
                });
            }
            None => {}
        }
    }

    /// Baut Deckvorschläge aus der Sammlung und legt sie unter eigenen Schlüsseln ab.
    fn compute_built(&mut self) {
        let Some(db) = &self.db else { return };
        // Turnier-Präsenz: in wie vielen geladenen Turnierlisten kommt eine Karte vor?
        let mut meta_use: HashMap<String, u32> = HashMap::new();
        for (_, eval) in self.evals.iter().filter(|(id, _)| **id > BUILT_MAX) {
            let keys: HashSet<&String> = eval.lines.iter().filter_map(|l| l.key.as_ref()).collect();
            for key in keys {
                *meta_use.entry(key.clone()).or_insert(0) += 1;
            }
        }
        let legal = self.legal.get(&self.settings.format);
        let built = builder::build_all(db, &self.owned, legal, &meta_use);
        self.repl
            .retain(|k, _| !(BUILT_BASE..=BUILT_MAX).contains(&k.0));
        self.subs
            .retain(|k, _| !(BUILT_BASE..=BUILT_MAX).contains(&k.0));
        for (i, b) in built.iter().enumerate() {
            self.set_list(BUILT_BASE + i as u32, b.deck.clone());
        }
        self.sel_built = self.sel_built.min(built.len().saturating_sub(1));
        self.built = Some(built);
    }

    fn built_tab(&mut self, ui: &mut egui::Ui) {
        if self.collection.is_none() || self.db.is_none() {
            ui.label("Dafür muss zuerst die Sammlung gelesen werden (oben rechts).");
            return;
        }
        if self.built.is_none() {
            self.compute_built();
        }
        ui.label(format!(
            "Decks, die du sofort und ohne Wildcards bauen kannst – zusammengestellt aus deinen stärksten {}-legalen Karten je Farbkombination.",
            FORMATS[self.settings.format].1
        ));
        ui.label(
            RichText::new("Die Kartenstärke ist geschätzt (Seltenheit, Vorkommen in Turnierdecks, Werte, Removal). Je mehr Turnierdecks links geladen sind, desto besser die Schätzung.")
                .weak(),
        );
        let built = self.built.clone().unwrap_or_default();
        if built.is_empty() {
            ui.label(RichText::new("Aus deiner Sammlung lässt sich in diesem Format noch kein vollständiges Deck bauen.").color(YELLOW));
            return;
        }
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            for (i, b) in built.iter().enumerate() {
                let text = format!("{}  {:.1}", b.label, b.score);
                if ui
                    .selectable_label(self.sel_built == i, text)
                    .on_hover_text(
                        "Geschätzte Stärke (höher = besser), nur untereinander vergleichbar",
                    )
                    .clicked()
                {
                    self.sel_built = i;
                }
            }
            if ui.button("Neu berechnen").clicked() {
                self.built = None;
            }
        });
        let Some(b) = built.get(self.sel_built) else {
            return;
        };
        ui.label(format!(
            "{}: {} Kreaturen · {} Removal · Ø {:.1} Mana · {} Rares/Mythics · {} Karten, die auch in Turnierdecks gespielt werden",
            b.label, b.creatures, b.removal, b.avg_cmc, b.rares, b.meta_cards
        ));
        ui.separator();
        self.deck_view(ui, BUILT_BASE + self.sel_built as u32);
    }

    fn paste_tab(&mut self, ui: &mut egui::Ui) {
        ui.label("Deckliste im Arena- oder MTGO-Textformat einfügen (z. B. von MTGGoldfish, Untapped oder aus Arena exportiert):");
        egui::ScrollArea::vertical()
            .id_salt("paste")
            .max_height(140.0)
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut self.paste_text)
                        .desired_rows(6)
                        .desired_width(f32::INFINITY)
                        .hint_text("4 Watery Grave\n4 Island\n…\n\nSideboard\n2 Duress"),
                );
            });
        if ui.button("Prüfen").clicked() {
            self.repl.retain(|k, _| k.0 != PASTED);
            self.subs.retain(|k, _| k.0 != PASTED);
            self.set_list(PASTED, deck::parse(&self.paste_text));
        }
        ui.separator();
        if self.evals.get(&PASTED).is_some_and(|e| !e.lines.is_empty()) {
            self.deck_view(ui, PASTED);
        }
    }

    /// Berechnet (einmalig) Ersatzvorschläge für alle fehlenden Karten eines Decks.
    fn ensure_suggestions(&mut self, deck_id: u32, eval: &DeckEval) {
        let Some(db) = &self.db else { return };
        if self.owned.is_empty() {
            return;
        }
        let in_deck: HashSet<String> = eval.lines.iter().filter_map(|l| l.key.clone()).collect();
        let colors = in_deck
            .iter()
            .filter_map(|k| db.representative(k))
            .filter(|c| !c.is_land())
            .fold(0u8, |m, c| m | c.colors);
        let legal = self.legal.get(&self.settings.format);
        for line in eval.lines.iter().filter(|l| l.missing > 0) {
            let Some(key) = &line.key else { continue };
            let slot = (deck_id, key.clone());
            if !self.subs.contains_key(&slot) {
                let found = similar::suggest(db, &self.owned, legal, colors, &in_deck, key, 6);
                self.subs.insert(slot, found);
            }
        }
    }

    fn deck_view(&mut self, ui: &mut egui::Ui, deck_id: u32) {
        let Some(eval) = self.evals.get(&deck_id).cloned() else {
            return;
        };
        self.ensure_suggestions(deck_id, &eval);
        let summary = eval.summary(self.settings.include_side, |l| {
            self.replaced_qty(deck_id, l)
        });
        let wc = self.wildcards();

        // --- Zusammenfassung ---
        ui.horizontal_wrapped(|ui| {
            if self.collection.is_none() {
                ui.label(
                    RichText::new("Ohne gelesene Sammlung gilt alles als fehlend.").color(YELLOW),
                );
            } else if summary.complete() {
                ui.label(
                    RichText::new("Du kannst dieses Deck komplett bauen.")
                        .color(GREEN)
                        .strong(),
                );
            } else if summary.craftable(wc) {
                ui.label(
                    RichText::new("Mit deinen Wildcards baubar.")
                        .color(YELLOW)
                        .strong(),
                );
            } else {
                ui.label(RichText::new("Noch nicht baubar.").color(RED).strong());
            }
            ui.label(format!(
                "{} von {} Karten vorhanden ({:.0} %)",
                summary.total - summary.missing,
                summary.total,
                summary.percent().floor()
            ));
        });
        if summary.missing > 0 {
            ui.horizontal_wrapped(|ui| {
                ui.label("Benötigte Wildcards:");
                for i in 0..4 {
                    if summary.need[i] > 0 {
                        let enough = summary.need[i] <= wc[i];
                        ui.label(
                            RichText::new(format!(
                                "{} {} (du hast {})",
                                summary.need[i], RARITY_NAMES[i], wc[i]
                            ))
                            .color(if enough { GREEN } else { RED }),
                        );
                    }
                }
                if summary.unavailable > 0 {
                    ui.label(
                        RichText::new(format!(
                            "{} Karten gibt es in Arena nicht",
                            summary.unavailable
                        ))
                        .color(RED),
                    );
                }
            });
        }
        let has_repl = self.repl.keys().any(|k| k.0 == deck_id);
        let mut budget_clicked = false;
        let mut reset_clicked = false;
        ui.horizontal(|ui| {
            if summary.missing > 0 || has_repl {
                budget_clicked = ui
                    .button("Budget-Version bauen")
                    .on_hover_text(
                        "Stellt die wichtigsten fehlenden Karten mit deinen Wildcards her und ersetzt den Rest                          durch ähnliche Karten aus deiner Sammlung. Ergebnis siehst du in der Spalte „Ersatz“.",
                    )
                    .clicked();
            }
            if has_repl {
                reset_clicked = ui.button("Ersetzungen zurücksetzen").clicked();
            }
            if ui.button("Für Arena kopieren").on_hover_text("In Arena: Decks → Importieren").clicked()
                && let Some(db) = &self.db {
                    let text = deck::export_arena(
                        &eval,
                        db,
                        self.collection.as_ref().map(|c| &c.cards),
                        self.settings.with_sets,
                        self.settings.include_side,
                        |line| {
                            let rep = self.repl.get(&(deck_id, line.key.clone()?))?;
                            Some((rep.clone(), self.replaced_qty(deck_id, line)))
                        },
                    );
                    ui.ctx().copy_text(text);
                    self.status = "Deck in die Zwischenablage kopiert – in Arena unter Decks → Importieren einfügen.".into();
                }
            let mut changed = ui.checkbox(&mut self.settings.include_side, "Sideboard einbeziehen").changed();
            changed |= ui
                .checkbox(&mut self.settings.with_sets, "Mit Set-Kürzeln exportieren")
                .on_hover_text("Empfohlen, wenn Arena nicht auf Englisch läuft.")
                .changed();
            if changed {
                self.save_settings();
            }
        });
        if reset_clicked {
            self.repl.retain(|k, _| k.0 != deck_id);
            self.status = "Ersetzungen zurückgesetzt.".into();
        }
        if budget_clicked {
            // Neu planen: bisherige Ersetzungen zählen dabei nicht.
            self.repl.retain(|k, _| k.0 != deck_id);
            self.apply_budget(deck_id, &eval);
        }
        ui.add_space(4.0);

        // --- Kartenliste ---
        let lines: Vec<&LineEval> = eval
            .lines
            .iter()
            .filter(|l| self.settings.include_side || !l.side)
            .collect();
        let mut new_choice: Option<(String, Option<String>)> = None;
        ui.push_id(("cards", deck_id), |ui| {
            TableBuilder::new(ui)
                .striped(true)
                .column(Column::auto().at_least(30.0))
                .column(Column::auto().at_least(260.0).clip(true))
                .column(Column::auto().at_least(80.0))
                .column(Column::auto().at_least(50.0))
                .column(Column::auto().at_least(45.0))
                .column(Column::remainder().at_least(220.0))
                .header(20.0, |mut h| {
                    for title in [
                        "Anz.",
                        "Karte",
                        "Seltenheit",
                        "Besitz",
                        "Fehlt",
                        "Ersatz aus deiner Sammlung",
                    ] {
                        h.col(|ui| {
                            ui.strong(title);
                        });
                    }
                })
                .body(|mut body| {
                    let mut side_started = false;
                    for line in lines {
                        if line.side && !side_started {
                            side_started = true;
                            body.row(24.0, |mut row| {
                                row.col(|_| {});
                                row.col(|ui| {
                                    ui.strong("Sideboard");
                                });
                                for _ in 0..4 {
                                    row.col(|_| {});
                                }
                            });
                        }
                        body.row(22.0, |mut row| {
                            self.card_row(&mut row, deck_id, line, &mut new_choice);
                        });
                    }
                });
        });
        if let Some((key, choice)) = new_choice {
            match choice {
                Some(rep) => self.repl.insert((deck_id, key), rep),
                None => self.repl.remove(&(deck_id, key)),
            };
        }
    }

    fn card_row(
        &self,
        row: &mut egui_extras::TableRow<'_, '_>,
        deck_id: u32,
        line: &LineEval,
        new_choice: &mut Option<(String, Option<String>)>,
    ) {
        let db = self.db.as_ref();
        let card = line.key.as_deref().and_then(|k| db?.representative(k));
        row.col(|ui| {
            ui.label(line.qty.to_string());
        });
        row.col(|ui| {
            let label = ui.label(&line.name);
            if let Some(c) = card {
                let de = c.name_de.as_deref().filter(|d| *d != c.name);
                let info = format!(
                    "{}{}  ·  {} {}",
                    de.map(|d| format!("{d}\n")).unwrap_or_default(),
                    c.mana_cost,
                    c.set,
                    c.collector
                );
                self.card_hover(label, &c.name, Some(info));
            }
        });
        row.col(|ui| {
            if line.key.is_some() {
                ui.label(RichText::new(rarity_label(line.rarity)).color(rarity_color(line.rarity)));
            }
        });
        row.col(|ui| {
            ui.label(if line.basic {
                "∞".to_string()
            } else {
                line.owned.to_string()
            });
        });
        row.col(|ui| {
            if line.missing > 0 {
                ui.label(RichText::new(line.missing.to_string()).color(RED).strong());
            }
        });
        row.col(|ui| {
            if line.missing == 0 {
                return;
            }
            let Some(key) = &line.key else {
                ui.label(RichText::new("nicht in Arena verfügbar").color(RED));
                return;
            };
            let slot = (deck_id, key.clone());
            let options = self.subs.get(&slot).map(Vec::as_slice).unwrap_or(&[]);
            if options.is_empty() {
                ui.label(RichText::new("kein Vorschlag").weak());
                return;
            }
            let current = self.repl.get(&slot);
            let shown = current
                .and_then(|k| options.iter().find(|o| &o.key == k))
                .map(|o| o.name.clone())
                .unwrap_or_else(|| format!("– ({} Vorschläge)", options.len()));
            egui::ComboBox::from_id_salt(("rep", deck_id, key, line.side))
                .selected_text(shown)
                .width(210.0)
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(current.is_none(), "– (Wildcard nutzen)")
                        .clicked()
                    {
                        *new_choice = Some((key.clone(), None));
                    }
                    for o in options {
                        let text = format!("{}  {}  ({}×)", o.name, o.info, o.owned);
                        if ui.selectable_label(current == Some(&o.key), text).clicked() {
                            *new_choice = Some((key.clone(), Some(o.key.clone())));
                        }
                    }
                });
        });
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
        self.side_panel(ctx);
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.db.is_none() {
                ui.label(RichText::new(&self.db_status).color(RED));
                ui.label("Den Ordner kannst du links unter „Einstellungen“ angeben.");
                ui.separator();
            }
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.tab, Tab::Meta, "Turnierdecks");
                ui.selectable_value(&mut self.tab, Tab::Built, "Aus meiner Sammlung");
                ui.selectable_value(&mut self.tab, Tab::Paste, "Eigene Liste prüfen");
                ui.selectable_value(&mut self.tab, Tab::Games, "Meine Partien");
                ui.selectable_value(&mut self.tab, Tab::DeckCheck, "Deck verbessern");
            });
            ui.separator();
            match self.tab {
                Tab::Meta => self.meta_tab(ui),
                Tab::Paste => self.paste_tab(ui),
                Tab::Built => self.built_tab(ui),
                Tab::Games => self.games_tab(ui),
                Tab::DeckCheck => self.deck_check_tab(ui),
            }
        });
    }
}

// ------------------------------------------------------- Meine Partien ----

fn event_label(event: &str) -> String {
    match event {
        "Ladder" => "Rangliste".into(),
        "Traditional_Ladder" => "Rangliste Bo3".into(),
        "Play" => "Spielen".into(),
        "Traditional_Play" => "Spielen Bo3".into(),
        "DualColorPrecons" => "Starterdecks".into(),
        "" => "?".into(),
        e if e.starts_with("WelcomeDeckDuels") => "Willkommens-Duelle".into(),
        e if e.starts_with("ColorChallenge") => "Farb-Challenge".into(),
        e => e.replace('_', " "),
    }
}

fn ago(ms: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let secs = now.saturating_sub(ms) / 1000;
    match secs {
        0..=89 => "gerade eben".into(),
        90..=5399 => format!("vor {} Min.", secs / 60),
        5400..=172_799 => format!("vor {} Std.", secs / 3600),
        _ => format!("vor {} Tagen", secs / 86_400),
    }
}

fn ratio(won: usize, total: usize) -> String {
    if total == 0 {
        "–".into()
    } else {
        format!("{won} von {total} gewonnen")
    }
}

impl App {
    fn games_tab(&mut self, ui: &mut egui::Ui) {
        // Beim Öffnen des Reiters automatisch nachladen (höchstens einmal pro Minute).
        if self
            .games_scanned
            .is_none_or(|t| t.elapsed().as_secs() > 60)
        {
            self.scan_games();
        }
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!self.games_loading, egui::Button::new("Logs neu einlesen"))
                .clicked()
            {
                self.scan_games();
            }
            if self.games_loading {
                ui.spinner();
            }
            let mut events: Vec<String> = self.games.iter().map(|g| g.event.clone()).collect();
            events.sort();
            events.dedup();
            let shown = self
                .games_event
                .as_deref()
                .map(event_label)
                .unwrap_or_else(|| "Alle Modi".into());
            let before = self.games_event.clone();
            egui::ComboBox::from_id_salt("games_event")
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.games_event, None, "Alle Modi");
                    for e in events {
                        let label = event_label(&e);
                        ui.selectable_value(&mut self.games_event, Some(e), label);
                    }
                });
            if before != self.games_event {
                self.sel_game = None;
            }
            ui.label(RichText::new(format!("{} Partien gespeichert", self.games.len())).weak());
        });
        if self.logs_disabled {
            ui.label(
                RichText::new(
                    "In Arena sind „Detaillierte Logs“ ausgeschaltet – neue Partien werden nicht mitgeschrieben. \
                     Einschalten: Zahnrad → Konto → „Detaillierte Logs (Plugin-Unterstützung)“, danach Arena neu starten.",
                )
                .color(YELLOW),
            );
        }
        let Some(db) = &self.db else { return };
        let games: Vec<GameRecord> = self
            .games
            .iter()
            .filter(|g| self.games_event.as_ref().is_none_or(|e| &g.event == e))
            .cloned()
            .collect();
        if games.is_empty() {
            ui.add_space(10.0);
            ui.label("Noch keine Partien. Spiel mit eingeschalteten „Detaillierten Logs“ ein paar Runden – die App liest sie dann hier ein.");
            return;
        }
        let r = games::report(&games, db);
        let meta = self.meta_archetypes();
        let opp_groups = opponents::analyze(&games, db, &meta);
        let opp_names: Vec<String> = games
            .iter()
            .map(|g| opponents::classify(g, db, &meta))
            .collect();
        let mut clicked: Option<usize> = None;

        egui::ScrollArea::vertical().id_salt("games_scroll").auto_shrink([false, false]).show(ui, |ui| {
            // --- Bilanz ---
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                let color = if r.wins >= r.losses { GREEN } else { RED };
                ui.label(RichText::new(format!("{} Siege, {} Niederlagen", r.wins, r.losses)).color(color).strong());
                ui.label(format!("aus {} Partien", r.games));
                if r.conceded > 0 {
                    ui.label(format!("· {} davon aufgegeben", r.conceded));
                }
            });
            ui.label(format!(
                "Als Erster: {} · Als Zweiter: {} · Partien dauern im Schnitt {:.0} Züge",
                ratio(r.on_play.0, r.on_play.1),
                ratio(r.on_draw.0, r.on_draw.1),
                r.avg_turns
            ));
            ui.add_space(6.0);

            // --- Größte Baustellen ---
            ui.heading("Was du als Nächstes verbessern kannst");
            let top: Vec<&games::Category> =
                r.categories.iter().filter(|c| !c.items.is_empty()).take(3).collect();
            if top.is_empty() {
                ui.label(
                    RichText::new("Keine typischen Fehler gefunden. Die App erkennt allerdings nur einfache Muster.")
                        .color(GREEN),
                );
            }
            for (n, c) in top.iter().enumerate() {
                let in_games: HashSet<usize> = c.items.iter().map(|f| f.game).collect();
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(format!("{}. {}", n + 1, c.title)).strong().color(YELLOW));
                    ui.label(format!("– {}× in {} von {} Partien", c.items.len(), in_games.len(), r.games));
                });
                ui.label(c.advice);
                ui.add_space(4.0);
            }
            if r.good_keeps.1 + r.bad_keeps.1 > 0 {
                ui.label(
                    RichText::new(format!(
                        "Starthand mit 2–4 Ländern: {} · andere Starthände: {}",
                        ratio(r.good_keeps.0, r.good_keeps.1),
                        ratio(r.bad_keeps.0, r.bad_keeps.1)
                    ))
                    .weak(),
                );
            }
            ui.add_space(8.0);

            // --- Alle Auffälligkeiten ---
            ui.strong("Alle Auffälligkeiten");
            ui.label(
                RichText::new(format!(
                    "Zug-Auswertung über {} eigene Züge aus vollständig mitgeschriebenen Partien. Ein Klick auf einen Eintrag öffnet die Partie.",
                    r.my_turns
                ))
                .weak(),
            );
            for c in &r.categories {
                let color = if c.items.is_empty() { GREEN } else { YELLOW };
                egui::CollapsingHeader::new(RichText::new(format!("{}: {}×", c.title, c.items.len())).color(color))
                    .id_salt(c.title)
                    .show(ui, |ui| {
                        ui.label(RichText::new(c.advice).weak());
                        for f in &c.items {
                            let when = if f.turn == 0 { "Starthand".to_string() } else { format!("Zug {}", f.turn) };
                            if ui.link(format!("Partie #{}, {when}: {}", f.game, f.text)).clicked() {
                                clicked = Some(f.game - 1);
                            }
                        }
                    });
            }
            if r.discards > 0 {
                ui.label(format!("Abgeworfene Karten: {} (z. B. weil die Hand am Zugende zu voll war)", r.discards));
            }
            egui::CollapsingHeader::new(format!("Was dir am meisten Schaden gemacht hat ({})", r.damage_sources.len()))
                .id_salt("damage_sources")
                .show(ui, |ui| {
                    ui.label(
                        RichText::new("Gegen diese Karten lohnt es sich, Removal aufzuheben oder zu blocken.").weak(),
                    );
                    for (name, dmg) in &r.damage_sources {
                        let resp = ui.label(format!("{name} – {dmg} Schaden"));
                        self.card_hover(resp, name, None);
                    }
                });
            egui::CollapsingHeader::new(format!("Häufigste Karten deiner Gegner ({})", r.opp_cards.len()))
                .id_salt("opp_cards")
                .show(ui, |ui| {
                    for (name, n) in &r.opp_cards {
                        let resp = ui.label(format!("{name} – in {n} Partie(n)"));
                        self.card_hover(resp, name, None);
                    }
                });
            ui.add_space(6.0);
            ui.separator();

            // --- Gegner ---
            egui::CollapsingHeader::new(RichText::new("Gegen wen du gewinnst und verlierst").strong())
                .id_salt("opponents")
                .default_open(true)
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(
                            "Das gegnerische Deck erkennt die App an den Karten, die du gesehen hast: passend zu einem                              geladenen Turnier-Archetyp („erkannt“), sonst nach Farben.",
                        )
                        .weak(),
                    );
                    egui::Grid::new("opp_grid").striped(true).num_columns(4).show(ui, |ui| {
                        for t in ["Gegner", "Bilanz", "Ø Züge", "Hat dir den meisten Schaden gemacht"] {
                            ui.strong(t);
                        }
                        ui.end_row();
                        for o in &opp_groups {
                            ui.label(&o.name);
                            let color = if o.wins >= o.losses() { GREEN } else { RED };
                            ui.label(RichText::new(format!("{} Siege, {} Niederlagen", o.wins, o.losses())).color(color));
                            ui.label(format!("{:.0}", o.avg_turns));
                            let threats: Vec<String> =
                                o.threats.iter().map(|(n, d)| format!("{n} ({d})")).collect();
                            ui.label(if threats.is_empty() { "–".into() } else { threats.join(", ") });
                            ui.end_row();
                        }
                    });
                });
            ui.add_space(6.0);

            // --- Partienliste ---
            ui.push_id("games_table", |ui| {
                TableBuilder::new(ui)
                    .striped(true)
                    .vscroll(false)
                    .column(Column::auto().at_least(34.0))
                    .column(Column::auto().at_least(80.0))
                    .column(Column::auto().at_least(120.0))
                    .column(Column::auto().at_least(90.0))
                    .column(Column::auto().at_least(70.0))
                    .column(Column::auto().at_least(40.0))
                    .column(Column::auto().at_least(40.0))
                    .column(Column::auto().at_least(60.0))
                    .column(Column::auto().at_least(50.0))
                    .column(Column::remainder().at_least(50.0))
                    .header(20.0, |mut h| {
                        for title in ["#", "Wann", "Modus", "Ergebnis", "Anfang", "Mull.", "Züge", "Leben", "Deck", "Gegner"] {
                            h.col(|ui| {
                                ui.strong(title);
                            });
                        }
                    })
                    .body(|mut body| {
                        for (i, g) in games.iter().enumerate() {
                            body.row(20.0, |mut row| {
                                row.col(|ui| {
                                    if ui.selectable_label(self.sel_game == Some(i), format!("#{}", i + 1)).clicked() {
                                        clicked = Some(i);
                                    }
                                });
                                row.col(|ui| {
                                    ui.label(ago(g.started_ms));
                                });
                                row.col(|ui| {
                                    ui.label(event_label(&g.event));
                                });
                                row.col(|ui| {
                                    let (text, color) = match g.won {
                                        Some(true) => ("Sieg", GREEN),
                                        Some(false) if g.conceded() => ("Aufgegeben", RED),
                                        Some(false) => ("Niederlage", RED),
                                        None => ("?", Color32::GRAY),
                                    };
                                    ui.label(RichText::new(text).color(color));
                                });
                                row.col(|ui| {
                                    ui.label(match g.on_play {
                                        Some(true) => "Erster",
                                        Some(false) => "Zweiter",
                                        None => "?",
                                    });
                                });
                                row.col(|ui| {
                                    ui.label(g.mulligans.to_string());
                                });
                                row.col(|ui| {
                                    ui.label(if g.turns.is_empty() { "–".into() } else { g.last_turn().to_string() });
                                });
                                row.col(|ui| {
                                    let (me, opp) = g.final_life();
                                    ui.label(format!("{me} : {opp}"));
                                });
                                row.col(|ui| {
                                    ui.label(games::deck_colors(&g.deck, db));
                                });
                                row.col(|ui| {
                                    ui.label(&opp_names[i]);
                                });
                            });
                        }
                    });
            });

            // --- Zugdetails ---
            if let Some(g) = self.sel_game.and_then(|i| games.get(i).map(|g| (i, g))) {
                let (i, g) = g;
                ui.add_space(8.0);
                ui.separator();
                ui.heading(format!("Partie #{} – {}", i + 1, event_label(&g.event)));
                if !g.opening_hand.is_empty() {
                    let hand: Vec<&crate::carddb::Card> =
                        g.opening_hand.iter().filter_map(|grp| db.cards.get(grp)).collect();
                    let lands = hand.iter().filter(|c| c.is_land()).count();
                    let names: Vec<&str> = hand.iter().map(|c| c.name.as_str()).collect();
                    ui.label(format!("Starthand ({} Karten, {lands} Länder): {}", hand.len(), names.join(", ")));
                }
                // Tipps nur für diese Partie
                let single = games::report(std::slice::from_ref(g), db);
                let found: Vec<(&str, &games::Finding)> = single
                    .categories
                    .iter()
                    .flat_map(|c| c.items.iter().map(move |f| (c.title, f)))
                    .collect();
                if found.is_empty() {
                    if g.complete {
                        ui.label(
                            RichText::new("In dieser Partie hat die App keine typischen Fehler gefunden.").color(GREEN),
                        );
                    }
                } else {
                    ui.strong("Was du in dieser Partie besser machen kannst");
                    let mut sorted = found;
                    sorted.sort_by_key(|(_, f)| f.turn);
                    for (title, f) in sorted {
                        let when = if f.turn == 0 { "Starthand".to_string() } else { format!("Zug {}", f.turn) };
                        ui.label(format!("• {when} – {title}: {}", f.text));
                    }
                }
                if let Some((name, dmg)) = {
                    let mut d: Vec<(u32, u32)> = g.damage_taken.iter().map(|(k, v)| (*k, *v)).collect();
                    d.sort_by_key(|(_, v)| std::cmp::Reverse(*v));
                    d.first().and_then(|(grp, v)| db.cards.get(grp).map(|c| (c.name.clone(), *v)))
                } {
                    ui.label(RichText::new(format!("Am meisten Schaden an dir: {name} ({dmg})")).weak());
                }
                ui.add_space(4.0);
                if g.turns.is_empty() {
                    ui.label("Für diese Partie stehen keine Zugdaten im Log.");
                    return;
                }
                if !g.complete {
                    ui.label(
                        RichText::new(format!(
                            "Erst ab Zug {} mitgeschrieben – Länder und Handkarten aus der Zeit davor fehlen.",
                            g.turns.first().map(|t| t.turn).unwrap_or(0)
                        ))
                        .color(YELLOW),
                    );
                }
                ui.label(RichText::new("Stand jeweils am Ende des Zuges. „Frei“ = ungetappte eigene Länder.").weak());
                ui.push_id(("turns", i), |ui| {
                    TableBuilder::new(ui)
                        .striped(true)
                        .vscroll(false)
                        .column(Column::auto().at_least(36.0))
                        .column(Column::auto().at_least(56.0))
                        .column(Column::auto().at_least(64.0))
                        .column(Column::auto().at_least(56.0))
                        .column(Column::auto().at_least(70.0))
                        .column(Column::auto().at_least(36.0))
                        .column(Column::auto().at_least(40.0))
                        .column(Column::auto().at_least(40.0))
                        .column(Column::auto().at_least(46.0))
                        .column(Column::remainder().at_least(50.0))
                        .header(20.0, |mut h| {
                            for title in ["Zug", "Wer", "Leben", "Länder", "Kreaturen", "Frei", "Hand", "Land", "Zauber", "Angriff"] {
                                h.col(|ui| {
                                    ui.strong(title);
                                });
                            }
                        })
                        .body(|mut body| {
                            for t in &g.turns {
                                body.row(20.0, |mut row| {
                                    let who = if t.mine { RichText::new("ich").strong() } else { RichText::new("Gegner") };
                                    row.col(|ui| {
                                        ui.label(t.turn.to_string());
                                    });
                                    row.col(|ui| {
                                        ui.label(who);
                                    });
                                    row.col(|ui| {
                                        ui.label(format!("{} : {}", t.life_me, t.life_opp));
                                    });
                                    row.col(|ui| {
                                        ui.label(format!("{} : {}", t.lands_me, t.lands_opp));
                                    });
                                    row.col(|ui| {
                                        ui.label(format!("{} : {}", t.creatures_me, t.creatures_opp));
                                    });
                                    row.col(|ui| {
                                        if t.mine {
                                            ui.label(t.untapped_lands.to_string());
                                        }
                                    });
                                    row.col(|ui| {
                                        let names: Vec<String> = t
                                            .hand
                                            .iter()
                                            .map(|g| db.cards.get(g).map(|c| c.name.clone()).unwrap_or_else(|| "?".into()))
                                            .collect();
                                        let label = ui.label(t.hand.len().to_string());
                                        if !names.is_empty() {
                                            label.on_hover_text(names.join("\n"));
                                        }
                                    });
                                    row.col(|ui| {
                                        ui.label(if t.land_played { "ja" } else { "–" });
                                    });
                                    row.col(|ui| {
                                        ui.label(t.spells.to_string());
                                    });
                                    row.col(|ui| {
                                        ui.label(if t.attacked { "ja" } else { "–" });
                                    });
                                });
                            }
                        });
                });
            }
        });
        if let Some(i) = clicked {
            self.sel_game = Some(i);
        }
    }
}

// ------------------------------------------------------ Deck verbessern ----

impl App {
    /// Öffnet bzw. pflegt das Begleitfenster (eigenes Fenster, immer im Vordergrund).
    fn companion(&mut self, ctx: &egui::Context) {
        if !self.settings.companion {
            return;
        }
        let id = egui::ViewportId::from_hash_of("companion");
        if self.live.is_none()
            && let Some(db) = &self.db
        {
            let shared = std::sync::Arc::new(live::Shared::default());
            live::spawn(db.clone(), shared.clone(), ctx.clone(), id);
            self.live = Some(shared);
        }
        let Some(shared) = self.live.clone() else {
            return;
        };
        // Turnierlisten für die Gegner-Erkennung weitergeben, wenn neue geladen wurden.
        if self.lists.len() != self.live_meta_lists
            && let Some(db) = &self.db
        {
            self.live_meta_lists = self.lists.len();
            let shares: Vec<live::ArchShare> = self
                .meta_archetypes()
                .into_iter()
                .filter(|a| !a.lists.is_empty())
                .map(|a| {
                    let n = a.lists.len() as f32;
                    let mut share: HashMap<String, f32> = HashMap::new();
                    for list in &a.lists {
                        let keys: HashSet<String> = list
                            .main
                            .iter()
                            .chain(&list.side)
                            .filter_map(|e| db.resolve(&e.name))
                            .collect();
                        for k in keys {
                            *share.entry(k).or_insert(0.0) += 1.0 / n;
                        }
                    }
                    live::ArchShare {
                        name: a.name,
                        share,
                    }
                })
                .collect();
            *shared.meta.write().unwrap() = shares;
        }
        if shared
            .close_requested
            .swap(false, std::sync::atomic::Ordering::Relaxed)
        {
            self.settings.companion = false;
            self.save_settings();
            return;
        }
        let mut builder = egui::ViewportBuilder::default()
            .with_title("MTG Begleiter")
            .with_inner_size([360.0, 330.0])
            .with_min_inner_size([260.0, 180.0])
            .with_always_on_top();
        if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png")) {
            builder = builder.with_icon(icon);
        }
        ctx.show_viewport_deferred(id, builder, move |ctx, _class| {
            if ctx.input(|i| i.viewport().close_requested()) {
                shared
                    .close_requested
                    .store(true, std::sync::atomic::Ordering::Relaxed);
            }
            egui::CentralPanel::default().show(ctx, |ui| {
                let view = shared.view.lock().unwrap().clone();
                live::draw(ui, &view);
            });
        });
    }

    /// Zeigt beim Überfahren eines Kartennamens das Kartenbild (lädt es bei Bedarf).
    fn card_hover(
        &self,
        resp: egui::Response,
        name: &str,
        extra: Option<String>,
    ) -> egui::Response {
        if resp.hovered()
            && !self.textures.contains_key(name)
            && !self.img_missing.contains(name)
            && self.img_requested.borrow_mut().insert(name.to_string())
        {
            let _ = self.img_tx.send(name.to_string());
        }
        resp.on_hover_ui(|ui| {
            if let Some(tex) = self.textures.get(name) {
                ui.add(
                    egui::Image::new((tex.id(), tex.size_vec2()))
                        .fit_to_exact_size(egui::vec2(244.0, 340.0)),
                );
            } else if self.img_missing.contains(name) {
                ui.label(RichText::new("Kein Bild verfügbar").weak());
            } else {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Bild wird geladen …");
                });
            }
            if let Some(text) = &extra {
                ui.label(text);
            }
        })
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

    fn deck_check_tab(&mut self, ui: &mut egui::Ui) {
        if self
            .games_scanned
            .is_none_or(|t| t.elapsed().as_secs() > 60)
        {
            self.scan_games();
        }
        let Some(db) = &self.db else { return };
        let groups = deckcheck::group_decks(&self.games, db);
        if groups.is_empty() {
            ui.add_space(10.0);
            ui.label(
                "Noch keine Partien mit bekannter Deckliste. Spiel mit eingeschalteten „Detaillierten Logs“ – \
                 die App erkennt das Deck dann automatisch.",
            );
            return;
        }
        let mut sel = self.sel_deck_group.min(groups.len() - 1);
        let mut adopt = false;
        ui.horizontal(|ui| {
            ui.label("Deck:");
            egui::ComboBox::from_id_salt("deck_group")
                .selected_text(groups[sel].label.clone())
                .width(420.0)
                .show_ui(ui, |ui| {
                    for (i, g) in groups.iter().enumerate() {
                        ui.selectable_value(&mut sel, i, g.label.clone());
                    }
                });
            adopt = ui
                .button("In „Eigene Liste prüfen“ übernehmen")
                .on_hover_text("Dort kannst du Karten gegen Ersatz aus deiner Sammlung tauschen und das Deck für Arena kopieren.")
                .clicked();
        });
        let group = &groups[sel];
        let meta = self.meta_archetypes();
        let legal = self.legal.get(&self.settings.format);
        let r = deckcheck::check(group, &self.games, db, &self.owned, legal, &meta);
        let section = |ui: &mut egui::Ui, title: &str| {
            ui.add_space(10.0);
            ui.heading(title);
        };

        egui::ScrollArea::vertical().id_salt("deck_check_scroll").auto_shrink([false, false]).show(ui, |ui| {
            if group.fixed {
                ui.label(
                    RichText::new(
                        "Dieses Deck gibt das Event vor – in Arena kannst du es nicht ändern. \
                         Die Auswertung zeigt trotzdem, wie du die Karten nutzt.",
                    )
                    .color(YELLOW),
                );
            }
            let color = if r.wins * 2 >= r.games { GREEN } else { RED };
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(format!("{} Siege aus {} Partien", r.wins, r.games)).color(color).strong());
                ui.label(format!("· {} davon mit Daten zu gezogenen und gespielten Karten", r.tracked));
            });
            if !r.show_winrates() {
                ui.label(
                    RichText::new(format!(
                        "Gewinnraten einzelner Karten zeigt die App ab {} Partien mit Kartendaten – bisher {}. \
                         Bis dahin zählt, wie du die Karten tatsächlich nutzt.",
                        deckcheck::MIN_GAMES_FOR_WINRATE,
                        r.tracked
                    ))
                    .weak(),
                );
            }

            // --- Mana ---
            section(ui, "Länder");
            ui.label(format!(
                "{} Länder bei {} Karten · zu wenig Länder in {} von {} Partien · zu viele in {}",
                r.mana.lands, r.mana.size, r.mana.screw, r.mana.games, r.mana.flood
            ));
            let ok = r.mana.advice.contains("passt") || r.mana.advice.contains("mindestens");
            ui.label(RichText::new(&r.mana.advice).color(if ok { Color32::GRAY } else { YELLOW }));

            if !group.fixed {
                // --- Tausch-Vorschläge ---
                section(ui, "Tausch-Vorschläge");
                if r.swaps.is_empty() {
                    ui.label(
                        "Noch keine klaren Kandidaten. Je mehr Partien du mit dem Deck spielst, desto genauer werden die Vorschläge.",
                    );
                }
                for sw in &r.swaps {
                    ui.horizontal_wrapped(|ui| {
                        let resp = ui.label(RichText::new(format!("Raus: {}", sw.out)).strong().color(YELLOW));
                        self.card_hover(resp, &sw.out, None);
                        ui.label(format!("– {}", sw.reason));
                    });
                    if let Some((name, why)) = &sw.replacement {
                        ui.horizontal_wrapped(|ui| {
                            ui.add_space(16.0);
                            let resp = ui.label(RichText::new(format!("Rein: {name}")).color(GREEN));
                            self.card_hover(resp, name, None);
                            ui.label(RichText::new(format!("({why})")).weak());
                        });
                    }
                }

                // --- Turniervergleich ---
                section(ui, "Vergleich mit Turnierdecks");
                match &r.meta {
                    Some(m) => {
                        ui.label(format!(
                            "Am ähnlichsten: {} ({} geladene Turnierlisten, {:.0} % Übereinstimmung der Nichtland-Karten)",
                            m.archetype,
                            m.lists,
                            m.overlap * 100.0
                        ));
                        if m.missing.is_empty() {
                            ui.label(RichText::new("Du spielst alle Karten, die die meisten dieser Turnierlisten spielen.").color(GREEN));
                        } else {
                            ui.label("Diese Karten spielt mindestens die Hälfte der Turnierlisten, du aber nicht:");
                            for (name, share, own) in &m.missing {
                                ui.horizontal(|ui| {
                                    ui.label(format!("• {name} – in {:.0} % der Listen", share * 100.0));
                                    if *own > 0 {
                                        ui.label(RichText::new(format!("du hast {own}")).color(GREEN));
                                    } else {
                                        ui.label(RichText::new("nicht in deiner Sammlung").weak());
                                    }
                                });
                            }
                        }
                    }
                    None => {
                        ui.label(
                            RichText::new(
                                "Kein passender Turnier-Archetyp geladen. Unter „Turnierdecks“ den Archetyp anklicken \
                                 oder „Alle bewerten“ – dann vergleicht die App hier.",
                            )
                            .weak(),
                        );
                    }
                }
            }

            // --- Karten ---
            section(ui, "Karten im Deck");
            ui.label(
                RichText::new(
                    "Gezogen/Gespielt = in wie vielen Partien. „Lange auf der Hand“ = mindestens drei eigene Züge \
                     bezahlbar, aber nicht gespielt. „Turnier“ = Anteil der Turnierlisten mit dieser Karte.",
                )
                .weak(),
            );
            let winrates = r.show_winrates();
            ui.push_id(("deck_cards", sel), |ui| {
                let mut table = TableBuilder::new(ui)
                    .striped(true)
                    .vscroll(false)
                    .column(Column::auto().at_least(34.0))
                    .column(Column::auto().at_least(230.0).clip(true))
                    .column(Column::auto().at_least(60.0))
                    .column(Column::auto().at_least(60.0))
                    .column(Column::auto().at_least(110.0))
                    .column(Column::auto().at_least(60.0))
                    .column(Column::auto().at_least(60.0));
                if winrates {
                    table = table.column(Column::auto().at_least(70.0));
                }
                table
                    .header(20.0, |mut h| {
                        let mut titles = vec!["Anz.", "Karte", "Gezogen", "Gespielt", "Lange auf Hand", "Schaden", "Turnier"];
                        if winrates {
                            titles.push("Siegquote");
                        }
                        for t in titles {
                            h.col(|ui| {
                                ui.strong(t);
                            });
                        }
                    })
                    .body(|mut body| {
                        for c in &r.cards {
                            body.row(20.0, |mut row| {
                                row.col(|ui| {
                                    ui.label(c.copies.to_string());
                                });
                                row.col(|ui| {
                                    let text = RichText::new(&c.name);
                                    let resp = ui.label(if c.land { text.weak() } else { text.color(rarity_color(c.rarity)) });
                                    self.card_hover(resp, &c.name, None);
                                });
                                let dash = |ui: &mut egui::Ui, show: bool, v: String| {
                                    ui.label(if show { v } else { "–".into() });
                                };
                                row.col(|ui| dash(ui, r.tracked > 0, c.seen.to_string()));
                                row.col(|ui| dash(ui, r.tracked > 0 && !c.land, c.cast.to_string()));
                                row.col(|ui| {
                                    if c.held > 0 {
                                        ui.label(RichText::new(c.held.to_string()).color(YELLOW));
                                    } else {
                                        dash(ui, r.tracked > 0 && !c.land, "0".into());
                                    }
                                });
                                row.col(|ui| dash(ui, c.damage > 0, c.damage.to_string()));
                                row.col(|ui| {
                                    dash(ui, c.meta_share.is_some(), format!("{:.0} %", c.meta_share.unwrap_or(0.0) * 100.0));
                                });
                                if winrates {
                                    row.col(|ui| {
                                        dash(
                                            ui,
                                            c.seen > 0,
                                            format!("{:.0} %", 100.0 * c.won_when_seen as f32 / c.seen.max(1) as f32),
                                        );
                                    });
                                }
                            });
                        }
                    });
            });
        });

        // Änderungen erst nach dem Zeichnen anwenden (Borrows auf self sind dann frei).
        let deck_text = adopt.then(|| deckcheck::deck_text(&group.deck, db));
        let match_id = r.meta.as_ref().map(|m| m.id);
        self.sel_deck_group = sel;
        if let Some(text) = deck_text {
            self.paste_text = text;
            self.repl.retain(|k, _| k.0 != PASTED);
            self.subs.retain(|k, _| k.0 != PASTED);
            self.set_list(PASTED, deck::parse(&self.paste_text));
            self.tab = Tab::Paste;
        }
        // Für den Vergleich alle Turnierlisten des passenden Archetyps nachladen.
        if let Some(id) = match_id {
            self.queue_variants(id);
        }
    }
}
