//! Deckansicht mit fehlenden Karten, Ersatzvorschlägen und Kartenbildern.

use super::*;

impl App {
    /// Berechnet (einmalig) Ersatzvorschläge für alle fehlenden Karten eines Decks.
    pub(super) fn ensure_suggestions(&mut self, deck_id: u32, eval: &DeckEval) {
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

    pub(super) fn deck_view(&mut self, ui: &mut egui::Ui, deck_id: u32) {
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
        let mut sim_clicked = false;
        ui.horizontal(|ui| {
            if summary.missing > 0 || has_repl {
                budget_clicked = ui
                    .button("Budget-Version bauen")
                    .on_hover_text(
                        "Stellt die wichtigsten fehlenden Karten mit deinen Wildcards her und ersetzt den Rest durch ähnliche Karten aus deiner Sammlung. Ergebnis siehst du in der Spalte „Ersatz“.",
                    )
                    .clicked();
            }
            if has_repl {
                reset_clicked = ui.button("Ersetzungen zurücksetzen").clicked();
            }
            sim_clicked = ui
                .button("Probehände testen")
                .on_hover_text(
                    "Spielt das Deck 1000-mal bis Zug 6 durch (mit deinen Ersatzkarten) und zeigt,                      wie oft Länder oder Farben fehlen und wie gut das Mana genutzt wird.",
                )
                .clicked();
            if ui.button("Für Arena kopieren").on_hover_text("In Arena: Decks > Importieren").clicked()
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
                    self.status = "Deck in die Zwischenablage kopiert – in Arena unter Decks > Importieren einfügen.".into();
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
        if sim_clicked && let Some(db) = &self.db {
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
            self.sims
                .insert(deck_id, sim::simulate(&deck, db, sim::DEFAULT_GAMES, 1));
        }
        if let Some(report) = self.sims.get(&deck_id) {
            sim_view(ui, report);
        }
        if self.collection.is_some()
            && summary.missing > 0
            && let Some(db) = &self.db
        {
            let plan =
                budget::wildcard_plan(&eval, db, self.wildcards(), self.settings.include_side);
            wildcard_plan_view(ui, &plan);
        }
        if reset_clicked || budget_clicked {
            self.sims.remove(&deck_id);
        }
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

    pub(super) fn card_row(
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

    /// Zeigt beim Überfahren eines Kartennamens das Kartenbild (lädt es bei Bedarf).
    pub(super) fn card_hover(
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
}

/// Ergebnis der Probehände: Empfehlungen und Kennzahlen.
pub(super) fn sim_view(ui: &mut egui::Ui, r: &sim::SimReport) {
    egui::CollapsingHeader::new(RichText::new("Probehände").strong())
        .id_salt("sim_view")
        .default_open(true)
        .show(ui, |ui| {
            ui.label(
                RichText::new(format!(
                    "{} simulierte Partien bis Zug {} · {} Karten, davon {} Länder",
                    r.games,
                    sim::TURNS,
                    r.size,
                    r.lands
                ))
                .weak(),
            );
            for (i, a) in r.advice.iter().enumerate() {
                let color = if a.contains("rund") { GREEN } else { YELLOW };
                let text = RichText::new(a).color(color);
                ui.label(if i == 0 { text.strong() } else { text });
            }
            let pct = |x: f32| format!("{:.0} %", x * 100.0);
            ui.label(format!(
                "Land rechtzeitig (mind. so viele Länder wie der Zug): Zug 1 {} · Zug 2 {} · Zug 3 {} · Zug 4 {}",
                pct(r.on_curve[0]),
                pct(r.on_curve[1]),
                pct(r.on_curve[2]),
                pct(r.on_curve[3])
            ));
            ui.label(format!(
                "Mulligan {} · Farbe fehlte {} · Länderflut {}",
                pct(r.mulligan),
                pct(r.color_screw),
                pct(r.flood)
            ));
            let mana: Vec<String> = (0..sim::TURNS)
                .map(|t| format!("Z{} {:.1}/{:.1}", t + 1, r.mana_used[t], r.mana_avail[t]))
                .collect();
            ui.label(RichText::new(format!("Mana genutzt/verfügbar: {}", mana.join(" · "))).weak());
        });
}

/// Fehlende Wildcards als Text, z. B. „2 Rare, 1 Mythic“.
pub(super) fn wildcards_text(short: &[u32; 4]) -> String {
    let parts: Vec<String> = short
        .iter()
        .enumerate()
        .filter(|(_, n)| **n > 0)
        .map(|(i, n)| format!("{n} {}", RARITY_NAMES[i]))
        .collect();
    if parts.is_empty() {
        "keine".into()
    } else {
        parts.join(", ")
    }
}

/// Wildcard-Plan: jetzt herstellbar und die Reihenfolge für den Rest.
pub(super) fn wildcard_plan_view(ui: &mut egui::Ui, plan: &budget::WildcardPlan) {
    egui::CollapsingHeader::new(RichText::new("Wildcard-Plan").strong())
        .id_salt("wildcard_plan")
        .default_open(false)
        .show(ui, |ui| {
            ui.label(format!("Schon vorhanden: {:.0} % des Decks.", plan.complete));
            if !plan.now.is_empty() {
                ui.label(RichText::new("Jetzt mit deinen Wildcards herstellen:").color(GREEN).strong());
                for s in &plan.now {
                    ui.label(format!("• {}× {} ({})", s.qty, s.name, RARITY_NAMES[s.slot]));
                }
                if let Some(last) = plan.now.last() {
                    ui.label(RichText::new(format!("Danach ist das Deck zu {:.0} % komplett.", last.complete)).weak());
                }
            }
            if !plan.later.is_empty() {
                ui.label(RichText::new("Danach, in dieser Reihenfolge:").strong());
                for s in &plan.later {
                    ui.label(format!(
                        "• {}× {} ({}) – dafür fehlen insgesamt noch: {} → {:.0} %",
                        s.qty,
                        s.name,
                        RARITY_NAMES[s.slot],
                        wildcards_text(&s.short),
                        s.complete
                    ));
                }
                ui.label(
                    RichText::new(
                        "Bis dahin ersetzt „Budget-Version bauen“ die fehlenden Karten durch ähnliche aus deiner Sammlung.",
                    )
                    .weak(),
                );
            }
            if !plan.unavailable.is_empty() {
                ui.label(
                    RichText::new(format!("Gibt es in Arena nicht: {}", plan.unavailable.join(", "))).color(YELLOW),
                );
            }
        });
}
