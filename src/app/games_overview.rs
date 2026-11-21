//! "My games" pages: overview (key figures and short cards), improve (tips
//! and all findings), your style and opponents.

use super::*;

/// What was clicked on a page of "My games".
#[derive(Default)]
pub(super) struct PageClicks {
    /// Fixed number of a game.
    pub game: Option<usize>,
    /// Recommended deck to open.
    pub deck: Option<u32>,
    /// Sub page to switch to.
    pub view: Option<GamesView>,
}

impl App {
    /// Overview: key figures as tiles, then the top tips and the style in
    /// short, each with a link to its page.
    pub(super) fn overview_page(
        &self,
        ui: &mut egui::Ui,
        r: &games::Report,
        player: &style::PlayerStyle,
    ) -> PageClicks {
        let mut clicks = PageClicks::default();
        ui.add_space(4.0);
        key_figures(ui, r);
        ui.add_space(theme::SECTION_GAP);
        ui.columns(2, |cols| {
            card(&mut cols[0], tr!("games_overview.top_tips"), |ui| {
                top_tips_short(ui, r);
                if ui.link(tr!("games_overview.all_findings")).clicked() {
                    clicks.view = Some(GamesView::Improve);
                }
            });
            card(&mut cols[1], tr!("games_tab.style"), |ui| {
                style_short(ui, player);
                if ui.link(tr!("games_overview.style_and_decks")).clicked() {
                    clicks.view = Some(GamesView::Style);
                }
            });
        });
        clicks
    }

    /// Improve: the most important kinds of mistakes and all findings.
    pub(super) fn improve_page(&self, ui: &mut egui::Ui, r: &games::Report) -> PageClicks {
        ui.add_space(4.0);
        top_mistakes(ui, r);
        ui.add_space(theme::SECTION_GAP);
        PageClicks {
            game: all_findings(self, ui, r),
            ..Default::default()
        }
    }

    /// Opponents: record per opposing deck, damage sources, common cards.
    pub(super) fn opponents_page(
        &self,
        ui: &mut egui::Ui,
        r: &games::Report,
        opp_groups: &[opponents::OppGroup],
    ) {
        ui.add_space(4.0);
        opponents_section(ui, opp_groups);
        ui.add_space(theme::SECTION_GAP);
        self.card_lists(ui, r);
    }

    /// Your style: play style, recommendation and matching tournament decks.
    pub(super) fn style_page(
        &self,
        ui: &mut egui::Ui,
        player: &style::PlayerStyle,
        recs: &[style::Candidate],
        loaded: usize,
    ) -> PageClicks {
        let mut clicks = PageClicks::default();
        ui.add_space(4.0);
        if !player.records.is_empty() {
            ui.label(tr!("games_overview.played", v = played_styles(player)));
        }
        for t in &player.traits {
            ui.label(t);
        }
        ui.add_space(theme::SECTION_GAP);
        ui.label(
            RichText::new(tr!(
                "games_overview.recommendation",
                recommended = player.recommended.name(),
                v = player.recommended.description()
            ))
            .color(theme::tone(ui.ctx(), Tone::Good))
            .strong(),
        );
        note(ui, &player.reason);
        ui.add_space(theme::SECTION_GAP);
        if recs.is_empty() {
            let text = if loaded == 0 {
                tr!("games_overview.app_suggests_matching_decks")
            } else {
                tr!(
                    "games_overview.no_deck_among_loaded",
                    loaded = loaded,
                    recommended = player.recommended.name()
                )
            };
            note(ui, text);
            return clicks;
        }
        ui.label(tr!("games_overview.matching_tournament_decks_cheapest"));
        egui::Grid::new("style_recs").num_columns(3).show(ui, |ui| {
            for c in recs {
                if ui.link(&c.label).clicked() {
                    clicks.deck = Some(c.deck_id);
                }
                note(ui, c.source.as_str());
                let (color, text) = self.fit_text(&c.summary);
                ui.label(RichText::new(text).color(color))
                    .on_hover_text(cost_tooltip(&c.summary));
                ui.end_row();
            }
        });
        clicks
    }

