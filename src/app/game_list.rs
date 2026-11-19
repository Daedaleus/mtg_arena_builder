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
                    .add_enabled(page > 0, egui::Button::new(tr!("game_list.zurueck")))
                    .clicked()
                {
                    self.games_page.set(page - 1);
                }
                ui.label(tr!(
                    "game_list.seite_partien",
                    v = page + 1,
                    pages = pages,
                    v2 = start + 1,
                    end = end,
                    games_n = games.len()
                ));
                if ui
                    .add_enabled(page + 1 < pages, egui::Button::new(tr!("game_list.weiter")))
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
                        "#".to_string(),
                        tr!("game_list.wann"),
                        tr!("game_list.modus"),
                        tr!("game_list.ergebnis"),
                        tr!("game_list.anfang"),
                        tr!("game_list.mull"),
                        tr!("game_list.zuege"),
                        tr!("game_list.leben"),
                        tr!("game_list.deck"),
                        tr!("game_list.gegner"),
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
                                    Some(true) => (tr!("game_list.sieg"), GREEN),
                                    Some(false) if g.conceded() => {
                                        (tr!("game_list.aufgegeben"), RED)
                                    }
                                    Some(false) => (tr!("game_list.niederlage"), RED),
                                    None => ("?".to_string(), Color32::GRAY),
                                };
                                ui.label(RichText::new(text).color(color));
                            });
                            row.col(|ui| {
                                ui.label(match g.on_play {
                                    Some(true) => tr!("game_list.erster"),
                                    Some(false) => tr!("game_list.zweiter"),
                                    None => "?".to_string(),
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
                                ui.label(tr!("game_list.text", me = me, opp = opp));
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
    ui.heading(tr!(
        "game_list.partie",
        v = i + 1,
        v2 = event_label(&g.event)
    ));
    opening_hand(ui, g, db);
    game_tips(ui, g, db);
    biggest_threat(ui, g, db);
    ui.add_space(4.0);
    if g.turns.is_empty() {
        ui.label(tr!("game_list.diese_partie_stehen_keine"));
        return;
    }
    if !g.complete {
        ui.label(
            RichText::new(tr!(
                "game_list.erst_ab_zug_mitgeschrieben",
                v = g.turns.first().map(|t| t.turn).unwrap_or(0)
            ))
            .color(YELLOW),
        );
    }
    ui.label(RichText::new(tr!("game_list.stand_jeweils_ende_zuges")).weak());
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
    let names: Vec<&str> = hand.iter().map(|c| c.display_name()).collect();
    ui.label(tr!(
        "game_list.starthand_karten_laender",
        hand_n = hand.len(),
        lands = lands,
        v = names.join(", ")
    ));
}

/// Tipps nur für diese Partie, nach Zug sortiert.
fn game_tips(ui: &mut egui::Ui, g: &GameRecord, db: &CardDb) {
    let single = games::report(std::slice::from_ref(g), db);
    let found: Vec<(String, &games::Finding)> = single
        .categories
        .iter()
        .flat_map(|c| c.items.iter().map(move |f| (c.title(), f)))
        .collect();
    if found.is_empty() {
        if g.complete {
            ui.label(RichText::new(tr!("game_list.dieser_partie_hat_app")).color(GREEN));
        }
    } else {
        ui.strong(tr!("game_list.was_dieser_partie_besser"));
        let mut sorted = found;
        sorted.sort_by_key(|(_, f)| f.turn);
        for (title, f) in sorted {
            let when = if f.turn == 0 {
                tr!("game_list.starthand").to_string()
            } else {
                tr!("game_list.zug", turn = f.turn)
            };
            ui.label(tr!(
                "game_list.text_2",
                when = when,
                title = title,
                text = f.text
            ));
        }
    }
}

/// Karte, die in dieser Partie am meisten Schaden gemacht hat.
fn biggest_threat(ui: &mut egui::Ui, g: &GameRecord, db: &CardDb) {
    let top = g
        .damage_taken
        .iter()
        .max_by_key(|(grp, dmg)| (**dmg, std::cmp::Reverse(**grp)))
        .and_then(|(grp, dmg)| db.cards.get(grp).map(|c| (c.display_name(), *dmg)));
    if let Some((name, dmg)) = top {
        ui.label(
            RichText::new(tr!("game_list.meisten_schaden_dir", name = name, dmg = dmg)).weak(),
        );
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
                    tr!("game_list.zug_kurz"),
                    tr!("game_list.wer"),
                    tr!("game_list.leben"),
                    tr!("game_list.laender"),
                    tr!("game_list.kreaturen"),
                    tr!("game_list.frei"),
                    tr!("game_list.hand"),
                    tr!("game_list.land"),
                    tr!("game_list.zauber"),
                    tr!("game_list.angriff"),
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
        RichText::new(tr!("game_list.ich")).strong()
    } else {
        RichText::new(tr!("game_list.gegner"))
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
                    .map(|c| c.display_name().to_string())
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
