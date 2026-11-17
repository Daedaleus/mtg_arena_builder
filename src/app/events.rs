//! Reiter „Offizielle Events“: Decklisten von magic.gg.

use super::*;

impl App {
    /// Lädt die Events des aktuellen Formats, falls noch nicht geschehen.
    fn ensure_gg_events(&mut self, force: bool) {
        let format = self.settings.format;
        if force || self.gg_format != Some(format) {
            self.gg_format = Some(format);
            self.status = "Lade Decklisten von magic.gg …".into();
            let _ = self.tx_job.send(Job::GgEvents { format, force });
        }
    }

    /// Seitenleiste mit den Events.
    pub(super) fn events_panel(&mut self, ctx: &egui::Context) {
        self.ensure_gg_events(false);
        egui::SidePanel::left("gg_events")
            .default_width(330.0)
            .show(ctx, |ui| {
                ui.add_space(6.0);
                ui.strong(format!("Events · {}", FORMATS[self.settings.format].1));
                ui.horizontal(|ui| {
                    if ui
                        .button("Aktualisieren")
                        .on_hover_text("Liste neu von magic.gg laden")
                        .clicked()
                    {
                        self.ensure_gg_events(true);
                    }
                    ui.label(RichText::new("Quelle: magic.gg").weak());
                });
                ui.separator();
                let mut clicked: Option<String> = None;
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if self.gg_events.is_empty() {
                            ui.label(RichText::new("Noch keine Events geladen.").weak());
                        }
                        for e in &self.gg_events {
                            let selected = self.sel_gg_event.as_deref() == Some(e.slug.as_str());
                            let resp = ui.selectable_label(selected, &e.title);
                            if !e.date.is_empty() {
                                ui.label(RichText::new(&e.date).weak().small());
                            }
                            if resp.clicked() {
                                clicked = Some(e.slug.clone());
                            }
                        }
                    });
                if let Some(slug) = clicked {
                    self.sel_gg_deck = None;
                    if !self.gg_decks.contains_key(&slug) && self.gg_requested.insert(slug.clone())
                    {
                        self.status = "Lade Decks von magic.gg …".into();
                        let _ = self.tx_job.send(Job::GgDecks { slug: slug.clone() });
                    }
                    self.sel_gg_event = Some(slug);
                }
            });
    }

    /// Decks des gewählten Events und die Ansicht des gewählten Decks.
    pub(super) fn events_tab(&mut self, ui: &mut egui::Ui) {
        let Some(slug) = self.sel_gg_event.clone() else {
            ui.add_space(20.0);
            ui.label(
                "Links ein Event wählen. magic.gg veröffentlicht die Decks von Championships, \
                 Arena-Events und jede Woche Decks aus der Ranglisten-Spitze (Traditional, Bo3).",
            );
            return;
        };
        let Some(decks) = self.gg_decks.get(&slug).cloned() else {
            ui.add_space(20.0);
            ui.label("Decks werden geladen …");
            return;
        };
        let Some(db) = self.db.clone() else { return };
        let title = self
            .gg_events
            .iter()
            .find(|e| e.slug == slug)
            .map(|e| e.title.clone())
            .unwrap_or(slug.clone());
        ui.heading(title);
        ui.checkbox(&mut self.sort_by_fit, "Günstigste zuerst");

        let shares = opponents::arch_shares(&self.meta_archetypes(), &db);
        let mut rows: Vec<(u32, String, String, Option<Summary>)> = decks
            .iter()
            .map(|(id, d)| {
                // Bei Championships nennt magic.gg den Archetyp im Untertitel;
                // sonst (z. B. Ranglisten-Decks) erkennt die App ihn selbst.
                let who = d.title.clone();
                let label = if d.subtitle.is_empty() {
                    opponents::deck_label(&d.deck, &db, &shares)
                } else {
                    d.subtitle.clone()
                };
                (*id, who, label, self.summary(*id))
            })
            .collect();
        if self.sort_by_fit {
            let wc = self.wildcards();
            rows.sort_by_key(|(_, _, _, s)| (s.is_none(), s.map(|s| cost_key(&s, wc))));
        }
        let mut clicked = None;
        egui::ScrollArea::vertical()
            .id_salt("gg_decks")
            .max_height((ui.available_height() * 0.45).max(220.0))
            .auto_shrink([false, true])
            .show(ui, |ui| {
                egui::Grid::new("gg_deck_grid")
                    .striped(true)
                    .num_columns(3)
                    .show(ui, |ui| {
                        for (id, who, label, summary) in &rows {
                            if ui
                                .selectable_label(self.sel_gg_deck == Some(*id), label)
                                .clicked()
                            {
                                clicked = Some(*id);
                            }
                            ui.label(RichText::new(who).weak());
                            match summary {
                                Some(s) => {
                                    let (color, text) = self.fit_text(s);
                                    ui.label(RichText::new(text).color(color))
                                        .on_hover_text(cost_tooltip(s));
                                }
                                None => {
                                    ui.label("–");
                                }
                            }
                            ui.end_row();
                        }
                    });
            });
        if let Some(id) = clicked {
            self.sel_gg_deck = Some(id);
        }
        if let Some(id) = self.sel_gg_deck {
            ui.separator();
            self.deck_view(ui, id);
        }
    }
}
