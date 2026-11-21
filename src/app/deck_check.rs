//! Reiter „Deck verbessern“.

use super::*;

/// Was der Nutzer im Reiter angeklickt hat; wird nach dem Zeichnen angewandt
/// (dann sind die Borrows auf `self` frei).
#[derive(Default)]
struct Clicks {
    adopt: bool,
    simulate: bool,
}

impl App {
    pub(super) fn deck_check_tab(&mut self, ui: &mut egui::Ui) {
        if self
            .games
            .scanned
            .is_none_or(|t| t.elapsed().as_secs() > 60)
        {
            self.scan_games();
        }
        let Some(db) = self.db.clone() else { return };
        // Zwischengespeichert; neu nur bei neuen Daten, anderem Deck oder Sprache.
        let Some(a) = self.deck_analysis(self.sel_deck_group) else {
            return;
        };
        let (Some(r), Some(group)) = (a.check.as_ref(), a.groups.get(a.sel)) else {
            ui.add_space(10.0);
            ui.label(tr!("deck_check.no_games_known_decklist"));
            self.deck_cache = Some(a);
            return;
        };
        let groups = &a.groups;
        let mut sel = a.sel;
        let mut clicks = Clicks::default();
        ui.horizontal(|ui| {
            ui.label(tr!("deck_check.deck"));
            egui::ComboBox::from_id_salt("deck_group")
                .selected_text(groups[sel].label.clone())
                .width(420.0)
                .show_ui(ui, |ui| {
                    for (i, g) in groups.iter().enumerate() {
                        ui.selectable_value(&mut sel, i, g.label.clone());
                    }
                });
            clicks.adopt = ui
                .button(tr!("deck_check.copy_own_list"))
                .on_hover_text(tr!("deck_check.swap_cards_replacements_collection"))
                .clicked();
        });
        let target = SimTarget::Group(group.deck.clone());
        let running = self.trial_running(&target);
        let sim_report = self.sims.groups.get(&group.deck);

        egui::ScrollArea::vertical()
            .id_salt("deck_check_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                summary(ui, group, r);
                clicks.simulate = lands_section(ui, &r.mana, sim_report, running);
                if !group.fixed {
                    self.swaps_section(ui, &r.swaps);
                    meta_section(ui, r.meta.as_ref(), &db);
                }
                self.cards_section(ui, r, sel);
            });

        let deck_text = clicks.adopt.then(|| deckcheck::deck_text(&group.deck, &db));
        let match_id = r.meta.as_ref().map(|m| m.id);
        if clicks.simulate {
            self.start_trial(target, group.deck.clone());
        }
        self.sel_deck_group = sel;
        self.deck_cache = Some(a);
        if let Some(text) = deck_text {
            self.paste_text = text;
            self.repl.retain(|k, _| k.0 != DeckId::Pasted);
            self.subs.retain(|k, _| k.0 != DeckId::Pasted);
            self.set_list(DeckId::Pasted, deck::parse(&self.paste_text));
            self.tab = Tab::Paste;
        }
        // Für den Vergleich alle Turnierlisten des passenden Archetyps nachladen.
        if let Some(id) = match_id {
            self.queue_variants(id);
        }
    }

    /// Tausch-Vorschläge: raus mit Grund, rein mit Ersatz aus der Sammlung.
    fn swaps_section(&self, ui: &mut egui::Ui, swaps: &[deckcheck::Swap]) {
        section(ui, &tr!("deck_check.swap_suggestions"));
        if swaps.is_empty() {
            ui.label(tr!("deck_check.no_clear_candidates_yet"));
        }
        for sw in swaps {
            ui.horizontal_wrapped(|ui| {
                let resp = ui.label(
                    RichText::new(tr!("deck_check.out", out = self.shown(&sw.out)))
                        .strong()
                        .color(theme::tone(ui.ctx(), Tone::Warn)),
                );
                self.card_hover(resp, &sw.out, None);
                ui.label(format!("– {}", sw.reason));
            });
            if let Some((name, why)) = &sw.replacement {
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(16.0);
                    let resp = ui.label(
                        RichText::new(tr!("deck_check.in", name = self.shown(name)))
                            .color(theme::tone(ui.ctx(), Tone::Good)),
                    );
                    self.card_hover(resp, name, None);
                    note(ui, tr!("deck_check.text", why = why));
                });
            }
        }
    }

    /// Tabelle aller Karten im Deck mit Nutzung, Schaden und Turnier-Anteil.
    fn cards_section(&self, ui: &mut egui::Ui, r: &deckcheck::DeckCheck, sel: usize) {
        section(ui, &tr!("deck_check.cards_deck"));
        note(ui, tr!("deck_check.drawn_played_how_many"));
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
                    let mut titles = vec![
                        tr!("deck_check.qty"),
                        tr!("deck_check.card"),
                        tr!("deck_check.drawn"),
                        tr!("deck_check.played"),
                        tr!("deck_check.long_hand"),
                        tr!("deck_check.damage"),
                        tr!("deck_check.tournament"),
                    ];
                    if winrates {
                        titles.push(tr!("deck_check.win_rate"));
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
                            self.usage_row(&mut row, c, r.tracked > 0, winrates)
                        });
                    }
                });
        });
    }

    /// Eine Zeile der Kartentabelle; „–“, wo es keine Daten gibt.
    fn usage_row(
        &self,
        row: &mut egui_extras::TableRow,
        c: &deckcheck::CardStat,
        tracked: bool,
        winrates: bool,
    ) {
        let dash = |ui: &mut egui::Ui, show: bool, v: String| {
            ui.label(if show { v } else { "–".into() });
        };
        row.col(|ui| {
            ui.label(c.copies.to_string());
        });
        row.col(|ui| {
            let text = RichText::new(self.shown(&c.name));
            let resp = ui.label(if c.land {
                text.weak()
            } else {
                text.color(theme::rarity(ui.ctx(), c.rarity))
            });
            self.card_hover(resp, &c.name, None);
        });
        row.col(|ui| dash(ui, tracked, c.seen.to_string()));
        row.col(|ui| dash(ui, tracked && !c.land, c.cast.to_string()));
        row.col(|ui| {
            if c.held > 0 {
                toned(ui, Tone::Warn, c.held.to_string());
            } else {
                dash(ui, tracked && !c.land, "0".into());
            }
        });
        row.col(|ui| dash(ui, c.damage > 0, c.damage.to_string()));
        row.col(|ui| {
            dash(
                ui,
                c.meta_share.is_some(),
                format!("{:.0} %", c.meta_share.unwrap_or(0.0) * 100.0),
            );
        });
        if winrates {
            row.col(|ui| {
                dash(
                    ui,
                    c.seen > 0,
                    format!(
                        "{:.0} %",
                        100.0 * c.won_when_seen as f32 / c.seen.max(1) as f32
                    ),
                );
            });
        }
    }
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(10.0);
    ui.heading(title);
}

