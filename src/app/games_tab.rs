//! Reiter „Meine Partien“: Übersicht, Partienliste und Verlauf.

use super::*;

impl App {
    pub(super) fn games_tab(&mut self, ui: &mut egui::Ui) {
        // Beim Öffnen des Reiters automatisch nachladen (höchstens einmal pro Minute).
        if self
            .games_scanned
            .is_none_or(|t| t.elapsed().as_secs() > 60)
        {
            self.scan_games();
        }
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!self.games_loading, egui::Button::new("Logs neu einlesen"))
                .clicked()
            {
                self.scan_games();
            }
            if self.games_loading {
                ui.spinner();
            }
            let mut events: Vec<String> = self.games.iter().map(|g| g.event.clone()).collect();
            events.sort();
            events.dedup();
            let shown = self
                .games_event
                .as_deref()
                .map(event_label)
                .unwrap_or_else(|| "Alle Modi".into());
            let before = self.games_event.clone();
            egui::ComboBox::from_id_salt("games_event")
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.games_event, None, "Alle Modi");
                    for e in events {
                        let label = event_label(&e);
                        ui.selectable_value(&mut self.games_event, Some(e), label);
                    }
                });
            if before != self.games_event {
                self.sel_game = None;
            }
            ui.label(RichText::new(format!("{} Partien gespeichert", self.games.len())).weak());
        });
        ui.horizontal(|ui| {
            ui.selectable_value(
                &mut self.games_view,
                0,
                RichText::new("Übersicht & Tipps").strong(),
            );
            ui.selectable_value(
                &mut self.games_view,
                1,
                RichText::new("Partienliste").strong(),
            );
            ui.selectable_value(&mut self.games_view, 2, RichText::new("Verlauf").strong());
        });
        ui.separator();
        let overview = self.games_view == 0;
        if self.logs_disabled {
            ui.label(
                RichText::new(
                    "In Arena sind „Detaillierte Logs“ ausgeschaltet – neue Partien werden nicht mitgeschrieben. \
                     Einschalten: Zahnrad > Konto > „Detaillierte Logs (Plugin-Unterstützung)“, danach Arena neu starten.",
                )
                .color(YELLOW),
            );
        }
        let Some(db) = &self.db else { return };
        let games: Vec<GameRecord> = self
            .games
            .iter()
            .filter(|g| self.games_event.as_ref().is_none_or(|e| &g.event == e))
            .cloned()
            .collect();
        if games.is_empty() {
            ui.add_space(10.0);
            ui.label("Noch keine Partien. Spiel mit eingeschalteten „Detaillierten Logs“ ein paar Runden – die App liest sie dann hier ein.");
            return;
        }
        if self.games_view == 2 {
            progress_view(ui, &games, db);
            return;
        }
        let r = games::report(&games, db);
        let shares = opponents::arch_shares(&self.meta_archetypes(), db);
        let opp_groups = opponents::analyze(&games, db, &shares);
        let opp_names: Vec<String> = games
            .iter()
            .map(|g| opponents::classify(g, db, &shares))
            .collect();
        let mut clicked: Option<usize> = None;

        egui::ScrollArea::vertical().id_salt("games_scroll").auto_shrink([false, false]).show(ui, |ui| {
            if overview {
            // --- Bilanz ---
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                let color = if r.wins >= r.losses { GREEN } else { RED };
                ui.label(RichText::new(format!("{} Siege, {} Niederlagen", r.wins, r.losses)).color(color).strong());
                ui.label(format!("aus {} Partien", r.games));
                if r.conceded > 0 {
                    ui.label(format!("· {} davon aufgegeben", r.conceded));
                }
            });
            ui.label(format!(
                "Als Erster: {} · Als Zweiter: {} · Partien dauern im Schnitt {:.0} Züge",
                ratio(r.on_play.0, r.on_play.1),
                ratio(r.on_draw.0, r.on_draw.1),
                r.avg_turns
            ));
            ui.add_space(6.0);

            // --- Größte Baustellen ---
            ui.heading("Was du als Nächstes verbessern kannst");
            let top: Vec<&games::Category> =
                r.categories.iter().filter(|c| !c.items.is_empty()).take(3).collect();
            if top.is_empty() {
                ui.label(
                    RichText::new("Keine typischen Fehler gefunden. Die App erkennt allerdings nur einfache Muster.")
                        .color(GREEN),
                );
            }
            for (n, c) in top.iter().enumerate() {
                let in_games: HashSet<usize> = c.items.iter().map(|f| f.game).collect();
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(format!("{}. {}", n + 1, c.title)).strong().color(YELLOW));
                    ui.label(format!("– {}× in {} von {} Partien", c.items.len(), in_games.len(), r.games));
                });
                ui.label(c.advice);
                ui.add_space(4.0);
            }
            if r.good_keeps.1 + r.bad_keeps.1 > 0 {
                ui.label(
                    RichText::new(format!(
                        "Starthand mit 2–4 Ländern: {} · andere Starthände: {}",
                        ratio(r.good_keeps.0, r.good_keeps.1),
                        ratio(r.bad_keeps.0, r.bad_keeps.1)
                    ))
                    .weak(),
                );
            }
            ui.add_space(8.0);

            // --- Alle Auffälligkeiten ---
            ui.strong("Alle Auffälligkeiten");
            ui.label(
                RichText::new(format!(
                    "Zug-Auswertung über {} eigene Züge aus vollständig mitgeschriebenen Partien. Ein Klick auf einen Eintrag öffnet die Partie.",
                    r.my_turns
                ))
                .weak(),
            );
            for c in &r.categories {
                let color = if c.items.is_empty() { GREEN } else { YELLOW };
                egui::CollapsingHeader::new(RichText::new(format!("{}: {}×", c.title, c.items.len())).color(color))
                    .id_salt(c.title)
                    .show(ui, |ui| {
                        ui.label(RichText::new(c.advice).weak());
                        for f in &c.items {
                            let when = if f.turn == 0 { "Starthand".to_string() } else { format!("Zug {}", f.turn) };
                            if ui.link(format!("Partie #{}, {when}: {}", f.game, f.text)).clicked() {
                                clicked = Some(f.game - 1);
                            }
                        }
                    });
            }
            if r.discards > 0 {
                ui.label(format!("Abgeworfene Karten: {} (z. B. weil die Hand am Zugende zu voll war)", r.discards));
            }
            egui::CollapsingHeader::new(format!("Was dir am meisten Schaden gemacht hat ({})", r.damage_sources.len()))
                .id_salt("damage_sources")
                .show(ui, |ui| {
                    ui.label(
                        RichText::new("Gegen diese Karten lohnt es sich, Removal aufzuheben oder zu blocken.").weak(),
                    );
                    for (name, dmg) in &r.damage_sources {
                        let resp = ui.label(format!("{name} – {dmg} Schaden"));
                        self.card_hover(resp, name, None);
                    }
                });
            egui::CollapsingHeader::new(format!("Häufigste Karten deiner Gegner ({})", r.opp_cards.len()))
                .id_salt("opp_cards")
                .show(ui, |ui| {
                    for (name, n) in &r.opp_cards {
                        let resp = ui.label(format!("{name} – in {n} Partie(n)"));
                        self.card_hover(resp, name, None);
                    }
                });
            ui.add_space(6.0);
            ui.separator();

            // --- Gegner ---
            egui::CollapsingHeader::new(RichText::new("Gegen wen du gewinnst und verlierst").strong())
                .id_salt("opponents")
                .default_open(true)
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(
                            "Das gegnerische Deck erkennt die App an den Karten, die du gesehen hast: passend zu einem geladenen Turnier-Archetyp („erkannt“), sonst nach Farben.",
                        )
                        .weak(),
                    );
                    egui::Grid::new("opp_grid").striped(true).num_columns(4).show(ui, |ui| {
                        for t in ["Gegner", "Bilanz", "Ø Züge", "Hat dir den meisten Schaden gemacht"] {
                            ui.strong(t);
                        }
                        ui.end_row();
                        for o in &opp_groups {
                            ui.label(&o.name);
                            let color = if o.wins >= o.losses() { GREEN } else { RED };
                            ui.label(RichText::new(format!("{} Siege, {} Niederlagen", o.wins, o.losses())).color(color));
                            ui.label(format!("{:.0}", o.avg_turns));
                            let threats: Vec<String> =
                                o.threats.iter().map(|(n, d)| format!("{n} ({d})")).collect();
                            ui.label(if threats.is_empty() { "–".into() } else { threats.join(", ") });
                            ui.end_row();
                        }
                    });
                });
            ui.add_space(6.0);

            }
            if !overview {
            // --- Partienliste ---
            ui.push_id("games_table", |ui| {
                TableBuilder::new(ui)
                    .striped(true)
                    .vscroll(false)
                    .column(Column::auto().at_least(34.0))
                    .column(Column::auto().at_least(80.0))
                    .column(Column::auto().at_least(120.0))
                    .column(Column::auto().at_least(90.0))
                    .column(Column::auto().at_least(70.0))
                    .column(Column::auto().at_least(40.0))
                    .column(Column::auto().at_least(40.0))
                    .column(Column::auto().at_least(60.0))
                    .column(Column::auto().at_least(50.0))
                    .column(Column::remainder().at_least(50.0))
                    .header(20.0, |mut h| {
                        for title in ["#", "Wann", "Modus", "Ergebnis", "Anfang", "Mull.", "Züge", "Leben", "Deck", "Gegner"] {
                            h.col(|ui| {
                                ui.strong(title);
                            });
                        }
                    })
                    .body(|mut body| {
                        for (i, g) in games.iter().enumerate() {
                            body.row(20.0, |mut row| {
                                row.col(|ui| {
                                    if ui.selectable_label(self.sel_game == Some(i), format!("#{}", i + 1)).clicked() {
                                        clicked = Some(i);
                                    }
                                });
                                row.col(|ui| {
                                    ui.label(ago(g.started_ms));
                                });
                                row.col(|ui| {
                                    ui.label(event_label(&g.event));
                                });
                                row.col(|ui| {
                                    let (text, color) = match g.won {
                                        Some(true) => ("Sieg", GREEN),
                                        Some(false) if g.conceded() => ("Aufgegeben", RED),
                                        Some(false) => ("Niederlage", RED),
                                        None => ("?", Color32::GRAY),
                                    };
                                    ui.label(RichText::new(text).color(color));
                                });
                                row.col(|ui| {
                                    ui.label(match g.on_play {
                                        Some(true) => "Erster",
                                        Some(false) => "Zweiter",
                                        None => "?",
                                    });
                                });
                                row.col(|ui| {
                                    ui.label(g.mulligans.to_string());
                                });
                                row.col(|ui| {
                                    ui.label(if g.turns.is_empty() { "–".into() } else { g.last_turn().to_string() });
                                });
                                row.col(|ui| {
                                    let (me, opp) = g.final_life();
                                    ui.label(format!("{me} : {opp}"));
                                });
                                row.col(|ui| {
                                    ui.label(games::deck_colors(&g.deck, db));
                                });
                                row.col(|ui| {
                                    ui.label(&opp_names[i]);
                                });
                            });
                        }
                    });
            });

            // --- Zugdetails ---
            if let Some(g) = self.sel_game.and_then(|i| games.get(i).map(|g| (i, g))) {
                let (i, g) = g;
                ui.add_space(8.0);
                ui.separator();
                ui.heading(format!("Partie #{} – {}", i + 1, event_label(&g.event)));
                if !g.opening_hand.is_empty() {
                    let hand: Vec<&crate::carddb::Card> =
                        g.opening_hand.iter().filter_map(|grp| db.cards.get(grp)).collect();
                    let lands = hand.iter().filter(|c| c.is_land()).count();
                    let names: Vec<&str> = hand.iter().map(|c| c.name.as_str()).collect();
                    ui.label(format!("Starthand ({} Karten, {lands} Länder): {}", hand.len(), names.join(", ")));
                }
                // Tipps nur für diese Partie
                let single = games::report(std::slice::from_ref(g), db);
                let found: Vec<(&str, &games::Finding)> = single
                    .categories
                    .iter()
                    .flat_map(|c| c.items.iter().map(move |f| (c.title, f)))
                    .collect();
                if found.is_empty() {
                    if g.complete {
                        ui.label(
                            RichText::new("In dieser Partie hat die App keine typischen Fehler gefunden.").color(GREEN),
                        );
                    }
                } else {
                    ui.strong("Was du in dieser Partie besser machen kannst");
                    let mut sorted = found;
                    sorted.sort_by_key(|(_, f)| f.turn);
                    for (title, f) in sorted {
                        let when = if f.turn == 0 { "Starthand".to_string() } else { format!("Zug {}", f.turn) };
                        ui.label(format!("• {when} – {title}: {}", f.text));
                    }
                }
                if let Some((name, dmg)) = {
                    let mut d: Vec<(u32, u32)> = g.damage_taken.iter().map(|(k, v)| (*k, *v)).collect();
                    d.sort_by_key(|(_, v)| std::cmp::Reverse(*v));
                    d.first().and_then(|(grp, v)| db.cards.get(grp).map(|c| (c.name.clone(), *v)))
                } {
                    ui.label(RichText::new(format!("Am meisten Schaden an dir: {name} ({dmg})")).weak());
                }
                ui.add_space(4.0);
                if g.turns.is_empty() {
                    ui.label("Für diese Partie stehen keine Zugdaten im Log.");
                    return;
                }
                if !g.complete {
                    ui.label(
                        RichText::new(format!(
                            "Erst ab Zug {} mitgeschrieben – Länder und Handkarten aus der Zeit davor fehlen.",
                            g.turns.first().map(|t| t.turn).unwrap_or(0)
                        ))
                        .color(YELLOW),
                    );
                }
                ui.label(RichText::new("Stand jeweils am Ende des Zuges. „Frei“ = ungetappte eigene Länder.").weak());
                ui.push_id(("turns", i), |ui| {
                    TableBuilder::new(ui)
                        .striped(true)
                        .vscroll(false)
                        .column(Column::auto().at_least(36.0))
                        .column(Column::auto().at_least(56.0))
                        .column(Column::auto().at_least(64.0))
                        .column(Column::auto().at_least(56.0))
                        .column(Column::auto().at_least(70.0))
                        .column(Column::auto().at_least(36.0))
                        .column(Column::auto().at_least(40.0))
                        .column(Column::auto().at_least(40.0))
                        .column(Column::auto().at_least(46.0))
                        .column(Column::remainder().at_least(50.0))
                        .header(20.0, |mut h| {
                            for title in ["Zug", "Wer", "Leben", "Länder", "Kreaturen", "Frei", "Hand", "Land", "Zauber", "Angriff"] {
                                h.col(|ui| {
                                    ui.strong(title);
                                });
                            }
                        })
                        .body(|mut body| {
                            for t in &g.turns {
                                body.row(20.0, |mut row| {
                                    let who = if t.mine { RichText::new("ich").strong() } else { RichText::new("Gegner") };
                                    row.col(|ui| {
                                        ui.label(t.turn.to_string());
                                    });
                                    row.col(|ui| {
                                        ui.label(who);
                                    });
                                    row.col(|ui| {
                                        ui.label(format!("{} : {}", t.life_me, t.life_opp));
                                    });
                                    row.col(|ui| {
                                        ui.label(format!("{} : {}", t.lands_me, t.lands_opp));
                                    });
                                    row.col(|ui| {
                                        ui.label(format!("{} : {}", t.creatures_me, t.creatures_opp));
                                    });
                                    row.col(|ui| {
                                        if t.mine {
                                            ui.label(t.untapped_lands.to_string());
                                        }
                                    });
                                    row.col(|ui| {
                                        let names: Vec<String> = t
                                            .hand
                                            .iter()
                                            .map(|g| db.cards.get(g).map(|c| c.name.clone()).unwrap_or_else(|| "?".into()))
                                            .collect();
                                        let label = ui.label(t.hand.len().to_string());
                                        if !names.is_empty() {
                                            label.on_hover_text(names.join("\n"));
                                        }
                                    });
                                    row.col(|ui| {
                                        ui.label(if t.land_played { "ja" } else { "–" });
                                    });
                                    row.col(|ui| {
                                        ui.label(t.spells.to_string());
                                    });
                                    row.col(|ui| {
                                        ui.label(if t.attacked { "ja" } else { "–" });
                                    });
                                });
                            }
                        });
                });
            }
            }
        });
        if let Some(i) = clicked {
            self.sel_game = Some(i);
            self.games_view = 1;
        }
    }
}

