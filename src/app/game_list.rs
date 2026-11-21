//! "My games" > "Game list": table of all games and turn details of a game.

use super::*;

impl App {
    /// Table of the games; returns the clicked game.
    pub(super) fn game_list(
        &self,
        ui: &mut egui::Ui,
        games: &[GameRecord],
        numbers: &games::GameNumbers,
        db: &CardDb,
        opp_names: &[String],
    ) -> Option<usize> {
        let mut clicked = None;
        let (start, end) = self.pager(ui, games.len());
        // --- Game list ---
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
                        tr!("game_list.when"),
                        tr!("game_list.mode"),
                        tr!("game_list.result"),
                        tr!("game_list.start"),
                        tr!("game_list.mull"),
                        tr!("game_list.turns"),
                        tr!("game_list.life"),
                        tr!("game_list.deck"),
                        tr!("game_list.opponent"),
                    ] {
                        h.col(|ui| {
                            ui.strong(title);
                        });
                    }
                })
                .body(|mut body| {
                    for (i, g) in games.iter().enumerate().take(end).skip(start) {
                        body.row(20.0, |mut row| {
                            let selected = self.games.sel == Some(i);
                            let nr = numbers.of(g);
                            if game_row(&mut row, nr, g, db, &opp_names[i], selected) {
                                clicked = Some(i);
                            }
                        });
                    }
                });
        });
        clicked
    }

    /// Paging (10 games per page); returns the shown range.
    fn pager(&self, ui: &mut egui::Ui, total: usize) -> (usize, usize) {
        let (start, end, pages, page) =
            present::page_range(total, self.games.page.get(), present::PAGE_SIZE);
        self.games.page.set(page);
        // --- Paging ---
        if pages > 1 {
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(page > 0, egui::Button::new(tr!("game_list.back")))
                    .clicked()
                {
                    self.games.page.set(page - 1);
                }
                ui.label(tr!(
                    "game_list.page_games",
                    v = page + 1,
                    pages = pages,
                    v2 = start + 1,
                    end = end,
                    games_n = total
                ));
                if ui
                    .add_enabled(page + 1 < pages, egui::Button::new(tr!("game_list.next")))
                    .clicked()
                {
                    self.games.page.set(page + 1);
                }
            });
        }
        (start, end)
    }
}

/// One row of the game list; returns whether the number was clicked.
fn game_row(
    row: &mut egui_extras::TableRow,
    nr: usize,
    g: &GameRecord,
    db: &CardDb,
    opp_name: &str,
    selected: bool,
) -> bool {
    let mut clicked = false;
    row.col(|ui| {
        clicked = ui.selectable_label(selected, format!("#{nr}")).clicked();
    });
    row.col(|ui| {
        ui.label(ago(g.started_ms));
    });
    row.col(|ui| {
        ui.label(event_label(&g.event));
    });
    row.col(|ui| {
        let (text, color) = match g.won {
            Some(true) => (tr!("game_list.win"), theme::tone(ui.ctx(), Tone::Good)),
            Some(false) if g.conceded() => {
                (tr!("game_list.conceded"), theme::tone(ui.ctx(), Tone::Bad))
            }
            Some(false) => (tr!("game_list.loss"), theme::tone(ui.ctx(), Tone::Bad)),
            None => ("?".to_string(), theme::tone(ui.ctx(), Tone::Muted)),
        };
        ui.label(RichText::new(text).color(color));
    });
    row.col(|ui| {
        ui.label(match g.on_play {
            Some(true) => tr!("game_list.play"),
            Some(false) => tr!("game_list.draw"),
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
        ui.label(opp_name);
    });
    clicked
}

impl App {
    /// Details of a game: opening hand, tips and turn table.
    pub(super) fn game_detail(
        &self,
        ui: &mut egui::Ui,
        g: &GameRecord,
        numbers: &games::GameNumbers,
        db: &CardDb,
    ) {
        ui.add_space(8.0);
        ui.separator();
        ui.heading(tr!(
            "game_list.game",
            v = numbers.of(g),
            v2 = event_label(&g.event)
        ));
        opening_hand(ui, g, db);
        self.game_tips(ui, g, numbers, db);
        biggest_threat(ui, g, db);
        ui.add_space(4.0);
        if g.turns.is_empty() {
            ui.label(tr!("game_list.no_turn_data_game"));
            return;
        }
        if !g.complete {
            ui.label(
                RichText::new(tr!(
                    "game_list.only_logged_turn_lands",
                    v = g.turns.first().map(|t| t.turn).unwrap_or(0)
                ))
                .color(theme::tone(ui.ctx(), Tone::Warn)),
            );
        }
        note(ui, tr!("game_list.state_end_each_turn"));
        turn_table(ui, numbers.of(g), g, db);
    }

    /// Tips only for this game, sorted by turn.
    fn game_tips(
        &self,
        ui: &mut egui::Ui,
        g: &GameRecord,
        numbers: &games::GameNumbers,
        db: &CardDb,
    ) {
        let single = games::report(std::slice::from_ref(g), numbers, db);
        let found: Vec<(String, &games::Finding)> = single
            .categories
            .iter()
            .flat_map(|c| c.items.iter().map(move |f| (c.title(), f)))
            .collect();
        if found.is_empty() {
            if g.complete {
                toned(ui, Tone::Good, tr!("game_list.app_found_no_typical"));
            }
        } else {
            ui.strong(tr!("game_list.what_do_better_game"));
            let mut sorted = found;
            sorted.sort_by_key(|(_, f)| f.turn);
            for (title, f) in sorted {
                let when = if f.turn == 0 {
                    tr!("game_list.opening_hand")
                } else {
                    tr!("game_list.turn", turn = f.turn)
                };
                let line = ui.label(tr!(
                    "game_list.text_2",
                    when = when,
                    title = title,
                    text = f.text
                ));
                self.cards_hover(line, &f.cards);
            }
        }
    }
}

/// Kept opening hand with land count.
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
        "game_list.opening_hand_cards_lands",
        hand_n = hand.len(),
        lands = lands,
        v = names.join(", ")
    ));
}

/// Card that dealt the most damage in this game.
fn biggest_threat(ui: &mut egui::Ui, g: &GameRecord, db: &CardDb) {
    let top = g
        .damage_taken
        .iter()
        .max_by_key(|(grp, dmg)| (**dmg, std::cmp::Reverse(**grp)))
        .and_then(|(grp, dmg)| db.cards.get(grp).map(|c| (c.display_name(), *dmg)));
    if let Some((name, dmg)) = top {
        note(ui, tr!("game_list.most_damage", name = name, dmg = dmg));
    }
}

/// Table of all turns (state at the end of the turn).
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
                    tr!("game_list.turn_2"),
                    tr!("game_list.who"),
                    tr!("game_list.life"),
                    tr!("game_list.lands"),
                    tr!("game_list.creatures"),
                    tr!("game_list.free"),
                    tr!("game_list.hand"),
                    tr!("game_list.land"),
                    tr!("game_list.spells"),
                    tr!("game_list.attack"),
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

/// One row of the turn table.
fn turn_row(row: &mut egui_extras::TableRow, t: &games::TurnStat, db: &CardDb) {
    let who = if t.mine {
        RichText::new(tr!("game_list.me")).strong()
    } else {
        RichText::new(tr!("game_list.opponent"))
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