/// Bilanz des Decks und Hinweis, ab wann Gewinnraten je Karte erscheinen.
fn summary(ui: &mut egui::Ui, group: &deckcheck::DeckGroup, r: &deckcheck::DeckCheck) {
    if group.fixed {
        toned(ui, Tone::Warn, tr!("deck_check.deck_set_event_cannot"));
    }
    let color = if r.wins * 2 >= r.games {
        theme::tone(ui.ctx(), Tone::Good)
    } else {
        theme::tone(ui.ctx(), Tone::Bad)
    };
    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new(tr!("deck_check.wins_games", wins = r.wins, games = r.games))
                .color(color)
                .strong(),
        );
        ui.label(tr!(
            "deck_check.them_data_drawn_played",
            tracked = r.tracked
        ));
    });
    if !r.show_winrates() {
        ui.label(
            RichText::new(tr!(
                "deck_check.win_rates_single_cards",
                min_games_for_winrate = deckcheck::MIN_GAMES_FOR_WINRATE,
                tracked = r.tracked
            ))
            .weak(),
        );
    }
}

/// Länder-Urteil und Probehände. Liefert, ob „Probehände testen“ geklickt wurde.
fn lands_section(
    ui: &mut egui::Ui,
    m: &deckcheck::ManaCheck,
    sim_report: Option<&sim::Trial>,
    running: bool,
) -> bool {
    section(ui, &tr!("deck_check.lands"));
    ui.label(tr!(
        "deck_check.lands_cards_too_few",
        lands = m.lands,
        size = m.size,
        screw = m.screw,
        games = m.games,
        flood = m.flood
    ));
    let color = if m.verdict.needs_change() {
        theme::tone(ui.ctx(), Tone::Warn)
    } else {
        theme::tone(ui.ctx(), Tone::Muted)
    };
    ui.label(RichText::new(m.verdict.advice()).color(color));
    let clicked = ui
        .button(tr!("deck_check.test_sample_hands"))
        .on_hover_text(tr!("deck_check.plays_deck_1000_times"))
        .clicked();
    if running {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(tr!("common.calculating"));
        });
    }
    if let Some(report) = sim_report {
        super::widgets::sim_view(ui, report);
    }
    clicked
}

/// Vergleich mit dem ähnlichsten Turnier-Archetyp.
fn meta_section(ui: &mut egui::Ui, meta: Option<&deckcheck::MetaMatch>, db: &CardDb) {
    section(ui, &tr!("deck_check.comparison_tournament_decks"));
    let Some(m) = meta else {
        note(ui, tr!("deck_check.no_matching_tournament_archetype"));
        return;
    };
    ui.label(tr!("deck_check.most_similar_loaded_tournament", archetype = m.archetype, lists = m.lists, v = m.overlap * 100.0 : {:.0}));
    if m.missing.is_empty() {
        toned(ui, Tone::Good, tr!("deck_check.play_cards_most_these"));
        return;
    }
    ui.label(tr!("deck_check.least_half_tournament_lists"));
    for (name, share, own) in &m.missing {
        ui.horizontal(|ui| {
            ui.label(
                tr!("deck_check.lists", name = db.display_name(name), v = share * 100.0 : {:.0}),
            );
            if *own > 0 {
                toned(ui, Tone::Good, tr!("deck_check.you_have", own = own));
            } else {
                note(ui, tr!("deck_check.collection"));
            }
        });
    }
}
