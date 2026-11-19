//! Reiter „Offizielle Events“: Decklisten von magic.gg.

use super::*;

impl App {
    /// Lädt die Events des aktuellen Formats, falls noch nicht geschehen.
    fn ensure_gg_events(&mut self, force: bool) {
        let format = self.settings.format;
        if force || self.gg.format != Some(format) {
            self.gg.format = Some(format);
            self.status = tr!("events.lade_decklisten_magic_gg");
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
                ui.strong(tr!("events.events", v = self.settings.format.name()));
                ui.horizontal(|ui| {
                    if ui
                        .button(tr!("events.aktualisieren"))
                        .on_hover_text(tr!("events.liste_neu_magic_gg"))
                        .clicked()
                    {
                        self.ensure_gg_events(true);
                    }
                    ui.label(RichText::new(tr!("events.quelle_magic_gg")).weak());
                });
                ui.separator();
                let mut clicked: Option<String> = None;
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if self.gg.events.is_empty() {
                            ui.label(RichText::new(tr!("events.noch_keine_events_geladen")).weak());
                        }
                        for e in &self.gg.events {
                            let selected = self.gg.sel_event.as_deref() == Some(e.slug.as_str());
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
                    self.gg.sel_deck = None;
                    if !self.gg.decks.contains_key(&slug) && self.gg.requested.insert(slug.clone())
                    {
                        self.status = tr!("events.lade_decks_magic_gg");
                        let _ = self.tx_job.send(Job::GgDecks { slug: slug.clone() });
                    }
                    self.gg.sel_event = Some(slug);
                }
            });
    }

    /// Decks des gewählten Events und die Ansicht des gewählten Decks.
    pub(super) fn events_tab(&mut self, ui: &mut egui::Ui) {
        let Some(slug) = self.gg.sel_event.clone() else {
            ui.add_space(20.0);
            ui.label(tr!("events.links_event_waehlen_magic"));
            return;
        };
        let Some(decks) = self.gg.decks.get(&slug).cloned() else {
            ui.add_space(20.0);
            ui.label(tr!("events.decks_werden_geladen"));
            return;
        };
        let Some(db) = self.db.clone() else { return };
        let title = self
            .gg
            .events
            .iter()
            .find(|e| e.slug == slug)
            .map(|e| e.title.clone())
            .unwrap_or(slug.clone());
        ui.heading(title);
        ui.checkbox(&mut self.sort_by_fit, tr!("events.guenstigste_zuerst"));

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
                (*id, who, label, self.summary(DeckId::Gg(*id)))
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
                                .selectable_label(self.gg.sel_deck == Some(*id), label)
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
            self.gg.sel_deck = Some(id);
        }
        if let Some(id) = self.gg.sel_deck {
            ui.separator();
            self.deck_view(ui, DeckId::Gg(id));
        }
    }
}
