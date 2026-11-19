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
            ui.label(tr!("deck_check.noch_keine_partien_bekannter"));
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
                .button(tr!("deck_check.eigene_liste_pruefen_uebernehmen"))
                .on_hover_text(tr!("deck_check.dort_kannst_karten_gegen"))
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
        section(ui, &tr!("deck_check.tausch_vorschlaege"));
        if swaps.is_empty() {
            ui.label(tr!("deck_check.noch_keine_klaren_kandidaten"));
        }
        for sw in swaps {
            ui.horizontal_wrapped(|ui| {
                let resp = ui.label(
                    RichText::new(tr!("deck_check.raus", out = self.shown(&sw.out)))
                        .strong()
                        .color(YELLOW),
                );
                self.card_hover(resp, &sw.out, None);
                ui.label(format!("– {}", sw.reason));
            });
            if let Some((name, why)) = &sw.replacement {
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(16.0);
                    let resp = ui.label(
                        RichText::new(tr!("deck_check.rein", name = self.shown(name))).color(GREEN),
                    );
                    self.card_hover(resp, name, None);
                    ui.label(RichText::new(tr!("deck_check.text", why = why)).weak());
                });
            }
        }
    }

    /// Tabelle aller Karten im Deck mit Nutzung, Schaden und Turnier-Anteil.
    fn cards_section(&self, ui: &mut egui::Ui, r: &deckcheck::DeckCheck, sel: usize) {
        section(ui, &tr!("deck_check.karten_deck"));
        ui.label(RichText::new(tr!("deck_check.gezogen_gespielt_wie_vielen")).weak());
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
                        tr!("deck_check.anz"),
                        tr!("deck_check.karte"),
                        tr!("deck_check.gezogen"),
                        tr!("deck_check.gespielt"),
                        tr!("deck_check.lange_hand"),
                        tr!("deck_check.schaden"),
                        tr!("deck_check.turnier"),
                    ];
                    if winrates {
                        titles.push(tr!("deck_check.siegquote"));
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
                text.color(rarity_color(c.rarity))
            });
            self.card_hover(resp, &c.name, None);
        });
        row.col(|ui| dash(ui, tracked, c.seen.to_string()));
        row.col(|ui| dash(ui, tracked && !c.land, c.cast.to_string()));
        row.col(|ui| {
            if c.held > 0 {
                ui.label(RichText::new(c.held.to_string()).color(YELLOW));
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
        ui.label(RichText::new(tr!("deck_check.dieses_deck_gibt_event")).color(YELLOW));
    }
    let color = if r.wins * 2 >= r.games { GREEN } else { RED };
    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new(tr!(
                "deck_check.siege_aus_partien",
                wins = r.wins,
                games = r.games
            ))
            .color(color)
            .strong(),
        );
        ui.label(tr!(
            "deck_check.davon_daten_gezogenen_gespielten",
            tracked = r.tracked
        ));
    });
    if !r.show_winrates() {
        ui.label(
            RichText::new(tr!(
                "deck_check.gewinnraten_einzelner_karten_zeigt",
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
    section(ui, &tr!("deck_check.laender"));
    ui.label(tr!(
        "deck_check.laender_bei_karten_wenig",
        lands = m.lands,
        size = m.size,
        screw = m.screw,
        games = m.games,
        flood = m.flood
    ));
    let color = if m.verdict.needs_change() {
        YELLOW
    } else {
        Color32::GRAY
    };
    ui.label(RichText::new(m.verdict.advice()).color(color));
    let clicked = ui
        .button(tr!("deck_check.probehaende_testen"))
        .on_hover_text(tr!("deck_check.spielt_dieses_deck_1000"))
        .clicked();
    if running {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(tr!("common.rechne"));
        });
    }
    if let Some(report) = sim_report {
        super::widgets::sim_view(ui, report);
    }
    clicked
}

/// Vergleich mit dem ähnlichsten Turnier-Archetyp.
fn meta_section(ui: &mut egui::Ui, meta: Option<&deckcheck::MetaMatch>, db: &CardDb) {
    section(ui, &tr!("deck_check.vergleich_turnierdecks"));
    let Some(m) = meta else {
        ui.label(RichText::new(tr!("deck_check.kein_passender_turnier_archetyp")).weak());
        return;
    };
    ui.label(tr!("deck_check.aehnlichsten_geladene_turnierlisten_uebereinstimmung", archetype = m.archetype, lists = m.lists, v = m.overlap * 100.0 : {:.0}));
    if m.missing.is_empty() {
        ui.label(RichText::new(tr!("deck_check.spielst_alle_karten_meisten")).color(GREEN));
        return;
    }
    ui.label(tr!("deck_check.diese_karten_spielt_mindestens"));
    for (name, share, own) in &m.missing {
        ui.horizontal(|ui| {
            ui.label(
                tr!("deck_check.listen", name = db.display_name(name), v = share * 100.0 : {:.0}),
            );
            if *own > 0 {
                ui.label(RichText::new(tr!("deck_check.hast", own = own)).color(GREEN));
            } else {
                ui.label(RichText::new(tr!("deck_check.nicht_deiner_sammlung")).weak());
            }
        });
    }
}
