//! Reiter „Turnierdecks“, „Aus meiner Sammlung“ und „Eigene Liste“.

use super::*;

impl App {
    pub(super) fn meta_tab(&mut self, ui: &mut egui::Ui) {
        let Some(arch_id) = self.tour.sel_arch else {
            ui.add_space(20.0);
            ui.label(tr!("lists.choose_archetype_left_rate"));
            return;
        };
        let name = self
            .tour
            .archetypes
            .iter()
            .find(|a| a.id == arch_id)
            .map(|a| a.name.clone())
            .unwrap_or_default();
        ui.heading(name);
        let Some(decks) = self.tour.decks.get(&arch_id).cloned() else {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(tr!("lists.loading_tournament_decks"));
            });
            return;
        };
        if decks.is_empty() {
            ui.label(tr!("lists.no_decks_found_archetype"));
            return;
        }

        let wc = self.wildcards();
        let summaries: HashMap<u32, Summary> = decks
            .iter()
            .filter_map(|d| Some((d.id, self.summary(DeckId::Top8(d.id))?)))
            .collect();
        let cheapest = summaries
            .iter()
            .min_by_key(|(id, s)| (cost_key(s, wc), std::cmp::Reverse(**id)))
            .map(|(id, _)| *id);
        if summaries.len() < decks.len() {
            ui.label(
                RichText::new(tr!(
                    "lists.lists_rated_rest_loading",
                    summaries_n = summaries.len(),
                    decks_n = decks.len()
                ))
                .weak(),
            );
        }
        if let Some(id) = self.deck_table(ui, &decks, &summaries, cheapest) {
            self.select_deck(id);
        }
        ui.separator();
        match self.tour.sel_deck {
            Some(id) if self.evals.contains_key(&DeckId::Top8(id)) => {
                self.deck_view(ui, DeckId::Top8(id))
            }
            Some(_) => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(tr!("lists.loading_decklist"));
                });
            }
            None => {}
        }
    }

    /// Tabelle der Turnierlisten eines Archetyps; liefert die angeklickte.
    fn deck_table(
        &self,
        ui: &mut egui::Ui,
        decks: &[DeckRef],
        summaries: &HashMap<u32, Summary>,
        cheapest: Option<u32>,
    ) -> Option<u32> {
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
                    for title in [
                        tr!("lists.player"),
                        tr!("lists.tournament"),
                        tr!("lists.place"),
                        tr!("lists.date"),
                        tr!("lists.wildcards_needed"),
                    ] {
                        h.col(|ui| {
                            ui.strong(title);
                        });
                    }
                })
                .body(|mut body| {
                    for d in decks {
                        body.row(20.0, |mut row| {
                            row.col(|ui| {
                                if ui
                                    .selectable_label(self.tour.sel_deck == Some(d.id), &d.player)
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
                                        text.push_str(&tr!("lists.cheapest"));
                                    }
                                    ui.label(RichText::new(text).color(color))
                                        .on_hover_text(cost_tooltip(s));
                                }
                                None => {
                                    note(ui, tr!("lists.loaded"));
                                }
                            });
                        });
                    }
                });
        });
        clicked
    }

    /// Turnierliste auswählen und bei Bedarf laden.
    fn select_deck(&mut self, id: u32) {
        self.tour.sel_deck = Some(id);
        if !self.lists.contains_key(&DeckId::Top8(id)) {
            self.tour.requested.insert(id);
            let _ = self.tx_job.send(Job::List {
                deck_id: id,
                urgent: true,
            });
        }
    }

    /// Übernimmt fertige Deckvorschläge und legt sie unter eigenen Schlüsseln ab.
    pub(super) fn apply_built(&mut self, built: Vec<Built>) {
        self.built.running = false;
        self.repl.retain(|k, _| !k.0.is_built());
        self.subs.retain(|k, _| !k.0.is_built());
        for (i, b) in built.iter().enumerate() {
            self.set_list(DeckId::Built(i as u8), b.deck.clone());
        }
        self.built.sel = self.built.sel.min(built.len().saturating_sub(1));
        self.built.decks = Some(built);
        self.built.at = Some(std::time::Instant::now());
    }

    pub(super) fn built_tab(&mut self, ui: &mut egui::Ui) {
        if self.collection.is_none() || self.db.is_none() {
            ui.label(tr!("lists.collection_must_read_first"));
            return;
        }
        // Neu rechnen, wenn inzwischen weitere Turnierlisten geladen wurden
        // (beim Start kommen sie erst nach und nach aus dem Zwischenspeicher).
        let stale = self.lists.len() != self.built.lists
            && self.built.at.is_none_or(|t| t.elapsed().as_secs() >= 2);
        if self.built.decks.is_none() || stale {
            self.start_build();
        }
        if self.built.running && self.built.decks.is_none() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(tr!("lists.building_decks_collection"));
            });
            return;
        }
        ui.label(tr!(
            "lists.decks_build_right_away",
            v = self.settings.format.name()
        ));
        note(ui, tr!("lists.card_strength_estimated_rarity"));
        let built = self.built.decks.clone().unwrap_or_default();
        if built.is_empty() {
            toned(ui, Tone::Warn, tr!("lists.no_complete_deck_built"));
            return;
        }
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            for (i, b) in built.iter().enumerate() {
                let text = format!("{}  {:.1}", b.label, b.score);
                if ui
                    .selectable_label(self.built.sel == i, text)
                    .on_hover_text(tr!("lists.estimated_strength_higher_better"))
                    .clicked()
                {
                    self.built.sel = i;
                }
            }
            if ui.button(tr!("lists.recalculate")).clicked() {
                self.built.decks = None;
            }
        });
        let Some(b) = built.get(self.built.sel) else {
            return;
        };
        ui.label(tr!("lists.creatures_removal_mana_rares", label = b.label, creatures = b.creatures, removal = b.removal, avg_cmc = b.avg_cmc : {:.1}, rares = b.rares, meta_cards = b.meta_cards));
        ui.separator();
        self.deck_view(ui, DeckId::Built(self.built.sel as u8));
    }

    pub(super) fn paste_tab(&mut self, ui: &mut egui::Ui) {
        ui.label(tr!("lists.paste_decklist_arena_mtgo"));
        egui::ScrollArea::vertical()
            .id_salt("paste")
            .max_height(140.0)
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut self.paste_text)
                        .desired_rows(6)
                        .desired_width(f32::INFINITY)
                        .hint_text(tr!("lists.4_watery_grave_n4")),
                );
            });
        if ui.button(tr!("lists.check")).clicked() {
            self.repl.retain(|k, _| k.0 != DeckId::Pasted);
            self.subs.retain(|k, _| k.0 != DeckId::Pasted);
            self.set_list(DeckId::Pasted, deck::parse(&self.paste_text));
        }
        ui.separator();
        if self
            .evals
            .get(&DeckId::Pasted)
            .is_some_and(|e| !e.lines.is_empty())
        {
            self.deck_view(ui, DeckId::Pasted);
        }
    }
}
