//! Reiter „Turnierdecks“, „Aus meiner Sammlung“ und „Eigene Liste“.

use super::*;

impl App {
    pub(super) fn meta_tab(&mut self, ui: &mut egui::Ui) {
        let Some(arch_id) = self.sel_arch else {
            ui.add_space(20.0);
            ui.label(tr!("lists.links_einen_archetyp_auswaehlen"));
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
                ui.label(tr!("lists.lade_turnierdecks"));
            });
            return;
        };
        if decks.is_empty() {
            ui.label(tr!("lists.diesen_archetyp_wurden_keine"));
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
                RichText::new(tr!(
                    "lists.listen_bewertet_uebrigen_werden",
                    summaries_n = summaries.len(),
                    decks_n = decks.len()
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
                    for title in [
                        tr!("lists.spieler"),
                        tr!("lists.turnier"),
                        tr!("lists.platz"),
                        tr!("lists.datum"),
                        tr!("lists.wildcards_noetig"),
                    ] {
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
                                        text.push_str(&tr!("lists.guenstigsten"));
                                    }
                                    ui.label(RichText::new(text).color(color))
                                        .on_hover_text(cost_tooltip(s));
                                }
                                None => {
                                    ui.label(RichText::new(tr!("lists.nicht_geladen")).weak());
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
                    ui.label(tr!("lists.lade_deckliste"));
                });
            }
            None => {}
        }
    }

    /// Baut Deckvorschläge aus der Sammlung und legt sie unter eigenen Schlüsseln ab.
    pub(super) fn compute_built(&mut self) {
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
        self.built_lists = self.lists.len();
        self.built_at = Some(std::time::Instant::now());
    }

    pub(super) fn built_tab(&mut self, ui: &mut egui::Ui) {
        if self.collection.is_none() || self.db.is_none() {
            ui.label(tr!("lists.dafuer_muss_zuerst_sammlung"));
            return;
        }
        // Neu rechnen, wenn inzwischen weitere Turnierlisten geladen wurden
        // (beim Start kommen sie erst nach und nach aus dem Zwischenspeicher).
        let stale = self.lists.len() != self.built_lists
            && self.built_at.is_none_or(|t| t.elapsed().as_secs() >= 2);
        if self.built.is_none() || stale {
            self.compute_built();
        }
        ui.label(tr!(
            "lists.decks_sofort_ohne_wildcards",
            v = FORMATS[self.settings.format].1
        ));
        ui.label(RichText::new(tr!("lists.kartenstaerke_geschaetzt_seltenheit_vorkommen")).weak());
        let built = self.built.clone().unwrap_or_default();
        if built.is_empty() {
            ui.label(RichText::new(tr!("lists.aus_deiner_sammlung_laesst")).color(YELLOW));
            return;
        }
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            for (i, b) in built.iter().enumerate() {
                let text = format!("{}  {:.1}", b.label, b.score);
                if ui
                    .selectable_label(self.sel_built == i, text)
                    .on_hover_text(tr!("lists.geschaetzte_staerke_hoeher_besser"))
                    .clicked()
                {
                    self.sel_built = i;
                }
            }
            if ui.button(tr!("lists.neu_berechnen")).clicked() {
                self.built = None;
            }
        });
        let Some(b) = built.get(self.sel_built) else {
            return;
        };
        ui.label(tr!("lists.kreaturen_removal_mana_rares", label = b.label, creatures = b.creatures, removal = b.removal, avg_cmc = b.avg_cmc : {:.1}, rares = b.rares, meta_cards = b.meta_cards));
        ui.separator();
        self.deck_view(ui, BUILT_BASE + self.sel_built as u32);
    }

    pub(super) fn paste_tab(&mut self, ui: &mut egui::Ui) {
        ui.label(tr!("lists.deckliste_arena_oder_mtgo"));
        egui::ScrollArea::vertical()
            .id_salt("paste")
            .max_height(140.0)
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut self.paste_text)
                        .desired_rows(6)
                        .desired_width(f32::INFINITY)
                        .hint_text(tr!("lists.4_watery_grave_4")),
                );
            });
        if ui.button(tr!("lists.pruefen")).clicked() {
            self.repl.retain(|k, _| k.0 != PASTED);
            self.subs.retain(|k, _| k.0 != PASTED);
            self.set_list(PASTED, deck::parse(&self.paste_text));
        }
        ui.separator();
        if self.evals.get(&PASTED).is_some_and(|e| !e.lines.is_empty()) {
            self.deck_view(ui, PASTED);
        }
    }
}