    /// Who dealt the most damage and common opponent cards.
    fn card_lists(&self, ui: &mut egui::Ui, r: &games::Report) {
        egui::CollapsingHeader::new(tr!(
            "games_overview.what_dealt_most_damage",
            damage_sources_n = r.damage_sources.len()
        ))
        .id_salt("damage_sources")
        .show(ui, |ui| {
            note(ui, tr!("games_overview.against_these_cards_pays"));
            for (name, dmg) in &r.damage_sources {
                let resp = ui.label(tr!(
                    "games_overview.damage",
                    name = self.shown(name),
                    dmg = dmg
                ));
                self.card_hover(resp, name, None);
            }
        });
        egui::CollapsingHeader::new(tr!(
            "games_overview.most_common_cards_opponents",
            opp_cards_n = r.opp_cards.len()
        ))
        .id_salt("opp_cards")
        .show(ui, |ui| {
            for (name, n) in &r.opp_cards {
                let resp = ui.label(tr!("games_overview.game_2", name = self.shown(name), n = n));
                self.card_hover(resp, name, None);
            }
        });
    }
}

/// Key figures as tiles: win rate, on the play / on the draw, game length,
/// conceded games.
fn key_figures(ui: &mut egui::Ui, r: &games::Report) {
    let decided = r.wins + r.losses;
    let rate = if decided == 0 {
        "–".to_string()
    } else {
        format!("{:.0} %", r.wins as f32 * 100.0 / decided as f32)
    };
    let tone = if r.wins >= r.losses {
        Tone::Good
    } else {
        Tone::Bad
    };
    let fraction = |(won, total): (usize, usize)| format!("{won}/{total}");
    ui.horizontal_wrapped(|ui| {
        stat_tile(
            ui,
            tr!("games_overview.tile_win_rate"),
            rate,
            Some(tone),
            tr!(
                "games_overview.wins_losses",
                wins = r.wins,
                losses = r.losses
            ),
        );
        stat_tile(
            ui,
            tr!("games_overview.tile_play_draw"),
            format!("{} · {}", fraction(r.on_play), fraction(r.on_draw)),
            None,
            tr!("games_overview.tile_play_draw_sub"),
        );
        stat_tile(
            ui,
            tr!("games_overview.tile_turns"),
            format!("{:.0}", r.avg_turns),
            None,
            tr!("games_overview.games", games = r.games),
        );
        stat_tile(
            ui,
            tr!("games_overview.tile_conceded"),
            r.conceded.to_string(),
            (r.conceded > 0).then_some(Tone::Warn),
            tr!("games_overview.tile_conceded_sub"),
        );
    });
}

/// The three most frequent kinds of mistakes, one line each.
fn top_tips_short(ui: &mut egui::Ui, r: &games::Report) {
    let top: Vec<&games::Category> = r
        .categories
        .iter()
        .filter(|c| !c.items.is_empty())
        .take(3)
        .collect();
    if top.is_empty() {
        toned(
            ui,
            Tone::Good,
            tr!("games_overview.no_typical_mistakes_found"),
        );
    }
    for (n, c) in top.iter().enumerate() {
        toned(
            ui,
            Tone::Warn,
            format!("{}. {} ({}×)", n + 1, c.title(), c.items.len()),
        );
    }
}

/// Played styles and the recommendation in short.
fn style_short(ui: &mut egui::Ui, player: &style::PlayerStyle) {
    if !player.records.is_empty() {
        ui.label(played_styles(player));
    }
    toned(
        ui,
        Tone::Good,
        tr!(
            "games_overview.recommended_short",
            recommended = player.recommended.name()
        ),
    );
}

