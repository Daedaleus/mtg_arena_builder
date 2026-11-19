//! „Meine Partien“ > „Übersicht & Tipps“: Bilanz, Verbesserungen, Gegner.

use super::*;

impl App {
    /// Übersicht; liefert die angeklickte Partie und das angeklickte
    /// empfohlene Deck.
    pub(super) fn games_overview(
        &self,
        ui: &mut egui::Ui,
        r: &games::Report,
        opp_groups: &[opponents::OppGroup],
        player: &style::PlayerStyle,
        recs: &[style::Candidate],
        loaded: usize,
    ) -> (Option<usize>, Option<u32>) {
        ui.add_space(4.0);
        balance(ui, r);
        ui.add_space(6.0);
        let open = self.style_section(ui, player, recs, loaded);
        ui.add_space(6.0);
        top_mistakes(ui, r);
        ui.add_space(8.0);
        let clicked = all_findings(ui, r);
        self.card_lists(ui, r);
        ui.add_space(6.0);
        ui.separator();
        opponents_section(ui, opp_groups);
        ui.add_space(6.0);
        (clicked, open)
    }

    /// Spielstil, Empfehlung und passende Turnierdecks. Liefert das
    /// angeklickte Deck.
    fn style_section(
        &self,
        ui: &mut egui::Ui,
        player: &style::PlayerStyle,
        recs: &[style::Candidate],
        loaded: usize,
    ) -> Option<u32> {
        let mut open = None;
        egui::CollapsingHeader::new(RichText::new(tr!("games_overview.spielstil")).strong())
            .id_salt("player_style")
            .default_open(true)
            .show(ui, |ui| {
                if !player.records.is_empty() {
                    let played: Vec<String> = player
                        .records
                        .iter()
                        .map(|s| {
                            tr!(
                                "games_overview.partien_siege",
                                style = s.style.name(),
                                games = s.games,
                                wins = s.wins
                            )
                        })
                        .collect();
                    ui.label(tr!("games_overview.gespielt", v = played.join(" · ")));
                }
                for t in &player.traits {
                    ui.label(t);
                }
                ui.label(
                    RichText::new(tr!(
                        "games_overview.empfehlung",
                        recommended = player.recommended.name(),
                        v = player.recommended.description()
                    ))
                    .color(GREEN)
                    .strong(),
                );
                ui.label(RichText::new(&player.reason).weak());
                if recs.is_empty() {
                    let text = if loaded == 0 {
                        tr!("games_overview.passende_decks_schlaegt_app").to_string()
                    } else {
                        tr!(
                            "games_overview.unter_geladenen_turnierlisten_kein",
                            loaded = loaded,
                            recommended = player.recommended.name()
                        )
                    };
                    ui.label(RichText::new(text).weak());
                    return;
                }
                ui.label(tr!(
                    "games_overview.passende_turnierdecks_sammlung_guenstigste"
                ));
                egui::Grid::new("style_recs").num_columns(3).show(ui, |ui| {
                    for c in recs {
                        if ui.link(&c.label).clicked() {
                            open = Some(c.deck_id);
                        }
                        ui.label(RichText::new(c.source.as_str()).weak());
                        let (color, text) = self.fit_text(&c.summary);
                        ui.label(RichText::new(text).color(color))
                            .on_hover_text(cost_tooltip(&c.summary));
                        ui.end_row();
                    }
                });
            });
        open
    }

    /// Wer am meisten Schaden gemacht hat und häufige Gegnerkarten.
    fn card_lists(&self, ui: &mut egui::Ui, r: &games::Report) {
        egui::CollapsingHeader::new(tr!(
            "games_overview.was_dir_meisten_schaden",
            damage_sources_n = r.damage_sources.len()
        ))
        .id_salt("damage_sources")
        .show(ui, |ui| {
            ui.label(RichText::new(tr!("games_overview.gegen_diese_karten_lohnt")).weak());
            for (name, dmg) in &r.damage_sources {
                let resp = ui.label(tr!(
                    "games_overview.schaden",
                    name = self.shown(name),
                    dmg = dmg
                ));
                self.card_hover(resp, name, None);
            }
        });
        egui::CollapsingHeader::new(tr!(
            "games_overview.haeufigste_karten_deiner_gegner",
            opp_cards_n = r.opp_cards.len()
        ))
        .id_salt("opp_cards")
        .show(ui, |ui| {
            for (name, n) in &r.opp_cards {
                let resp = ui.label(tr!(
                    "games_overview.partie_n",
                    name = self.shown(name),
                    n = n
                ));
                self.card_hover(resp, name, None);
            }
        });
    }
}

