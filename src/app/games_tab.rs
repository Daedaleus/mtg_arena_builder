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
        ui.horizontal(|ui| self.games_toolbar(ui));
        ui.horizontal(|ui| self.games_views(ui));
        ui.separator();
        let overview = self.games.view == 0;
        if self.games.logs_disabled {
            ui.label(
                RichText::new(tr!("games_tab.arena_detaillierte_logs_ausgeschaltet")).color(YELLOW),
            );
        }
        let Some(db) = self.db.clone() else { return };
        // Zwischengespeichert; neu nur bei neuen Daten, Filter oder Sprache.
        let Some(a) = self.games_analysis() else {
            return;
        };
        if a.games.is_empty() {
            ui.add_space(10.0);
            ui.label(tr!("games_tab.noch_keine_partien_spiel"));
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
                egui::Button::new(tr!("games_tab.logs_neu_einlesen")),
            )
            .clicked()
        {
            self.scan_games();
        }
        if self.games.loading {
            ui.spinner();
        }
        let mut events: Vec<String> = self.games.list.iter().map(|g| g.event.clone()).collect();
        events.sort();
        events.dedup();
        let shown = self
            .games
            .event
            .as_deref()
            .map(event_label)
            .unwrap_or_else(|| tr!("games_tab.alle_modi"));
        let before = self.games.event.clone();
        egui::ComboBox::from_id_salt("games_event")
            .selected_text(shown)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.games.event, None, tr!("games_tab.alle_modi"));
                for e in events {
                    let label = event_label(&e);
                    ui.selectable_value(&mut self.games.event, Some(e), label);
                }
            });
        if before != self.games.event {
            self.games.sel = None;
            self.games.page.set(0);
        }
        ui.label(
            RichText::new(tr!(
                "games_tab.partien_gespeichert",
                games_n = self.games.list.len()
            ))
            .weak(),
        );
    }

    /// Unterseiten: Übersicht, Partienliste, Verlauf.
    fn games_views(&mut self, ui: &mut egui::Ui) {
        ui.selectable_value(
            &mut self.games.view,
            0,
            RichText::new(tr!("games_tab.uebersicht_tipps")).strong(),
        );
        ui.selectable_value(
            &mut self.games.view,
            1,
            RichText::new(tr!("games_tab.partienliste")).strong(),
        );
        ui.selectable_value(
            &mut self.games.view,
            2,
            RichText::new(tr!("games_tab.verlauf")).strong(),
        );
    }
}
