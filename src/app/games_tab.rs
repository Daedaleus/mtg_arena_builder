//! Reiter „Meine Partien“: Kopfzeile, Filter und Auswahl der Unterseite.

use super::*;

impl App {
    pub(super) fn games_tab(&mut self, ui: &mut egui::Ui) {
        // Beim Öffnen des Reiters automatisch nachladen (höchstens einmal pro Minute).
        if self
            .games_scanned
            .is_none_or(|t| t.elapsed().as_secs() > 60)
        {
            self.scan_games();
        }
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!self.games_loading, egui::Button::new("Logs neu einlesen"))
                .clicked()
            {
                self.scan_games();
            }
            if self.games_loading {
                ui.spinner();
            }
            let mut events: Vec<String> = self.games.iter().map(|g| g.event.clone()).collect();
            events.sort();
            events.dedup();
            let shown = self
                .games_event
                .as_deref()
                .map(event_label)
                .unwrap_or_else(|| "Alle Modi".into());
            let before = self.games_event.clone();
            egui::ComboBox::from_id_salt("games_event")
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.games_event, None, "Alle Modi");
                    for e in events {
                        let label = event_label(&e);
                        ui.selectable_value(&mut self.games_event, Some(e), label);
                    }
                });
            if before != self.games_event {
                self.sel_game = None;
            }
            ui.label(RichText::new(format!("{} Partien gespeichert", self.games.len())).weak());
        });
        ui.horizontal(|ui| {
            ui.selectable_value(
                &mut self.games_view,
                0,
                RichText::new("Übersicht & Tipps").strong(),
            );
            ui.selectable_value(
                &mut self.games_view,
                1,
                RichText::new("Partienliste").strong(),
            );
            ui.selectable_value(&mut self.games_view, 2, RichText::new("Verlauf").strong());
        });
        ui.separator();
        let overview = self.games_view == 0;
        if self.logs_disabled {
            ui.label(
                RichText::new(
                    "In Arena sind „Detaillierte Logs“ ausgeschaltet – neue Partien werden nicht mitgeschrieben. \
                     Einschalten: Zahnrad > Konto > „Detaillierte Logs (Plugin-Unterstützung)“, danach Arena neu starten.",
                )
                .color(YELLOW),
            );
        }
        let Some(db) = &self.db else { return };
        let games: Vec<GameRecord> = self
            .games
            .iter()
            .filter(|g| self.games_event.as_ref().is_none_or(|e| &g.event == e))
            .cloned()
            .collect();
        if games.is_empty() {
            ui.add_space(10.0);
            ui.label("Noch keine Partien. Spiel mit eingeschalteten „Detaillierten Logs“ ein paar Runden – die App liest sie dann hier ein.");
            return;
        }
        if self.games_view == 2 {
            progress_view(ui, &games, db);
            return;
        }
        let r = games::report(&games, db);
        let shares = opponents::arch_shares(&self.meta_archetypes(), db);
        let opp_groups = opponents::analyze(&games, db, &shares);
        let opp_names: Vec<String> = games
            .iter()
            .map(|g| opponents::classify(g, db, &shares))
            .collect();
        let mut clicked: Option<usize> = None;

        egui::ScrollArea::vertical()
            .id_salt("games_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if overview {
                    clicked = self.games_overview(ui, &r, &opp_groups);
                } else {
                    clicked = self.game_list(ui, &games, db, &opp_names);
                    if let Some(i) = self.sel_game
                        && let Some(g) = games.get(i)
                    {
                        game_detail(ui, i, g, db);
                    }
                }
            });
        if let Some(i) = clicked {
            self.sel_game = Some(i);
            self.games_view = 1;
        }
    }
}