/// Gleitendes Fenster der Siegquote im Verlauf.
const TREND_WINDOW: usize = 10;

/// „Meine Partien“ > „Verlauf“: Rang und Siegquote über die Zeit.
pub(super) fn progress_view(ui: &mut egui::Ui, games: &[GameRecord], db: &CardDb) {
    use egui_plot::{Legend, Line, Plot, PlotPoints};
    egui::ScrollArea::vertical()
        .id_salt("progress_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            // --- Rang ---
            ui.heading("Rang");
            let latest = games
                .iter()
                .filter(|g| g.rank_after.is_some())
                .max_by_key(|g| g.started_ms)
                .and_then(|g| g.rank_after.as_ref());
            match latest {
                Some(r) => {
                    ui.label(
                        RichText::new(format!(
                            "Constructed: {} · Saison {}: {} Siege, {} Niederlagen in der Rangliste",
                            progress::rank_label(r),
                            r.season,
                            r.won,
                            r.lost
                        ))
                        .strong(),
                    );
                    ui.label(format!(
                        "Limited: {}",
                        progress::queue_label(r, progress::Queue::Limited)
                    ));
                }
                None => {
                    ui.label("Noch kein Rang bekannt – Arena meldet ihn nach jeder Partie im Log.");
                }
            }
            let ranks = progress::rank_series(games);
            if ranks.len() >= 2 {
                Plot::new("rank_plot")
                    .height(200.0)
                    .include_y(0.0)
                    .include_y(4.0)
                    .allow_scroll(false)
                    .y_axis_formatter(|mark, _| progress::points_label(mark.value))
                    .x_axis_label("Partie")
                    .legend(Legend::default())
                    .show(ui, |plot| {
                        plot.line(Line::new(PlotPoints::from(ranks)).name("Constructed").color(GREEN));
                        plot.line(
                            Line::new(PlotPoints::from(progress::queue_series(
                                games,
                                progress::Queue::Limited,
                            )))
                            .name("Limited")
                            .color(YELLOW),
                        );
                    });
            }

            // --- Siegquote ---
            ui.add_space(8.0);
            ui.heading("Siegquote");
            if let Some((recent, before)) = progress::trend(games, TREND_WINDOW) {
                let color = if recent > before + 0.5 {
                    GREEN
                } else if recent + 0.5 < before {
                    RED
                } else {
                    YELLOW
                };
                ui.label(
                    RichText::new(format!(
                        "Letzte {TREND_WINDOW} Partien: {recent:.0} % gewonnen – davor {before:.0} %"
                    ))
                    .color(color)
                    .strong(),
                );
            }
            ui.label(
                RichText::new(format!(
                    "Gleitend über die letzten {TREND_WINDOW} Partien; Decks mit mindestens 3 Partien einzeln."
                ))
                .weak(),
            );
            let overall = progress::winrate_series(games, TREND_WINDOW, |_| true);
            if overall.len() < 2 {
                ui.label("Für einen Verlauf braucht es mindestens 2 beendete Partien.");
                return;
            }
            let groups = deckcheck::group_decks(games, db);
            let decks = progress::deck_series(games, &groups, TREND_WINDOW, 3);
            Plot::new("winrate_plot")
                .height(260.0)
                .include_y(0.0)
                .include_y(100.0)
                .allow_scroll(false)
                .legend(Legend::default())
                .y_axis_formatter(|mark, _| format!("{:.0} %", mark.value))
                .x_axis_label("Partie")
                .show(ui, |plot| {
                    plot.line(
                        Line::new(PlotPoints::from(overall))
                            .name("Alle Partien")
                            .width(2.5_f32),
                    );
                    for (label, series) in decks {
                        plot.line(Line::new(PlotPoints::from(series)).name(label));
                    }
                });
        });
}
