//! Deck view with missing cards, replacement suggestions and card images.

use super::*;

/// Clicked buttons of the deck view.
#[derive(Default)]
struct DeckActions {
    budget: bool,
    reset: bool,
    simulate: bool,
}

impl App {
    /// Computes (once) replacement suggestions for all missing cards of a deck.
    pub(super) fn ensure_suggestions(&mut self, deck_id: DeckId, eval: &DeckEval) {
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
        let legal = self.tour.legal.get(&self.settings.format);
        for line in eval.lines.iter().filter(|l| l.missing > 0) {
            let Some(key) = &line.key else { continue };
            let slot = (deck_id, key.clone());
            if !self.subs.contains_key(&slot) {
                let found = similar::suggest(db, &self.owned, legal, colors, &in_deck, key, 6);
                self.subs.insert(slot, found);
            }
        }
    }

    pub(super) fn deck_view(&mut self, ui: &mut egui::Ui, deck_id: DeckId) {
        let Some(eval) = self.evals.get(&deck_id).cloned() else {
            return;
        };
        self.ensure_suggestions(deck_id, &eval);
        let summary = eval.summary(self.settings.include_side, |l| {
            self.replaced_qty(deck_id, l)
        });
        let wc = self.wildcards();
        self.summary_section(ui, &summary, wc);
        let clicked = self.deck_actions(ui, deck_id, &eval, &summary);
        if clicked.simulate {
            self.simulate_deck(deck_id, &eval);
        }
        if self.trial_running(&SimTarget::Deck(deck_id, self.sims.list_gen(deck_id))) {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(tr!("common.calculating"));
            });
        }
        if let Some(report) = self.sims.decks.get(&deck_id) {
            sim_view(ui, report);
        }
        if self.collection.is_some()
            && summary.missing > 0
            && let Some(db) = &self.db
        {
            let plan =
                budget::wildcard_plan(&eval, db, self.wildcards(), self.settings.include_side);
            wildcard_plan_view(ui, &plan, db);
        }
        if clicked.reset || clicked.budget {
            self.sims.decks.remove(&deck_id);
        }
        if clicked.reset {
            self.repl.retain(|k, _| k.0 != deck_id);
            self.status = tr!("deck_view.replacements_reset");
        }
        if clicked.budget {
            // Plan anew: existing replacements don't count.
            self.repl.retain(|k, _| k.0 != deck_id);
            self.apply_budget(deck_id, &eval);
        }
        ui.add_space(4.0);

        self.card_table(ui, deck_id, &eval);
    }

    /// Buildable? Owned cards and required wildcards.
    fn summary_section(&self, ui: &mut egui::Ui, summary: &Summary, wc: [u32; 4]) {
        ui.horizontal_wrapped(|ui| {
            if self.collection.is_none() {
                ui.label(
                    RichText::new(tr!("deck_view.without_read_collection_everything")).color(theme::tone(ui.ctx(), Tone::Warn)),
                );
            } else if summary.complete() {
                ui.label(
                    RichText::new(tr!("deck_view.build_deck_completely"))
                        .color(theme::tone(ui.ctx(), Tone::Good))
                        .strong(),
                );
            } else if summary.craftable(wc) {
                ui.label(
                    RichText::new(tr!("deck_view.buildable_wildcards"))
                        .color(theme::tone(ui.ctx(), Tone::Warn))
                        .strong(),
                );
            } else {
                ui.label(RichText::new(tr!("deck_view.buildable_yet")).color(theme::tone(ui.ctx(), Tone::Bad)).strong());
            }
            ui.label(tr!("deck_view.cards_owned", missing = summary.total - summary.missing, total = summary.total, v = summary.percent().floor() : {:.0}));
        });
        if summary.missing > 0 {
            ui.horizontal_wrapped(|ui| {
                ui.label(tr!("deck_view.required_wildcards"));
                for i in 0..4 {
                    if summary.need[i] > 0 {
                        let enough = summary.need[i] <= wc[i];
                        ui.label(
                            RichText::new(tr!(
                                "deck_view.you_have",
                                need = summary.need[i],
                                rarity_names = Rarity::WILDCARDS[i].label(),
                                wc = wc[i]
                            ))
                            .color(if enough {
                                theme::tone(ui.ctx(), Tone::Good)
                            } else {
                                theme::tone(ui.ctx(), Tone::Bad)
                            }),
                        );
                    }
                }
                if summary.unavailable > 0 {
                    ui.label(
                        RichText::new(tr!(
                            "deck_view.cards_available_arena",
                            unavailable = summary.unavailable
                        ))
                        .color(theme::tone(ui.ctx(), Tone::Bad)),
                    );
                }
            });
        }
    }

    /// Buttons below the summary; clicks are applied afterwards.
    fn deck_actions(
        &mut self,
        ui: &mut egui::Ui,
        deck_id: DeckId,
        eval: &DeckEval,
        summary: &Summary,
    ) -> DeckActions {
        let has_repl = self.repl.keys().any(|k| k.0 == deck_id);
        let mut clicked = DeckActions::default();
        ui.horizontal(|ui| {
            if summary.missing > 0 || has_repl {
                clicked.budget = ui
                    .button(tr!("deck_view.build_budget_version"))
                    .on_hover_text(tr!("deck_view.crafts_most_important_missing"))
                    .clicked();
            }
            if has_repl {
                clicked.reset = ui.button(tr!("deck_view.reset_replacements")).clicked();
            }
            clicked.simulate = ui
                .button(tr!("deck_view.test_sample_hands"))
                .on_hover_text(tr!("deck_view.plays_deck_1000_times"))
                .clicked();
            if ui
                .button(tr!("deck_view.copy_arena"))
                .on_hover_text(tr!("deck_view.arena_decks_import"))
                .clicked()
                && let Some(db) = &self.db
            {
                let text = deck::export_arena(
                    eval,
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
                self.status = tr!("deck_view.deck_copied_clipboard_paste");
            }
            let mut changed = ui
                .checkbox(
                    &mut self.settings.include_side,
                    tr!("deck_view.include_sideboard"),
                )
                .changed();
            changed |= ui
                .checkbox(
                    &mut self.settings.with_sets,
                    tr!("deck_view.export_set_codes"),
                )
                .on_hover_text(tr!("deck_view.recommended_arena_running_english"))
                .changed();
            if changed {
                self.save_settings();
            }
        });
        clicked
    }

    /// Sample hands with the deck as it looks with the chosen replacement cards.
    fn simulate_deck(&mut self, deck_id: DeckId, eval: &DeckEval) {
        let Some(db) = &self.db else { return };
        let entries: Vec<(String, u32)> = eval
            .lines
            .iter()
            .filter(|l| !l.side)
            .flat_map(|l| {
                let rep = l
                    .key
                    .as_ref()
                    .and_then(|k| self.repl.get(&(deck_id, k.clone())))
                    .map(|r| (r.clone(), self.replaced_qty(deck_id, l)))
                    .filter(|(_, n)| *n > 0);
                let n = rep.as_ref().map_or(0, |(_, n)| *n);
                std::iter::once((l.name.clone(), l.qty - n)).chain(rep)
            })
            .collect();
        let deck = sim::deck_from_entries(entries.iter().map(|(n, q)| (n.as_str(), *q)), db);
        self.start_trial(SimTarget::Deck(deck_id, self.sims.list_gen(deck_id)), deck);
    }

    /// Card list with ownership, missing cards and replacement selection.
    fn card_table(&mut self, ui: &mut egui::Ui, deck_id: DeckId, eval: &DeckEval) {
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
                        tr!("deck_check.qty"),
                        tr!("deck_view.card"),
                        tr!("deck_view.rarity"),
                        tr!("deck_view.owned"),
                        tr!("deck_view.missing"),
                        tr!("deck_view.replacement_collection"),
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
                                    ui.strong(tr!("deck_view.sideboard"));
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

    pub(super) fn card_row(
        &self,
        row: &mut egui_extras::TableRow<'_, '_>,
        deck_id: DeckId,
        line: &LineEval,
        new_choice: &mut Option<(String, Option<String>)>,
    ) {
        let db = self.db.as_ref();
        let card = line.key.as_deref().and_then(|k| db?.representative(k));
        row.col(|ui| {
            ui.label(line.qty.to_string());
        });
        row.col(|ui| {
            let shown = card.map_or_else(|| line.name.clone(), |c| c.display_name().to_string());
            let label = ui.label(&shown);
            if let Some(c) = card {
                // Also show the name in the other language.
                let other = [Some(c.name.as_str()), c.name_de.as_deref()]
                    .into_iter()
                    .flatten()
                    .find(|n| *n != shown);
                let info = format!(
                    "{}{}  ·  {} {}",
                    other.map(|d| format!("{d}\n")).unwrap_or_default(),
                    c.mana_cost,
                    c.set,
                    c.collector
                );
                self.card_hover(label, &c.name, Some(info));
            }
        });
        row.col(|ui| {
            if line.key.is_some() {
                rarity_label(ui, line.rarity);
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
                ui.label(
                    RichText::new(line.missing.to_string())
                        .color(theme::tone(ui.ctx(), Tone::Bad))
                        .strong(),
                );
            }
        });
        row.col(|ui| {
            if line.missing == 0 {
                return;
            }
            let Some(key) = &line.key else {
                toned(ui, Tone::Bad, tr!("deck_view.available_arena"));
                return;
            };
            let slot = (deck_id, key.clone());
            let options = self.subs.get(&slot).map(Vec::as_slice).unwrap_or(&[]);
            if options.is_empty() {
                note(ui, tr!("deck_view.no_suggestion"));
                return;
            }
            let current = self.repl.get(&slot);
            let shown = current
                .and_then(|k| options.iter().find(|o| &o.key == k))
                .map(|o| o.name.clone())
                .unwrap_or_else(|| tr!("deck_view.suggestions", options_n = options.len()));
            egui::ComboBox::from_id_salt(("rep", deck_id, key, line.side))
                .selected_text(shown)
                .width(210.0)
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(current.is_none(), tr!("deck_view.use_wildcard"))
                        .clicked()
                    {
                        *new_choice = Some((key.clone(), None));
                    }
                    for o in options {
                        let text = format!("{}  {}  ({}×)", self.shown(&o.name), o.info, o.owned);
                        if ui.selectable_label(current == Some(&o.key), text).clicked() {
                            *new_choice = Some((key.clone(), Some(o.key.clone())));
                        }
                    }
                });
        });
    }
}
