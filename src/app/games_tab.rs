//! Reiter „Meine Partien“: Kopfzeile, Filter und Auswahl der Unterseite.

use super::*;

impl App {
    pub(super) fn games_tab(&mut self, ui: &mut egui::Ui) {
        // Beim Öffnen des Reiters automatisch nachladen (höchstens einmal pro Minute).
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
        let overview = self.games.view == 0;
        if self.games.logs_disabled {
            ui.label(RichText::new(tr!("games_tab.detailed_logs_turned_off")).color(YELLOW));
        }
        let Some(db) = self.db.clone() else { return };
        // Zwischengespeichert; neu nur bei neuen Daten, Filter oder Sprache.
        let Some(a) = self.games_analysis() else {
            return;
        };
        if a.games.is_empty() {
            ui.add_space(10.0);
            ui.label(tr!("games_tab.no_games_yet_play"));
            self.games.cache = Some(a);
            return;
        }
        if self.games.view == 2 {
            progress_view(ui, &a.games, &db);
            self.games.cache = Some(a);
            return;
        }
        // Index in `a.games` der angeklickten Partie.
        let mut clicked: Option<usize> = None;
        let mut open: Option<u32> = None;

        egui::ScrollArea::vertical()
            .id_salt("games_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if overview {
                    let number;
                    (number, open) = self.games_overview(
                        ui,
                        &a.report,
                        &a.opp_groups,
                        &a.player,
                        &a.recs,
                        a.loaded,
                    );
                    // Fundstellen nennen die feste Nummer, nicht die Listenposition.
                    clicked =
                        number.and_then(|n| a.games.iter().position(|g| a.numbers.of(g) == n));
                } else {
                    clicked = self.game_list(ui, &a.games, &a.numbers, &db, &a.opp_names);
                    if let Some(i) = self.games.sel
                        && let Some(g) = a.games.get(i)
                    {
                        self.game_detail(ui, g, &a.numbers, &db);
                    }
                }
            });
        self.games.cache = Some(a);
        if let Some(id) = open {
            self.open_deck(id);
        }
        if let Some(i) = clicked {
            self.games.sel = Some(i);
            // Auf die Seite der angeklickten Partie springen.
            self.games.page.set(i / present::PAGE_SIZE);
            self.games.view = 1;
        }
    }

    /// Einlesen, Filter nach Arena-Event und Anzahl gespeicherter Partien.
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

    /// Beim ersten Öffnen Modus und Deck der letzten Partie wählen, damit sich
    /// verschiedene Modi und Decks nicht in einer Auswertung mischen.
    fn preset_games_filter(&mut self) {
        if self.games.filter_set {
            return;
        }
        let (Some(db), Some(last)) = (self.db.as_ref(), self.games.list.first()) else {
            return;
        };
        self.games.filter = games::GamesFilter::like(last, db);
        self.games.filter_set = true;
    }

    /// Auswahl der Modus-Gruppe; bei Wechsel gilt wieder „alle Decks“.
    fn mode_choice(&mut self, ui: &mut egui::Ui) {
        let mut modes: Vec<games::GameMode> = self
            .games
            .list
            .iter()
            .map(|g| games::GameMode::of(&g.event))
            .collect();
        modes.sort();
        modes.dedup();
        let label =
            |m: Option<games::GameMode>| m.map_or_else(|| tr!("games_tab.modes"), |m| m.label());
        let mut mode = self.games.filter.mode;
        egui::ComboBox::from_id_salt("games_mode")
            .selected_text(label(mode))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut mode, None, label(None));
                for m in modes {
                    ui.selectable_value(&mut mode, Some(m), m.label());
                }
            });
        if mode != self.games.filter.mode {
            self.games.filter = games::GamesFilter { mode, deck: None };
        }
    }

    /// Auswahl des Decks unter den Decks des gewählten Modus.
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

    /// Unterseiten: Übersicht, Partienliste, Verlauf.
    fn games_views(&mut self, ui: &mut egui::Ui) {
        ui.selectable_value(
            &mut self.games.view,
            0,
            RichText::new(tr!("games_tab.overview_tips")).strong(),
        );
        ui.selectable_value(
            &mut self.games.view,
            1,
            RichText::new(tr!("games_tab.game_list")).strong(),
        );
        ui.selectable_value(
            &mut self.games.view,
            2,
            RichText::new(tr!("games_tab.progress")).strong(),
        );
    }
}