/// Siege, Niederlagen, Anfangen/Ziehen und Partielänge.
fn balance(ui: &mut egui::Ui, r: &games::Report) {
    ui.horizontal_wrapped(|ui| {
        let color = if r.wins >= r.losses { GREEN } else { RED };
        ui.label(
            RichText::new(tr!(
                "games_overview.siege_niederlagen",
                wins = r.wins,
                losses = r.losses
            ))
            .color(color)
            .strong(),
        );
        ui.label(tr!("games_overview.aus_partien", games = r.games));
        if r.conceded > 0 {
            ui.label(tr!(
                "games_overview.davon_aufgegeben",
                conceded = r.conceded
            ));
        }
    });
    ui.label(tr!("games_overview.als_erster_als_zweiter", v = ratio(r.on_play.0, r.on_play.1), v2 = ratio(r.on_draw.0, r.on_draw.1), avg_turns = r.avg_turns : {:.0}));
}

/// Die drei häufigsten Fehlerarten mit Rat.
fn top_mistakes(ui: &mut egui::Ui, r: &games::Report) {
    ui.heading(tr!("games_overview.was_als_naechstes_verbessern"));
    let top: Vec<&games::Category> = r
        .categories
        .iter()
        .filter(|c| !c.items.is_empty())
        .take(3)
        .collect();
    if top.is_empty() {
        ui.label(RichText::new(tr!("games_overview.keine_typischen_fehler_gefunden")).color(GREEN));
    }
    for (n, c) in top.iter().enumerate() {
        let in_games: HashSet<usize> = c.items.iter().map(|f| f.game).collect();
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(format!("{}. {}", n + 1, c.title()))
                    .strong()
                    .color(YELLOW),
            );
            ui.label(tr!(
                "games_overview.partien",
                items_n = c.items.len(),
                in_games_n = in_games.len(),
                games = r.games
            ));
        });
        ui.label(c.advice());
        ui.add_space(4.0);
    }
    if r.good_keeps.1 + r.bad_keeps.1 > 0 {
        ui.label(
            RichText::new(tr!(
                "games_overview.starthand_2_4_laendern",
                v = ratio(r.good_keeps.0, r.good_keeps.1),
                v2 = ratio(r.bad_keeps.0, r.bad_keeps.1)
            ))
            .weak(),
        );
    }
}

/// Alle Fehlerarten mit allen Fundstellen. Liefert die angeklickte Partie (Index).
fn all_findings(ui: &mut egui::Ui, r: &games::Report) -> Option<usize> {
    let mut clicked = None;
    ui.strong(tr!("games_overview.alle_auffaelligkeiten"));
    ui.label(
        RichText::new(tr!(
            "games_overview.zug_auswertung_ueber_eigene",
            my_turns = r.my_turns
        ))
        .weak(),
    );
    for c in &r.categories {
        let color = if c.items.is_empty() { GREEN } else { YELLOW };
        egui::CollapsingHeader::new(
            RichText::new(format!("{}: {}×", c.title(), c.items.len())).color(color),
        )
        .id_salt(c.title())
        .show(ui, |ui| {
            ui.label(RichText::new(c.advice()).weak());
            for f in &c.items {
                let when = if f.turn == 0 {
                    tr!("games_overview.starthand").to_string()
                } else {
                    tr!("games_overview.zug", turn = f.turn)
                };
                if ui
                    .link(tr!(
                        "games_overview.partie",
                        game = f.game,
                        when = when,
                        text = f.text
                    ))
                    .clicked()
                {
                    clicked = Some(f.game - 1);
                }
            }
        });
    }
    if r.discards > 0 {
        ui.label(tr!(
            "games_overview.abgeworfene_karten_weil_hand",
            discards = r.discards
        ));
    }
    clicked
}

/// Bilanz gegen die erkannten Gegnerdecks.
fn opponents_section(ui: &mut egui::Ui, opp_groups: &[opponents::OppGroup]) {
    egui::CollapsingHeader::new(
        RichText::new(tr!("games_overview.gegen_wen_gewinnst_verlierst")).strong(),
    )
    .id_salt("opponents")
    .default_open(true)
    .show(ui, |ui| {
        ui.label(RichText::new(tr!("games_overview.gegnerische_deck_erkennt_app")).weak());
        egui::Grid::new("opp_grid")
            .striped(true)
            .num_columns(4)
            .show(ui, |ui| {
                for t in [
                    tr!("game_list.gegner"),
                    tr!("games_overview.bilanz"),
                    tr!("games_overview.zuege"),
                    tr!("games_overview.hat_dir_meisten_schaden"),
                ] {
                    ui.strong(t);
                }
                ui.end_row();
                for o in opp_groups {
                    ui.label(&o.name);
                    let color = if o.wins >= o.losses() { GREEN } else { RED };
                    ui.label(
                        RichText::new(tr!(
                            "games_overview.siege_niederlagen_2",
                            wins = o.wins,
                            v = o.losses()
                        ))
                        .color(color),
                    );
                    ui.label(format!("{:.0}", o.avg_turns));
                    let threats: Vec<String> = o
                        .threats
                        .iter()
                        .map(|(n, d)| format!("{n} ({d})"))
                        .collect();
                    ui.label(if threats.is_empty() {
                        "–".into()
                    } else {
                        threats.join(", ")
                    });
                    ui.end_row();
                }
            });
    });
}
