//! Kopfzeile, Seitenleiste, Einstellungen und „Erste Schritte“.

use super::*;

impl App {
    // ---------------------------------------------------------------- UI ----

    pub(super) fn top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.heading(RichText::new(tr!("chrome.mtg_arena_builder")).strong());
                ui.separator();
                collection_summary(ui, self.collection.as_ref());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    self.header_controls(ui);
                });
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| self.tab_bar(ui));
            ui.add_space(4.0);
        });
        self.status_bar(ctx);
    }

    /// Rechts oben: Einstellungen, Begleitfenster, Sammlung lesen, Format.
    fn header_controls(&mut self, ui: &mut egui::Ui) {
        if ui.button(tr!("chrome.einstellungen")).clicked() {
            self.show_settings = !self.show_settings;
        }
        if ui
            .toggle_value(&mut self.settings.companion, tr!("chrome.begleitfenster"))
            .on_hover_text(tr!("chrome.kleines_fenster_waehrend_einer"))
            .changed()
        {
            self.save_settings();
        }
        let button = egui::Button::new(if self.syncing {
            tr!("chrome.lese")
        } else {
            tr!("chrome.sammlung_aktualisieren")
        });
        let hint = tr!("chrome.liest_karten_wildcards_aus");
        if ui
            .add_enabled(!self.syncing, button)
            .on_hover_text(hint)
            .clicked()
        {
            self.sync_collection();
        }
        let before = self.settings.format;
        egui::ComboBox::from_id_salt("format")
            .selected_text(self.settings.format.name())
            .show_ui(ui, |ui| {
                for f in Format::ALL {
                    ui.selectable_value(&mut self.settings.format, f, f.name());
                }
            })
            .response
            .on_hover_text(tr!("chrome.format_turnierdecks_legalitaet_ersatzkarten"));
        if before != self.settings.format {
            self.save_settings();
            self.load_format(false);
        }
        ui.label(tr!("chrome.format"));
    }

    /// Die Reiter.
    fn tab_bar(&mut self, ui: &mut egui::Ui) {
        let tabs = [
            (
                Tab::Meta,
                tr!("chrome.turnierdecks"),
                tr!("chrome.aktuelle_turnierdecks_was_kannst"),
            ),
            (
                Tab::Events,
                tr!("chrome.offizielle_events"),
                tr!("chrome.decklisten_magic_gg_championships"),
            ),
            (
                Tab::Built,
                tr!("chrome.aus_meiner_sammlung"),
                tr!("chrome.decks_nur_aus_karten"),
            ),
            (
                Tab::Paste,
                tr!("chrome.eigene_liste"),
                tr!("chrome.deckliste_einfuegen_gegen_sammlung"),
            ),
            (
                Tab::Games,
                tr!("chrome.meine_partien"),
                tr!("chrome.bilanz_tipps_aus_deinen"),
            ),
            (
                Tab::DeckCheck,
                tr!("chrome.deck_verbessern"),
                tr!("chrome.welche_karten_deines_decks"),
            ),
        ];
        for (tab, label, hint) in tabs {
            let text = RichText::new(label).size(15.0);
            if ui
                .add(egui::SelectableLabel::new(self.tab == tab, text))
                .on_hover_text(hint)
                .clicked()
            {
                self.tab = tab;
            }
        }
    }

    /// Statuszeile unten: laufende Abrufe, Meldung, Kartendaten.
    fn status_bar(&self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if self.tour.pending > 0 || self.syncing {
                    ui.spinner();
                }
                if self.tour.pending > 0 {
                    ui.label(tr!(
                        "chrome.abrufe_warteschlange",
                        pending = self.tour.pending
                    ));
                }
                ui.label(&self.status);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(self.db_status()).weak());
                });
            });
        });
    }

    pub(super) fn side_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("archetypes")
            .default_width(310.0)
            .show(ctx, |ui| {
                ui.add_space(6.0);
                ui.strong(tr!("chrome.archetypen", v = self.settings.format.name()));
                ui.horizontal(|ui| self.archetype_buttons(ui));
                ui.checkbox(&mut self.sort_by_fit, tr!("chrome.guenstigste_zuerst"))
                    .on_hover_text(tr!("chrome.sortiert_nach_benoetigten_wildcards"));
                ui.separator();

                let mut rows: Vec<(Archetype, Option<Summary>)> = self
                    .tour
                    .archetypes
                    .iter()
                    .map(|a| (a.clone(), self.best_of_arch(a.id)))
                    .collect();
                if self.sort_by_fit {
                    let wc = self.wildcards();
                    rows.sort_by_key(|(a, s)| {
                        (
                            s.is_none(),
                            s.map(|s| cost_key(&s, wc)),
                            std::cmp::Reverse(a.share),
                        )
                    });
                }
                let mut clicked = None;
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if rows.is_empty() {
                            ui.label(
                                RichText::new(tr!("chrome.noch_keine_archetypen_geladen")).weak(),
                            );
                        }
                        for (arch, best) in &rows {
                            ui.horizontal(|ui| {
                                let text = format!("{:>2} %  {}", arch.share, arch.name);
                                if ui
                                    .selectable_label(self.tour.sel_arch == Some(arch.id), text)
                                    .clicked()
                                {
                                    clicked = Some(arch.clone());
                                }
                                if let Some(s) = best {
                                    let (color, text) = self.fit_text(s);
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            ui.label(RichText::new(text).color(color))
                                                .on_hover_text(tr!(
                                                    "chrome.guenstigste_bisher_geladene_liste",
                                                    v = cost_tooltip(s)
                                                ));
                                        },
                                    );
                                }
                            });
                        }
                    });
                if let Some(arch) = clicked {
                    self.select_archetype(arch);
                }
            });
    }

    /// „Alle bewerten“ und „Aktualisieren“ über der Archetyp-Liste.
    fn archetype_buttons(&mut self, ui: &mut egui::Ui) {
        let todo: Vec<Archetype> = self
            .tour
            .archetypes
            .iter()
            .filter(|a| !self.tour.decks.contains_key(&a.id))
            .cloned()
            .collect();
        let hint = tr!("chrome.laedt_jedem_archetyp_neueste");
        if ui
            .add_enabled(
                !todo.is_empty(),
                egui::Button::new(tr!("chrome.alle_bewerten")),
            )
            .on_hover_text(hint)
            .clicked()
        {
            self.status = tr!("chrome.bewerte_archetypen", todo_n = todo.len());
            self.tour.rating_all = true;
            for arch in todo {
                let _ = self.tx_job.send(Job::Decks {
                    format: self.settings.format,
                    arch,
                    urgent: false,
                });
            }
        }
        if ui
            .button(tr!("chrome.aktualisieren"))
            .on_hover_text(tr!("chrome.metagame_neu_mtgtop8_laden"))
            .clicked()
        {
            self.load_format(true);
        }
    }

    /// Archetyp auswählen; seine Decks laden oder weitere Listen nachladen.
    pub(super) fn select_archetype(&mut self, arch: Archetype) {
        self.tab = Tab::Meta;
        self.tour.sel_arch = Some(arch.id);
        self.tour.sel_deck = self
            .tour
            .decks
            .get(&arch.id)
            .and_then(|d| d.first())
            .map(|d| d.id);
        if self.tour.decks.contains_key(&arch.id) {
            self.queue_variants(arch.id);
        } else {
            self.status = tr!("chrome.lade_decks", name = arch.name);
            let _ = self.tx_job.send(Job::Decks {
                format: self.settings.format,
                arch,
                urgent: true,
            });
        }
    }

    /// Checkliste für den Einstieg, solange noch etwas fehlt.
    pub(super) fn onboarding(&mut self, ui: &mut egui::Ui) {
        if self.settings.onboarding_hidden {
            return;
        }
        let logs_known = self.games.scanned.is_some() && !self.games.loading;
        let steps = [
            (
                self.db.is_some(),
                tr!("chrome.kartendaten_mtg_arena_gefunden"),
                tr!("chrome.installationsordner_einstellungen_angeben"),
            ),
            (
                self.collection.is_some(),
                tr!("chrome.sammlung_gelesen_2"),
                tr!("chrome.arena_starten_ins_hauptmenue"),
            ),
            (
                logs_known && !self.games.logs_disabled,
                tr!("chrome.detaillierte_logs_arena_aktiv"),
                tr!("chrome.arena_zahnrad_konto_detaillierte"),
            ),
            (
                !self.tour.archetypes.is_empty(),
                tr!("chrome.turnierdecks_geladen"),
                tr!("chrome.internetverbindung_pruefen_dann_aktualisieren"),
            ),
        ];
        if steps.iter().all(|(done, _, _)| *done) {
            return;
        }
        let mut sync = false;
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.strong(tr!("chrome.erste_schritte"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button(tr!("chrome.ausblenden")).clicked() {
                        self.settings.onboarding_hidden = true;
                    }
                });
            });
            for (i, (done, title, how)) in steps.iter().enumerate() {
                ui.horizontal_wrapped(|ui| {
                    if *done {
                        ui.label(RichText::new(tr!("chrome.text", title = title)).color(GREEN));
                    } else {
                        ui.label(RichText::new(tr!("chrome.text_2", title = title)).strong());
                        ui.label(RichText::new(how.as_str()).weak());
                        if i == 1 && !self.syncing {
                            sync = ui.small_button(tr!("chrome.jetzt_lesen")).clicked();
                        }
                    }
                });
            }
        });
        ui.add_space(6.0);
        if sync {
            self.sync_collection();
        }
    }
}

/// Kopfzeile: Kartenzahl und Wildcards der Sammlung.
fn collection_summary(ui: &mut egui::Ui, collection: Option<&Collection>) {
    match collection {
        Some(c) => {
            ui.label(tr!("chrome.karten", v = c.total_cards()))
                .on_hover_text(tr!("chrome.sammlung_gelesen", v = c.age_text()));
            ui.label(RichText::new(tr!("chrome.wildcards_2")).weak());
            for (i, n) in c.wildcards.as_array().iter().enumerate() {
                ui.label(
                    RichText::new(format!("{} {n}", ["C", "U", "R", "M"][i]))
                        .color(rarity_color(Rarity::WILDCARDS[i]))
                        .strong(),
                )
                .on_hover_text(tr!(
                    "chrome.wildcards",
                    n = n,
                    rarity_names = Rarity::WILDCARDS[i].label()
                ));
            }
        }
        None => {
            ui.label(RichText::new(tr!("chrome.sammlung_noch_nicht_gelesen")).color(YELLOW));
        }
    }
}
