//! Tab "My games": header, filters and choice of the sub page.

use super::*;

impl App {
    pub(super) fn games_tab(&mut self, ui: &mut egui::Ui) {
        // Reload automatically when the tab opens (at most once per minute).
        if self
            .games
            .scanned
            .is_none_or(|t| t.elapsed().as_secs() > 60)
        {
            self.scan_games();
        }
        self.preset_games_filter();
        ui.horizontal(|ui| self.games_toolbar(ui));
        ui.horizontal(|ui| self.games_views(ui));
        ui.separator();
        if self.games.logs_disabled {
            toned(ui, Tone::Warn, tr!("games_tab.detailed_logs_turned_off"));
        }
        let Some(db) = self.db.clone() else { return };
        // Cached; recomputed only for new data, filter or language.
        let Some(a) = self.games_analysis() else {
            return;
        };
        if a.games.is_empty() {
            ui.add_space(10.0);
            ui.label(tr!("games_tab.no_games_yet_play"));
            self.games.cache = Some(a);
            return;
        }
        if self.games.view == GamesView::Progress {
            progress_view(ui, &a.games, &db);
            self.games.cache = Some(a);
            return;
        }
        // Index in `a.games` of the clicked game.
        let mut clicked: Option<usize> = None;
        let mut clicks = PageClicks::default();

        egui::ScrollArea::vertical()
            .id_salt("games_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| match self.games.view {
                GamesView::Overview => clicks = self.overview_page(ui, &a.report, &a.player),
                GamesView::Improve => clicks = self.improve_page(ui, &a.report),
                GamesView::Style => {
                    clicks = self.style_page(ui, &a.player, &a.recs, a.loaded);
                }
                GamesView::Opponents => self.opponents_page(ui, &a.report, &a.opp_groups),
                GamesView::List | GamesView::Progress => {
                    clicked = self.game_list(ui, &a.games, &a.numbers, &db, &a.opp_names);
                    if let Some(i) = self.games.sel
                        && let Some(g) = a.games.get(i)
                    {
                        self.game_detail(ui, g, &a.numbers, &db);
                    }
                }
            });
        // Findings name the fixed number, not the list position.
        if let Some(n) = clicks.game {
            clicked = a.games.iter().position(|g| a.numbers.of(g) == n);
        }
        self.games.cache = Some(a);
        if let Some(id) = clicks.deck {
            self.open_deck(id);
        }
        if let Some(view) = clicks.view {
            self.games.view = view;
        }
        if let Some(i) = clicked {
            self.games.sel = Some(i);
            // Jump to the page of the clicked game.
            self.games.page.set(i / present::PAGE_SIZE);
            self.games.view = GamesView::List;
        }
    }

    /// Reading, filters and number of stored games.
    fn games_toolbar(&mut self, ui: &mut egui::Ui) {
        if ui
            .add_enabled(
                !self.games.loading,
                egui::Button::new(tr!("games_tab.re_read_logs")),
            )
            .clicked()
        {
            self.scan_games();
        }
        if self.games.loading {
            ui.spinner();
        }
        let before = self.games.filter.clone();
        self.mode_choice(ui);
        self.deck_choice(ui);
        if before != self.games.filter {
            self.games.sel = None;
            self.games.page.set(0);
        }
        ui.label(
            RichText::new(tr!(
                "games_tab.games_saved",
                games_n = self.games.list.len()
            ))
            .weak(),
        );
    }

    /// On first opening show the games of the chosen game mode, so different
    /// modes don't mix in one analysis.
    fn preset_games_filter(&mut self) {
        if self.games.filter_set {
            return;
        }
        self.games.filter = games::GamesFilter::of_format(self.settings.mode);
        self.games.filter_set = true;
    }

    /// The game mode of the app or all games; on change "all decks" applies
    /// again.
    fn mode_choice(&mut self, ui: &mut egui::Ui) {
        let label = |m: Option<GameFormat>| m.map_or_else(|| tr!("games_tab.modes"), |m| m.label());
        let mut format = self.games.filter.format;
        egui::ComboBox::from_id_salt("games_mode")
            .selected_text(label(format))
            .show_ui(ui, |ui| {
                let mode = Some(self.settings.mode);
                ui.selectable_value(&mut format, mode, label(mode));
                ui.selectable_value(&mut format, None, label(None));
            });
        if format != self.games.filter.format {
            self.games.filter = games::GamesFilter { format, deck: None };
        }
    }

    /// Choice of the deck among the decks of the chosen mode.
    fn deck_choice(&mut self, ui: &mut egui::Ui) {
        let decks = self
            .games
            .cache
            .as_ref()
            .map(|a| a.decks.clone())
            .unwrap_or_default();
        let shown = self
            .games
            .filter
            .deck
            .as_ref()
            .and_then(|k| decks.iter().find(|(_, key)| key == k))
            .map_or_else(|| tr!("games_tab.decks"), |(label, _)| label.clone());
        let mut deck = self.games.filter.deck.clone();
        egui::ComboBox::from_id_salt("games_deck")
            .selected_text(shown)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut deck, None, tr!("games_tab.decks"));
                for (label, key) in decks {
                    ui.selectable_value(&mut deck, Some(key), label);
                }
            });
        self.games.filter.deck = deck;
    }

    /// Sub pages: overview, improve, style, opponents, game list, progress.
    fn games_views(&mut self, ui: &mut egui::Ui) {
        for view in GamesView::ALL {
            ui.selectable_value(
                &mut self.games.view,
                view,
                RichText::new(view.label()).strong(),
            );
        }
    }
}
