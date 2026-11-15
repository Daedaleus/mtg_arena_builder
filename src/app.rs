//! Oberfläche (egui) und Hintergrund-Abrufe.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};

use eframe::egui::{self, Color32, RichText};
use egui_extras::{Column, TableBuilder};
use serde::{Deserialize, Serialize};

use crate::carddb::{CardDb, RARITY_COMMON, rarity_label};
use crate::collection::{self, Collection};
use crate::deck::{self, Deck, DeckEval, LineEval, Summary};
use crate::meta::{Archetype, DeckRef, FORMATS, Fetcher};
use crate::paths;
use crate::similar::{self, Suggestion};

const GREEN: Color32 = Color32::from_rgb(110, 200, 120);
const YELLOW: Color32 = Color32::from_rgb(235, 195, 80);
const RED: Color32 = Color32::from_rgb(235, 115, 105);
const RARITY_NAMES: [&str; 4] = ["Common", "Uncommon", "Rare", "Mythic"];
/// Schlüssel der selbst eingefügten Liste in den Deck-Tabellen.
const PASTED: u32 = 0;

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
    format: usize,
    include_side: bool,
    with_sets: bool,
    sort_by_fit: bool,
    paste_tab: bool,
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
            format: 0,
            include_side: true,
            with_sets: true,
            sort_by_fit: false,
            paste_tab: false,
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
}

pub struct App {
    data_dir: PathBuf,
    settings: Settings,
    db: Option<CardDb>,
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
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> App {
        let data_dir = paths::data_dir();
        let settings: Settings = std::fs::read(data_dir.join("settings.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();

        let (tx_job, rx_job) = channel();
        let (tx_msg, rx_msg) = channel();
        {
            let (tx, ctx, cache) = (tx_msg.clone(), cc.egui_ctx.clone(), data_dir.join("cache"));
            std::thread::spawn(move || worker(rx_job, tx, ctx, cache));
        }

        let repl = settings
            .replacements
            .iter()
            .cloned()
            .map(|(deck, key, rep)| ((deck, key), rep))
            .collect();
        let mut app = App {
            sort_by_fit: settings.sort_by_fit,
            tab: if settings.paste_tab {
                Tab::Paste
            } else {
                Tab::Meta
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
        app
    }

    /// Schreibt Einstellungen und Sitzungszustand nach `settings.json`
    /// (nur wenn sich etwas geändert hat).
    fn save_settings(&mut self) {
        self.settings.sort_by_fit = self.sort_by_fit;
        self.settings.paste_tab = self.tab == Tab::Paste;
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
                self.db = Some(db);
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
        self.repl
            .get(&(deck_id, key.clone()))
            .map(|rep| self.owned.get(rep).copied().unwrap_or(0).min(line.missing))
            .unwrap_or(0)
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
        ui.horizontal(|ui| {
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
                label.on_hover_text(format!(
                    "{}{}\n{} {}",
                    de.map(|d| format!("{d}\n")).unwrap_or_default(),
                    c.mana_cost,
                    c.set,
                    c.collector
                ));
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
                ui.selectable_value(&mut self.tab, Tab::Paste, "Eigene Liste prüfen");
            });
            ui.separator();
            match self.tab {
                Tab::Meta => self.meta_tab(ui),
                Tab::Paste => self.paste_tab(ui),
            }
        });
    }
}
