//! Daten der Oberfläche: Laden, Listen, Bewertungen und Budget-Version.

use super::*;

impl App {
    /// Liest die Arena-Logs im Hintergrund ein und ergänzt `games.json`.
    /// Neue Sprache: zwischengespeicherte Texte (Probehände, eigene Decks)
    /// neu berechnen lassen.
    pub(super) fn on_language_changed(&mut self) {
        self.sims.decks.clear();
        self.sims.groups.clear();
        self.built.decks = None;
        self.games.cache = None;
        self.deck_cache = None;
    }

    pub(super) fn scan_games(&mut self) {
        if self.games.loading {
            return;
        }
        self.games.loading = true;
        self.games.scanned = Some(std::time::Instant::now());
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
    pub(super) fn save_settings(&mut self) {
        self.settings.sort_by_fit = self.sort_by_fit;
        self.settings.tab = self.tab.saved();
        self.settings.paste_text = self.paste_text.clone();
        self.settings.sel_arch = self.tour.sel_arch;
        self.settings.sel_deck = self.tour.sel_deck;
        self.settings.set_replacements(&self.repl);
        if let Err(e) = self.settings_file.save(&self.settings) {
            self.status = tr!("state.einstellungen_nicht_gespeichert", e = e);
        }
    }

    pub(super) fn load_db(&mut self) {
        let found =
            paths::find_card_db(Some(&self.settings.mtga_dir)).and_then(|p| CardDb::load(&p));
        match found {
            Ok(db) => {
                self.db_status = tr!(
                    "state.kartendaten_karten_arena",
                    cards_n = db.cards.len(),
                    version = db.version
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
    pub(super) fn refresh_owned(&mut self) {
        self.owned = match (&self.collection, &self.db) {
            (Some(c), Some(db)) => c.owned_by_name(db),
            _ => HashMap::new(),
        };
        self.subs.clear();
        self.built.decks = None;
        self.evals.clear();
        if let Some(db) = &self.db {
            for (id, list) in &self.lists {
                self.evals
                    .insert(*id, deck::evaluate(list, db, &self.owned));
            }
        }
    }

    pub(super) fn load_format(&mut self, force: bool) {
        let format = self.settings.format;
        let _ = self.tx_job.send(Job::Clear);
        let _ = self.tx_job.send(Job::Archetypes { format, force });
        self.tour.archetypes.clear();
        self.tour.decks.clear();
        self.tour.requested.clear();
        self.tour.sel_arch = None;
        self.tour.sel_deck = None;
        self.subs.clear();
        self.built.decks = None;
        // magic.gg-Events beim nächsten Öffnen für das neue Format laden.
        self.gg.events.clear();
        self.gg.format = None;
        self.gg.sel_event = None;
        self.gg.sel_deck = None;
        self.status = tr!("state.lade_metagame_mtgtop8", v = FORMATS[format].1);
        if self.tour.legal_requested.insert(format) {
            let (tx, cache) = (self.tx_msg.clone(), self.data_dir.join("cache"));
            std::thread::spawn(move || {
                let r = Fetcher::new(cache)
                    .legal_names(FORMATS[format].2)
                    .map_err(|e| format!("{e:#}"));
                let _ = tx.send(Msg::Legal(format, r));
            });
        }
    }

    pub(super) fn sync_collection(&mut self, ctx: &egui::Context) {
        self.syncing = true;
        self.status = tr!("state.lese_sammlung_aus_mtg");
        let (tx, ctx, path) = (
            self.tx_msg.clone(),
            ctx.clone(),
            self.data_dir.join("collection.json"),
        );
        std::thread::spawn(move || {
            let result = collection::sync(&path).map_err(|e| format!("{e:#}"));
            let _ = tx.send(Msg::Collection(result));
            ctx.request_repaint();
        });
    }
    /// Reiht alle noch nicht geladenen Listen eines Archetyps ein, damit die
    /// günstigste Variante gefunden wird (eine Anfrage alle 5 s, dauerhaft gecacht).
    pub(super) fn queue_variants(&mut self, arch: u32) {
        let ids: Vec<u32> = self
            .tour
            .decks
            .get(&arch)
            .into_iter()
            .flatten()
            .map(|d| d.id)
            .collect();
        for deck_id in ids {
            if !self.lists.contains_key(&deck_id) && self.tour.requested.insert(deck_id) {
                let _ = self.tx_job.send(Job::List {
                    deck_id,
                    urgent: false,
                });
            }
        }
    }

    pub(super) fn set_list(&mut self, deck_id: u32, list: Deck) {
        if let Some(db) = &self.db {
            self.evals
                .insert(deck_id, deck::evaluate(&list, db, &self.owned));
        }
        self.lists.insert(deck_id, list);
    }

    pub(super) fn replaced_qty(&self, deck_id: u32, line: &LineEval) -> u32 {
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
    pub(super) fn apply_budget(&mut self, deck_id: u32, eval: &DeckEval) {
        let Some(db) = &self.db else { return };
        let legal = self.tour.legal.get(&self.settings.format);
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
            if !listed && let Some(s) = Suggestion::for_card(db, &self.owned, rep) {
                self.subs.entry(slot).or_default().push(s);
            }
        }
        self.repl.retain(|k, _| k.0 != deck_id);
        for (key, rep) in &plan.replacements {
            self.repl.insert((deck_id, key.clone()), rep.clone());
        }
        let text = budget::plan_text(&plan);
        self.status = text;
    }

    pub(super) fn summary(&self, deck_id: u32) -> Option<Summary> {
        let eval = self.evals.get(&deck_id)?;
        Some(eval.summary(self.settings.include_side, |l| {
            self.replaced_qty(deck_id, l)
        }))
    }

    pub(super) fn wildcards(&self) -> [u32; 4] {
        self.collection
            .as_ref()
            .map(|c| c.wildcards.as_array())
            .unwrap_or_default()
    }

    /// Farbe + Kurztext: was das Deck an Wildcards kostet.
    pub(super) fn fit_text(&self, s: &Summary) -> (Color32, String) {
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
    pub(super) fn best_of_arch(&self, arch: u32) -> Option<Summary> {
        self.tour
            .decks
            .get(&arch)?
            .iter()
            .filter_map(|d| self.summary(d.id))
            .min_by_key(|s| cost_key(s, self.wildcards()))
    }

    /// Alle geladenen Turnierlisten des aktuellen Formats, nach Archetyp.
    pub(super) fn meta_archetypes(&self) -> Vec<deckcheck::MetaArchetype<'_>> {
        self.tour
            .archetypes
            .iter()
            .map(|a| deckcheck::MetaArchetype {
                id: a.id,
                name: a.name.clone(),
                lists: self
                    .tour
                    .decks
                    .get(&a.id)
                    .into_iter()
                    .flatten()
                    .filter_map(|d| self.lists.get(&d.id))
                    .collect(),
            })
            .collect()
    }

    /// Alle geladenen Turnierlisten (MTGTop8 und magic.gg) als Kandidaten für
    /// die Empfehlung nach Spielstil.
    pub(super) fn style_candidates(&self, db: &CardDb) -> Vec<style::Candidate> {
        let mut out = Vec::new();
        for a in &self.tour.archetypes {
            for d in self.tour.decks.get(&a.id).into_iter().flatten() {
                if let (Some(list), Some(summary)) = (self.lists.get(&d.id), self.summary(d.id)) {
                    out.push(style::Candidate {
                        deck_id: d.id,
                        label: a.name.clone(),
                        source: tr!("state.turnierdecks"),
                        style: style::DeckProfile::of_deck(list, db).style(),
                        summary,
                    });
                }
            }
        }
        let shares = opponents::arch_shares(&self.meta_archetypes(), db);
        for decks in self.gg.decks.values() {
            for (id, d) in decks {
                let Some(summary) = self.summary(*id) else {
                    continue;
                };
                let label = if d.subtitle.is_empty() {
                    opponents::deck_label(&d.deck, db, &shares)
                } else {
                    d.subtitle.clone()
                };
                out.push(style::Candidate {
                    deck_id: *id,
                    label,
                    source: tr!("state.offizielle_events"),
                    style: style::DeckProfile::of_deck(&d.deck, db).style(),
                    summary,
                });
            }
        }
        out
    }

    /// Öffnet ein Turnierdeck in seinem Reiter (MTGTop8 oder magic.gg).
    pub(super) fn open_deck(&mut self, id: u32) {
        if let Some(arch) = self
            .tour
            .decks
            .iter()
            .find(|(_, ds)| ds.iter().any(|d| d.id == id))
            .map(|(a, _)| *a)
        {
            self.tour.sel_arch = Some(arch);
            self.tour.sel_deck = Some(id);
            self.tab = Tab::Meta;
        } else if let Some(slug) = self
            .gg
            .decks
            .iter()
            .find(|(_, ds)| ds.iter().any(|(i, _)| *i == id))
            .map(|(s, _)| s.clone())
        {
            self.gg.sel_event = Some(slug);
            self.gg.sel_deck = Some(id);
            self.tab = Tab::Events;
        }
    }
}