/// "Midrange 9 games (1 wins) · Aggro …"
fn played_styles(player: &style::PlayerStyle) -> String {
    player
        .records
        .iter()
        .map(|s| {
            tr!(
                "games_overview.games_wins",
                style = s.style.name(),
                games = s.games,
                wins = s.wins
            )
        })
        .collect::<Vec<String>>()
        .join(" · ")
}

/// The three most frequent kinds of mistakes with advice.
fn top_mistakes(ui: &mut egui::Ui, r: &games::Report) {
    ui.heading(tr!("games_overview.what_improve_next"));
    let top: Vec<&games::Category> = r
        .categories
        .iter()
        .filter(|c| !c.items.is_empty())
        .take(3)
        .collect();
    if top.is_empty() {
        toned(
            ui,
            Tone::Good,
            tr!("games_overview.no_typical_mistakes_found"),
        );
    }
    for (n, c) in top.iter().enumerate() {
        let in_games: HashSet<usize> = c.items.iter().map(|f| f.game).collect();
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(format!("{}. {}", n + 1, c.title()))
                    .strong()
                    .color(theme::tone(ui.ctx(), Tone::Warn)),
            );
            ui.label(tr!(
                "games_overview.games_2",
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
                "games_overview.opening_hand_2_4",
                v = ratio(r.good_keeps.0, r.good_keeps.1),
                v2 = ratio(r.bad_keeps.0, r.bad_keeps.1)
            ))
            .weak(),
        );
    }
}

/// All kinds of mistakes with all findings (card images on hover).
/// Returns the fixed number of the clicked game.
fn all_findings(app: &App, ui: &mut egui::Ui, r: &games::Report) -> Option<usize> {
    let mut clicked = None;
    ui.strong(tr!("games_overview.findings"));
    ui.label(
        RichText::new(tr!(
            "games_overview.turn_analysis_over_turns",
            my_turns = r.my_turns
        ))
        .weak(),
    );
    for c in &r.categories {
        let color = if c.items.is_empty() {
            theme::tone(ui.ctx(), Tone::Good)
        } else {
            theme::tone(ui.ctx(), Tone::Warn)
        };
        egui::CollapsingHeader::new(
            RichText::new(format!("{}: {}×", c.title(), c.items.len())).color(color),
        )
        .id_salt(c.title())
        .show(ui, |ui| {
            note(ui, c.advice());
            for f in &c.items {
                let when = if f.turn == 0 {
                    tr!("games_overview.opening_hand")
                } else {
                    tr!("games_overview.turn", turn = f.turn)
                };
                let link = ui.link(tr!(
                    "games_overview.game",
                    game = f.game,
                    when = when,
                    text = f.text
                ));
                if app.cards_hover(link, &f.cards).clicked() {
                    clicked = Some(f.game);
                }
            }
        });
    }
    if r.discards > 0 {
        ui.label(tr!(
            "games_overview.discarded_cards_e_g",
            discards = r.discards
        ));
    }
    clicked
}

/// Record against the recognised opponent decks.
fn opponents_section(ui: &mut egui::Ui, opp_groups: &[opponents::OppGroup]) {
    ui.strong(tr!("games_overview.who_win_lose_against"));
    note(ui, tr!("games_overview.app_recognizes_opponents_deck"));
    egui::Grid::new("opp_grid")
        .striped(true)
        .num_columns(4)
        .show(ui, |ui| {
            for t in [
                tr!("game_list.opponent"),
                tr!("games_overview.record"),
                tr!("games_overview.turns"),
                tr!("games_overview.dealt_most_damage"),
            ] {
                ui.strong(t);
            }
            ui.end_row();
            for o in opp_groups {
                ui.label(&o.name);
                let color = if o.wins >= o.losses() {
                    theme::tone(ui.ctx(), Tone::Good)
                } else {
                    theme::tone(ui.ctx(), Tone::Bad)
                };
                ui.label(
                    RichText::new(tr!(
                        "games_overview.wins_losses_2",
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
}
