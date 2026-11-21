//! Data of the interface: loading, lists, ratings and budget version.

use super::*;

impl App {
    /// Reads the Arena logs in the background and extends `games.json`.
    /// New language: have cached texts (sample hands, own decks) computed
    /// again.
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
        let store = self.data_dir.join("games.json");
        self.tasks.run(move || {
            let logs = paths::player_logs();
            let games = games::sync(&store, &logs);
            let disabled = logs
                .iter()
                .find(|p| p.file_name().is_some_and(|n| n == "Player.log"))
                .is_some_and(|p| games::detailed_logs_disabled(p));
            Msg::Games(games, disabled)
        });
    }

    /// Writes settings and session state to `settings.json` (only if something
    /// changed).
    pub(super) fn save_settings(&mut self) {
        // The screenshot mode changes tab and language only for itself.
        if self.shooter.is_some() {
            return;
        }
        self.settings.sort_by_fit = self.sort_by_fit;
        self.settings.tab = self.tab.saved();
        self.settings.paste_text = self.paste_text.clone();
        self.settings.sel_arch = self.tour.sel_arch;
        self.settings.sel_deck = self.tour.sel_deck;
        let raw: HashMap<(u32, String), String> = self
            .repl
            .iter()
            .map(|((id, key), rep)| ((id.to_raw(), key.clone()), rep.clone()))
            .collect();
        self.settings.set_replacements(&raw);
        if let Err(e) = self.settings_file.save(&self.settings) {
            self.status = tr!("state.could_save_settings", e = e);
        }
    }

    /// State of the card database – only phrased when shown, so a language
    /// switch covers it.
    pub(super) fn db_status(&self) -> String {
        match &self.db {
            Some(db) => tr!(
                "state.card_data_cards_arena",
                cards_n = db.card_count(),
                version = db.version
            ),
            None => self.db_error.clone(),
        }
    }

    pub(super) fn load_db(&mut self) {
        let found =
            paths::find_card_db(Some(&self.settings.mtga_dir)).and_then(|p| CardDb::load(&p));
        match found {
            Ok(db) => {
                self.db_error.clear();
                self.db = Some(std::sync::Arc::new(db));
                // The companion keeps reading with the new database.
                if let Some(live) = self.live.take() {
                    live.stop.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            }
            Err(e) => {
                self.db_error = format!("{e:#}");
                self.db = None;
            }
        }
        self.refresh_owned();
    }

    /// After changes to collection/card data, rate everything again.
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

    /// Switches the game mode of the app: tournament format, visible tabs.
    pub(super) fn set_mode(&mut self, mode: GameFormat) {
        self.settings.mode = mode;
        // "My games" follows, unless it shows all games.
        if self.games.filter.format.is_some() {
            self.games.filter = games::GamesFilter::of_format(mode);
            self.games.sel = None;
            self.games.page.set(0);
        }
        if !self.tab.fits(mode) {
            // Stay in the area if it has a fitting page, otherwise My games.
            self.tab = self.tab.area().first_tab(mode).unwrap_or(Tab::Games);
        }
        let format_changed = mode.meta().is_some_and(|f| f != self.settings.format);
        if let Some(f) = mode.meta() {
            self.settings.format = f;
        }
        self.save_settings();
        if format_changed {
            self.load_format(false);
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
        // Load magic.gg events for the new format at the next opening.
        self.gg.events.clear();
        self.gg.format = None;
        self.gg.sel_event = None;
        self.gg.sel_deck = None;
        self.status = tr!("state.loading_metagame_mtgtop8", v = format.name());
        if self.tour.legal_requested.insert(format) {
            let (cache, net) = (self.data_dir.join("cache"), self.net.clone());
            self.tasks.run(move || {
                let r = Fetcher::with_http(cache, net)
                    .legal_names(format.scryfall())
                    .map_err(|e| format!("{e:#}"));
                Msg::Legal(format, r)
            });
        }
    }

    pub(super) fn sync_collection(&mut self) {
        self.syncing = true;
        self.status = tr!("state.reading_collection_mtg_arena");
        let path = self.data_dir.join("collection.json");
        self.tasks
            .run(move || Msg::Collection(collection::sync(&path)));
    }
    /// Queues all not yet loaded lists of an archetype, so the cheapest
    /// variant is found (one request every 5 s, cached permanently).
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
            if !self.lists.contains_key(&DeckId::Top8(deck_id))
                && self.tour.requested.insert(deck_id)
            {
                let _ = self.tx_job.send(Job::List {
                    deck_id,
                    urgent: false,
                });
            }
        }
    }

    pub(super) fn set_list(&mut self, deck_id: DeckId, list: Deck) {
        if self.lists.get(&deck_id) != Some(&list) {
            self.sims.list_replaced(deck_id);
        }
        if let Some(db) = &self.db {
            self.evals
                .insert(deck_id, deck::evaluate(&list, db, &self.owned));
        }
        self.lists.insert(deck_id, list);
    }

    pub(super) fn replaced_qty(&self, deck_id: DeckId, line: &LineEval) -> u32 {
        let Some(key) = &line.key else { return 0 };
        let Some(rep) = self.repl.get(&(deck_id, key.clone())) else {
            return 0;
        };
        match &self.db {
            Some(db) => budget::replaced_qty(db, &self.owned, rep, line.missing),
            None => 0,
        }
    }

    /// Replaces missing cards automatically so the wildcards suffice.
    pub(super) fn apply_budget(&mut self, deck_id: DeckId, eval: &DeckEval) {
        let Some(db) = &self.db else { return };
        let legal = self.legal();
        let plan = budget::plan(
            eval,
            db,
            &self.owned,
            legal,
            self.wildcards(),
            self.settings.include_side,
        );
        // The chosen replacement must be in the selection list – otherwise add it.
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

    pub(super) fn summary(&self, deck_id: DeckId) -> Option<Summary> {
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

    /// Color + short text: what the deck costs in wildcards.
    pub(super) fn fit_text(&self, s: &Summary) -> (Color32, String) {
        let (fit, text) = present::fit(s, self.collection.is_some(), self.wildcards());
        let color = match fit {
            Fit::Unknown => theme::tone(&self.ctx, Tone::Muted),
            Fit::Complete => theme::tone(&self.ctx, Tone::Good),
            Fit::Craftable => theme::tone(&self.ctx, Tone::Warn),
            Fit::TooExpensive => theme::tone(&self.ctx, Tone::Bad),
        };
        (color, text)
    }

    /// Cheapest already rated deck of an archetype.
    pub(super) fn best_of_arch(&self, arch: u32) -> Option<Summary> {
        self.tour
            .decks
            .get(&arch)?
            .iter()
            .filter_map(|d| self.summary(DeckId::Top8(d.id)))
            .min_by_key(|s| cost_key(s, self.wildcards()))
    }

    /// All loaded tournament lists of the current format, by archetype.
    /// Cards legal in the game mode (`None` = unknown or no tournament format,
    /// e.g. Brawl: then everything counts as allowed).
    pub(super) fn legal(&self) -> Option<&HashSet<String>> {
        let format = self.settings.mode.meta()?;
        self.tour.legal.get(&format)
    }

    /// Archetypes of the tournament format; none in modes without one.
    pub(super) fn meta_archetypes(&self) -> Vec<deckcheck::MetaArchetype<'_>> {
        if self.settings.mode.meta().is_none() {
            return Vec::new();
        }
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
                    .filter_map(|d| self.lists.get(&DeckId::Top8(d.id)))
                    .collect(),
            })
            .collect()
    }

    /// All loaded tournament lists (MTGTop8 and magic.gg) as candidates for the
    /// recommendation by play style.
    pub(super) fn style_candidates(&self, db: &CardDb) -> Vec<style::Candidate> {
        let mut out = Vec::new();
        for a in &self.tour.archetypes {
            for d in self.tour.decks.get(&a.id).into_iter().flatten() {
                if let (Some(list), Some(summary)) = (
                    self.lists.get(&DeckId::Top8(d.id)),
                    self.summary(DeckId::Top8(d.id)),
                ) {
                    out.push(style::Candidate {
                        deck_id: d.id,
                        label: a.name.clone(),
                        source: tr!("state.tournament_decks"),
                        style: style::DeckProfile::of_deck(list, db).style(),
                        summary,
                    });
                }
            }
        }
        let shares = opponents::arch_shares(&self.meta_archetypes(), db);
        for decks in self.gg.decks.values() {
            for (id, d) in decks {
                let Some(summary) = self.summary(DeckId::Gg(*id)) else {
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
                    source: tr!("state.official_events"),
                    style: style::DeckProfile::of_deck(&d.deck, db).style(),
                    summary,
                });
            }
        }
        out
    }

    /// Opens a tournament deck in its tab (MTGTop8 or magic.gg).
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
