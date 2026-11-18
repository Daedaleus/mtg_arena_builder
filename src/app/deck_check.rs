//! Reiter „Deck verbessern“.

use super::*;

impl App {
    pub(super) fn deck_check_tab(&mut self, ui: &mut egui::Ui) {
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

        let sim_report = self.group_sims.get(&group.label).cloned();
        let mut sim_clicked = false;
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
            sim_clicked = ui
                .button("Probehände testen")
                .on_hover_text(
                    "Spielt dieses Deck 1000-mal durch – gegen einen schnellen Gegner bis Zug 6 und, mit genug \
                     eigenen Partien, gegen deine echten Gegner bis Zug 10 – und zeigt, wie oft Länder oder \
                     Farben fehlen.",
                )
                .clicked();
            if let Some(report) = &sim_report {
                super::widgets::sim_view(ui, report);
            }

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
        if sim_clicked {
            let report = sim::trial(&group.deck, db, &self.games, sim::DEFAULT_GAMES, 1);
            self.group_sims.insert(group.label.clone(), report);
        }
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
