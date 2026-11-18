//! „Meine Partien“ > „Partienliste“: Tabelle aller Partien und Zugdetails einer Partie.

use super::*;

impl App {
    /// Tabelle der Partien; liefert die angeklickte Partie.
    pub(super) fn game_list(
        &self,
        ui: &mut egui::Ui,
        games: &[GameRecord],
        db: &CardDb,
        opp_names: &[String],
    ) -> Option<usize> {
        let mut clicked = None;
        let (start, end, pages, page) =
            present::page_range(games.len(), self.games_page.get(), present::PAGE_SIZE);
        self.games_page.set(page);
        // --- Blättern ---
        if pages > 1 {
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(page > 0, egui::Button::new("Zurück"))
                    .clicked()
                {
                    self.games_page.set(page - 1);
                }
                ui.label(format!(
                    "Seite {} von {} · Partien {}–{} von {}",
                    page + 1,
                    pages,
                    start + 1,
                    end,
                    games.len()
                ));
                if ui
                    .add_enabled(page + 1 < pages, egui::Button::new("Weiter"))
                    .clicked()
                {
                    self.games_page.set(page + 1);
                }
            });
        }
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
                    for title in [
                        "#", "Wann", "Modus", "Ergebnis", "Anfang", "Mull.", "Züge", "Leben",
                        "Deck", "Gegner",
                    ] {
                        h.col(|ui| {
                            ui.strong(title);
                        });
                    }
                })
                .body(|mut body| {
                    for (i, g) in games.iter().enumerate().take(end).skip(start) {
                        body.row(20.0, |mut row| {
                            row.col(|ui| {
                                if ui
                                    .selectable_label(
                                        self.sel_game == Some(i),
                                        format!("#{}", i + 1),
                                    )
                                    .clicked()
                                {
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
                                ui.label(if g.turns.is_empty() {
                                    "–".into()
                                } else {
                                    g.last_turn().to_string()
                                });
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
        clicked
    }
}

/// Details einer Partie: Starthand, Tipps und Zugtabelle.
pub(super) fn game_detail(ui: &mut egui::Ui, i: usize, g: &GameRecord, db: &CardDb) {
    ui.add_space(8.0);
    ui.separator();
    ui.heading(format!("Partie #{} – {}", i + 1, event_label(&g.event)));
    opening_hand(ui, g, db);
    game_tips(ui, g, db);
    biggest_threat(ui, g, db);
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
    ui.label(
        RichText::new("Stand jeweils am Ende des Zuges. „Frei“ = ungetappte eigene Länder.").weak(),
    );
    turn_table(ui, i, g, db);
}

/// Behaltene Starthand mit Länderzahl.
fn opening_hand(ui: &mut egui::Ui, g: &GameRecord, db: &CardDb) {
    if g.opening_hand.is_empty() {
        return;
    }
    let hand: Vec<&crate::carddb::Card> = g
        .opening_hand
        .iter()
        .filter_map(|grp| db.cards.get(grp))
        .collect();
    let lands = hand.iter().filter(|c| c.is_land()).count();
    let names: Vec<&str> = hand.iter().map(|c| c.name.as_str()).collect();
    ui.label(format!(
        "Starthand ({} Karten, {lands} Länder): {}",
        hand.len(),
        names.join(", ")
    ));
}

/// Tipps nur für diese Partie, nach Zug sortiert.
fn game_tips(ui: &mut egui::Ui, g: &GameRecord, db: &CardDb) {
    let single = games::report(std::slice::from_ref(g), db);
    let found: Vec<(&str, &games::Finding)> = single
        .categories
        .iter()
        .flat_map(|c| c.items.iter().map(move |f| (c.title(), f)))
        .collect();
    if found.is_empty() {
        if g.complete {
            ui.label(
                RichText::new("In dieser Partie hat die App keine typischen Fehler gefunden.")
                    .color(GREEN),
            );
        }
    } else {
        ui.strong("Was du in dieser Partie besser machen kannst");
        let mut sorted = found;
        sorted.sort_by_key(|(_, f)| f.turn);
        for (title, f) in sorted {
            let when = if f.turn == 0 {
                "Starthand".to_string()
            } else {
                format!("Zug {}", f.turn)
            };
            ui.label(format!("• {when} – {title}: {}", f.text));
        }
    }
}

/// Karte, die in dieser Partie am meisten Schaden gemacht hat.
fn biggest_threat(ui: &mut egui::Ui, g: &GameRecord, db: &CardDb) {
    let top = g
        .damage_taken
        .iter()
        .max_by_key(|(grp, dmg)| (**dmg, std::cmp::Reverse(**grp)))
        .and_then(|(grp, dmg)| db.cards.get(grp).map(|c| (c.name.as_str(), *dmg)));
    if let Some((name, dmg)) = top {
        ui.label(RichText::new(format!("Am meisten Schaden an dir: {name} ({dmg})")).weak());
    }
}

/// Tabelle aller Züge (Stand am Zugende).
fn turn_table(ui: &mut egui::Ui, i: usize, g: &GameRecord, db: &CardDb) {
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
                for title in [
                    "Zug",
                    "Wer",
                    "Leben",
                    "Länder",
                    "Kreaturen",
                    "Frei",
                    "Hand",
                    "Land",
                    "Zauber",
                    "Angriff",
                ] {
                    h.col(|ui| {
                        ui.strong(title);
                    });
                }
            })
            .body(|mut body| {
                for t in &g.turns {
                    body.row(20.0, |mut row| turn_row(&mut row, t, db));
                }
            });
    });
}

/// Eine Zeile der Zugtabelle.
fn turn_row(row: &mut egui_extras::TableRow, t: &games::TurnStat, db: &CardDb) {
    let who = if t.mine {
        RichText::new("ich").strong()
    } else {
        RichText::new("Gegner")
    };
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
            .map(|g| {
                db.cards
                    .get(g)
                    .map(|c| c.name.clone())
                    .unwrap_or_else(|| "?".into())
            })
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
}
