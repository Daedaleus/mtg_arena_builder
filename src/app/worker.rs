//! Hintergrund-Abrufe (MTGTop8, Scryfall) und Verarbeitung ihrer Ergebnisse.

use super::*;

pub(super) enum Job {
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
    /// magic.gg: Events eines Formats.
    GgEvents {
        format: usize,
        force: bool,
    },
    /// magic.gg: Decks eines Events.
    GgDecks {
        slug: String,
    },
    /// Bereits heruntergeladene Decks aus dem Cache wiederherstellen (ohne Netz).
    Restore {
        format: usize,
        archetypes: Vec<Archetype>,
    },
    Clear,
}

pub(super) enum Msg {
    Archetypes(usize, Result<Vec<Archetype>, String>),
    Decks(usize, u32, Result<Vec<DeckRef>, String>),
    List(u32, Result<String, String>),
    Legal(usize, Result<HashSet<String>, String>),
    GgEvents(usize, Result<Vec<GgEvent>, String>),
    GgDecks(String, Result<Vec<GgDeck>, String>),
    Collection(Result<Collection, String>),
    /// Wiederherstellung aus dem Cache ist abgeschlossen.
    Restored(usize),
    /// Partien aus den Arena-Logs; `bool` = „Detaillierte Logs“ ist aus.
    Games(games::Synced, bool),
    /// Kartenbild von Scryfall (None = keins gefunden).
    Image(String, images::Fetched),
    Pending(usize),
    /// Probehände fertig.
    Trial(SimTarget, Box<sim::Trial>),
    /// Deckvorschläge aus der Sammlung fertig.
    Built(Vec<Built>),
}

/// Arbeitet Web-Abrufe nacheinander ab (MTGTop8 erlaubt nur eine Anfrage alle 5 s).
pub(super) fn worker(rx: Receiver<Job>, tx: Sender<Msg>, ctx: egui::Context, cache: PathBuf) {
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
            Job::GgEvents { format, force } => {
                send(Msg::GgEvents(
                    format,
                    fetcher.gg_events(FORMATS[format].2, force).map_err(err),
                ));
            }
            Job::GgDecks { slug } => {
                let decks = fetcher.gg_decks(&slug).map_err(err);
                send(Msg::GgDecks(slug, decks));
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
                // Schon geöffnete magic.gg-Events ebenfalls wiederherstellen.
                for (event, decks) in fetcher.cached_gg(FORMATS[format].2) {
                    send(Msg::GgDecks(event.slug, Ok(decks)));
                }
                send(Msg::Restored(format));
            }
        }
        send(Msg::Pending(queue.len()));
    }
}

impl App {
    pub(super) fn handle_messages(&mut self) {
        while let Ok(msg) = self.rx_msg.try_recv() {
            // Neue Daten: zwischengespeicherte Auswertungen neu berechnen
            // (Kartenbilder, Probehände und Fortschrittsmeldungen ändern daran nichts).
            if !matches!(msg, Msg::Image(..) | Msg::Trial(..) | Msg::Pending(..)) {
                self.data_gen = self.data_gen.wrapping_add(1);
            }
            match msg {
                Msg::Archetypes(format, result) if format == self.settings.format => match result {
                    Ok(list) => {
                        self.status = tr!("worker.archetypen_geladen", list_n = list.len());
                        let _ = self.tx_job.send(Job::Restore {
                            format,
                            archetypes: list.clone(),
                        });
                        self.tour.archetypes = list;
                    }
                    Err(e) => self.status = e,
                },
                Msg::Decks(format, arch, result) if format == self.settings.format => {
                    match result {
                        Ok(list) => {
                            if self.tour.sel_arch == Some(arch) && self.tour.sel_deck.is_none() {
                                self.tour.sel_deck = list.first().map(|d| d.id);
                            }
                            self.tour.decks.insert(arch, list);
                            if self.tour.sel_arch == Some(arch) {
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
                        self.tour.legal.insert(format, names);
                        self.subs.clear();
                        self.built.decks = None;
                    }
                    Err(e) => {
                        // Beim nächsten Laden des Formats erneut versuchen.
                        self.tour.legal_requested.remove(&format);
                        self.status = tr!("worker.format_legalitaet_nicht_verfuegbar", e = e);
                    }
                },
                Msg::Collection(result) => {
                    self.syncing = false;
                    match result {
                        Ok(c) => {
                            self.status =
                                tr!("worker.sammlung_gelesen_karten", v = c.total_cards());
                            self.collection = Some(c);
                            self.refresh_owned();
                        }
                        Err(e) => self.status = tr!("worker.sammlung_konnte_nicht_gelesen", e = e),
                    }
                }
                Msg::Restored(format) if format == self.settings.format => {
                    // Gemerkte Auswahl, deren Decks nicht (mehr) im Cache liegen, nachladen.
                    if let Some(id) = self
                        .tour
                        .sel_arch
                        .filter(|id| !self.tour.decks.contains_key(id))
                    {
                        match self.tour.archetypes.iter().find(|a| a.id == id).cloned() {
                            Some(arch) => {
                                let _ = self.tx_job.send(Job::Decks {
                                    format,
                                    arch,
                                    urgent: true,
                                });
                            }
                            None => {
                                self.tour.sel_arch = None;
                                self.tour.sel_deck = None;
                            }
                        }
                    }
                }
                Msg::Image(name, img) => match img {
                    images::Fetched::Found(img) => {
                        let img = egui::ColorImage::from_rgba_unmultiplied(img.size, &img.rgba);
                        let tex =
                            self.ctx
                                .load_texture(format!("card:{name}"), img, Default::default());
                        self.images.textures.insert(name, tex);
                    }
                    images::Fetched::NotFound => {
                        self.images.missing.insert(name);
                    }
                    // Beim nächsten Überfahren erneut anfragen.
                    images::Fetched::Failed => {
                        self.images.requested.borrow_mut().remove(&name);
                    }
                },
                Msg::Games(synced, disabled) => {
                    self.games.loading = false;
                    self.games.logs_disabled = disabled;
                    if let Some(e) = synced.problem {
                        self.status = e;
                    }
                    let games = synced.games;
                    if games.len() != self.games.list.len() {
                        self.games.sel = None;
                    }
                    self.games.list = games;
                }
                Msg::Trial(target, trial) => self.finish_trial(target, *trial),
                Msg::Built(built) => self.apply_built(built),
                Msg::Pending(n) => {
                    self.tour.pending = n;
                    if n == 0 && self.tour.rating_all {
                        self.tour.rating_all = false;
                        self.status = tr!("worker.alle_archetypen_bewertet");
                    }
                }
                Msg::GgEvents(format, result) if format == self.settings.format => match result {
                    Ok(list) => {
                        self.status = tr!("worker.events_magic_gg_geladen", list_n = list.len());
                        self.gg.events = list;
                    }
                    Err(e) => self.status = e,
                },
                Msg::GgDecks(slug, result) => match result {
                    Ok(decks) => {
                        self.status = tr!("worker.decks_magic_gg_geladen", decks_n = decks.len());
                        let mut list = Vec::new();
                        for (i, d) in decks.into_iter().enumerate() {
                            let id = magicgg::deck_id(&slug, i);
                            self.set_list(id, d.deck.clone());
                            list.push((id, d));
                        }
                        self.gg.decks.insert(slug, list);
                    }
                    Err(e) => {
                        self.gg.requested.remove(&slug);
                        self.status = e;
                    }
                },
                _ => {}
            }
        }
    }
}
