//! Deckansicht mit fehlenden Karten, Ersatzvorschlägen und Kartenbildern.

use super::*;

/// Angeklickte Knöpfe der Deckansicht.
#[derive(Default)]
struct DeckActions {
    budget: bool,
    reset: bool,
    simulate: bool,
}

impl App {
    /// Berechnet (einmalig) Ersatzvorschläge für alle fehlenden Karten eines Decks.
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
                ui.label(tr!("common.rechne"));
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
            self.status = tr!("deck_view.ersetzungen_zurueckgesetzt");
        }
        if clicked.budget {
            // Neu planen: bisherige Ersetzungen zählen dabei nicht.
            self.repl.retain(|k, _| k.0 != deck_id);
            self.apply_budget(deck_id, &eval);
        }
        ui.add_space(4.0);

        self.card_table(ui, deck_id, &eval);
    }

    /// Baubar? Vorhandene Karten und benötigte Wildcards.
    fn summary_section(&self, ui: &mut egui::Ui, summary: &Summary, wc: [u32; 4]) {
        ui.horizontal_wrapped(|ui| {
            if self.collection.is_none() {
                ui.label(
                    RichText::new(tr!("deck_view.ohne_gelesene_sammlung_gilt")).color(YELLOW),
                );
            } else if summary.complete() {
                ui.label(
                    RichText::new(tr!("deck_view.kannst_dieses_deck_komplett"))
                        .color(GREEN)
                        .strong(),
                );
            } else if summary.craftable(wc) {
                ui.label(
                    RichText::new(tr!("deck_view.deinen_wildcards_baubar"))
                        .color(YELLOW)
                        .strong(),
                );
            } else {
                ui.label(RichText::new(tr!("deck_view.noch_nicht_baubar")).color(RED).strong());
            }
            ui.label(tr!("deck_view.karten_vorhanden", missing = summary.total - summary.missing, total = summary.total, v = summary.percent().floor() : {:.0}));
        });
        if summary.missing > 0 {
            ui.horizontal_wrapped(|ui| {
                ui.label(tr!("deck_view.benoetigte_wildcards"));
                for i in 0..4 {
                    if summary.need[i] > 0 {
                        let enough = summary.need[i] <= wc[i];
                        ui.label(
                            RichText::new(tr!(
                                "deck_view.hast",
                                need = summary.need[i],
                                rarity_names = Rarity::WILDCARDS[i].label(),
                                wc = wc[i]
                            ))
                            .color(if enough { GREEN } else { RED }),
                        );
                    }
                }
                if summary.unavailable > 0 {
                    ui.label(
                        RichText::new(tr!(
                            "deck_view.karten_gibt_arena_nicht",
                            unavailable = summary.unavailable
                        ))
                        .color(RED),
                    );
                }
            });
        }
    }

    /// Knöpfe unter der Zusammenfassung; Klicks werden danach angewandt.
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
                    .button(tr!("deck_view.budget_version_bauen"))
                    .on_hover_text(tr!("deck_view.stellt_wichtigsten_fehlenden_karten"))
                    .clicked();
            }
            if has_repl {
                clicked.reset = ui
                    .button(tr!("deck_view.ersetzungen_zuruecksetzen"))
                    .clicked();
            }
            clicked.simulate = ui
                .button(tr!("deck_view.probehaende_testen"))
                .on_hover_text(tr!("deck_view.spielt_deck_1000_mal"))
                .clicked();
            if ui
                .button(tr!("deck_view.arena_kopieren"))
                .on_hover_text(tr!("deck_view.arena_decks_importieren"))
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
                self.status = tr!("deck_view.deck_zwischenablage_kopiert_arena");
            }
            let mut changed = ui
                .checkbox(
                    &mut self.settings.include_side,
                    tr!("deck_view.sideboard_einbeziehen"),
                )
                .changed();
            changed |= ui
                .checkbox(
                    &mut self.settings.with_sets,
                    tr!("deck_view.set_kuerzeln_exportieren"),
                )
                .on_hover_text(tr!("deck_view.empfohlen_wenn_arena_nicht"))
                .changed();
            if changed {
                self.save_settings();
            }
        });
        clicked
    }

    /// Probehände mit dem Deck, wie es mit den gewählten Ersatzkarten aussieht.
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

    /// Kartenliste mit Besitz, Fehlendem und Ersatzauswahl.
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
                        tr!("deck_check.anz"),
                        tr!("deck_view.karte"),
                        tr!("deck_view.seltenheit"),
                        tr!("deck_view.besitz"),
                        tr!("deck_view.fehlt"),
                        tr!("deck_view.ersatz_aus_deiner_sammlung"),
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
                // Den Namen in der anderen Sprache zusätzlich zeigen.
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
                ui.label(RichText::new(line.rarity.label()).color(rarity_color(line.rarity)));
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
                ui.label(RichText::new(tr!("deck_view.nicht_arena_verfuegbar")).color(RED));
                return;
            };
            let slot = (deck_id, key.clone());
            let options = self.subs.get(&slot).map(Vec::as_slice).unwrap_or(&[]);
            if options.is_empty() {
                ui.label(RichText::new(tr!("deck_view.kein_vorschlag")).weak());
                return;
            }
            let current = self.repl.get(&slot);
            let shown = current
                .and_then(|k| options.iter().find(|o| &o.key == k))
                .map(|o| o.name.clone())
                .unwrap_or_else(|| tr!("deck_view.vorschlaege", options_n = options.len()));
            egui::ComboBox::from_id_salt(("rep", deck_id, key, line.side))
                .selected_text(shown)
                .width(210.0)
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(current.is_none(), tr!("deck_view.wildcard_nutzen"))
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
