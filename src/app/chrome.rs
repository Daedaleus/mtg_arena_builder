//! Kopfzeile, Seitenleiste, Einstellungen und „Erste Schritte“.

use super::*;

impl App {
    // ---------------------------------------------------------------- UI ----

    pub(super) fn top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.heading(RichText::new("MTG Arena Builder").strong());
                ui.separator();
                match &self.collection {
                    Some(c) => {
                        ui.label(format!("{} Karten", c.total_cards()))
                            .on_hover_text(format!("Sammlung gelesen {}", c.age_text()));
                        ui.label(RichText::new("Wildcards").weak());
                        for (i, n) in c.wildcards.as_array().iter().enumerate() {
                            ui.label(
                                RichText::new(format!("{} {n}", ["C", "U", "R", "M"][i]))
                                    .color(rarity_color(i as u8 + RARITY_COMMON))
                                    .strong(),
                            )
                            .on_hover_text(format!("{n} {}-Wildcards", RARITY_NAMES[i]));
                        }
                    }
                    None => {
                        ui.label(RichText::new("Sammlung noch nicht gelesen").color(YELLOW));
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("⚙ Einstellungen").clicked() {
                        self.show_settings = !self.show_settings;
                    }
                    if ui
                        .toggle_value(&mut self.settings.companion, "Begleitfenster")
                        .on_hover_text(
                            "Kleines Fenster, das während einer Partie live Hinweise gibt (Land legen, gefahrlos angreifen, \
                             tödlicher Angriff …). Es liest nur das Arena-Log. Arena dafür im Fenstermodus oder randlosen Vollbild spielen.",
                        )
                        .changed()
                    {
                        self.save_settings();
                    }
                    let button = egui::Button::new(if self.syncing { "Lese …" } else { "Sammlung aktualisieren" });
                    let hint = "Liest Karten und Wildcards aus dem laufenden MTG Arena (Hauptmenü). Falls Windows nach \
                                Administratorrechten fragt: Die werden nur gebraucht, um den Speicher des Spiels zu lesen.";
                    if ui.add_enabled(!self.syncing, button).on_hover_text(hint).clicked() {
                        self.sync_collection(ctx);
                    }
                    let before = self.settings.format;
                    egui::ComboBox::from_id_salt("format")
                        .selected_text(FORMATS[self.settings.format].1)
                        .show_ui(ui, |ui| {
                            for (i, f) in FORMATS.iter().enumerate() {
                                ui.selectable_value(&mut self.settings.format, i, f.1);
                            }
                        })
                        .response
                        .on_hover_text("Format für Turnierdecks, Legalität der Ersatzkarten und Gegner-Erkennung");
                    if before != self.settings.format {
                        self.save_settings();
                        self.load_format(false);
                    }
                    ui.label("Format:");
                });
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let tabs = [
                    (Tab::Meta, "Turnierdecks", "Aktuelle Turnierdecks: Was kannst du nachbauen, was kostet es?"),
                    (Tab::Events, "Offizielle Events", "Decklisten von magic.gg: Championships, Arena-Events und Ranglisten-Decks"),
                    (Tab::Built, "Aus meiner Sammlung", "Decks nur aus Karten, die du schon besitzt"),
                    (Tab::Paste, "Eigene Liste", "Eine Deckliste einfügen und gegen deine Sammlung prüfen"),
                    (Tab::Games, "Meine Partien", "Bilanz und Tipps aus deinen gespielten Partien"),
                    (Tab::DeckCheck, "Deck verbessern", "Welche Karten deines Decks du tauschen solltest"),
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
            });
            ui.add_space(4.0);
        });
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if self.pending > 0 || self.syncing {
                    ui.spinner();
                }
                if self.pending > 0 {
                    ui.label(format!("{} Abrufe in der Warteschlange ·", self.pending));
                }
                ui.label(&self.status);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(&self.db_status).weak());
                });
            });
        });
    }

    pub(super) fn side_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("archetypes").default_width(310.0).show(ctx, |ui| {
            ui.add_space(6.0);
            ui.strong(format!("Archetypen · {}", FORMATS[self.settings.format].1));
            ui.horizontal(|ui| {
                let todo: Vec<Archetype> =
                    self.archetypes.iter().filter(|a| !self.decks.contains_key(&a.id)).cloned().collect();
                let hint = "Lädt zu jedem Archetyp das neueste Turnierdeck und prüft es gegen deine Sammlung. Dauert ca. 10 s pro Archetyp (MTGTop8 erlaubt nur eine Anfrage alle 5 s); Ergebnisse werden zwischengespeichert.";
                if ui.add_enabled(!todo.is_empty(), egui::Button::new("Alle bewerten")).on_hover_text(hint).clicked() {
                    self.status = format!("Bewerte {} Archetypen …", todo.len());
                    for arch in todo {
                        let _ = self.tx_job.send(Job::Decks { format: self.settings.format, arch, urgent: false });
                    }
                }
                if ui.button("Aktualisieren").on_hover_text("Metagame neu von MTGTop8 laden").clicked() {
                    self.load_format(true);
                }
            });
            ui.checkbox(&mut self.sort_by_fit, "Günstigste zuerst")
                .on_hover_text("Sortiert nach benötigten Wildcards (Rares und Mythics zuerst).");
            ui.separator();

            let mut rows: Vec<(Archetype, Option<Summary>)> =
                self.archetypes.iter().map(|a| (a.clone(), self.best_of_arch(a.id))).collect();
            if self.sort_by_fit {
                let wc = self.wildcards();
                rows.sort_by_key(|(a, s)| (s.is_none(), s.map(|s| cost_key(&s, wc)), std::cmp::Reverse(a.share)));
            }
            let mut clicked = None;
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                if rows.is_empty() {
                    ui.label(RichText::new("Noch keine Archetypen geladen.").weak());
                }
                for (arch, best) in &rows {
                    ui.horizontal(|ui| {
                        let text = format!("{:>2} %  {}", arch.share, arch.name);
                        if ui.selectable_label(self.sel_arch == Some(arch.id), text).clicked() {
                            clicked = Some(arch.clone());
                        }
                        if let Some(s) = best {
                            let (color, text) = self.fit_text(s);
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(RichText::new(text).color(color)).on_hover_text(format!(
                                    "Günstigste bisher geladene Liste\n{}",
                                    cost_tooltip(s)
                                ));
                            });
                        }
                    });
                }
            });
            if let Some(arch) = clicked {
                self.tab = Tab::Meta;
                self.sel_arch = Some(arch.id);
                self.sel_deck = self.decks.get(&arch.id).and_then(|d| d.first()).map(|d| d.id);
                if self.decks.contains_key(&arch.id) {
                    self.queue_variants(arch.id);
                } else {
                    self.status = format!("Lade Decks für {} …", arch.name);
                    let _ = self.tx_job.send(Job::Decks { format: self.settings.format, arch, urgent: true });
                }
            }
        });
    }

    pub(super) fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.show_settings {
            return;
        }
        let mut open = true;
        let mut reload_db = false;
        egui::Window::new("Einstellungen")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(440.0)
            .show(ctx, |ui| {
                ui.strong("Darstellung");
                let mut zoom = self.settings.zoom;
                if ui
                    .add(egui::Slider::new(&mut zoom, 0.8..=1.6).step_by(0.05).text("Textgröße"))
                    .changed()
                {
                    self.settings.zoom = zoom;
                    ctx.set_zoom_factor(zoom);
                }
                ui.separator();
                ui.strong("Decks exportieren");
                ui.checkbox(&mut self.settings.include_side, "Sideboard einbeziehen")
                    .on_hover_text("Für Best-of-One ausschalten – dann zählt das Sideboard auch nicht bei den Wildcards.");
                ui.checkbox(&mut self.settings.with_sets, "Mit Set-Kürzeln exportieren")
                    .on_hover_text("Empfohlen, wenn Arena nicht auf Englisch läuft.");
                ui.separator();
                ui.strong("Während der Partie");
                if ui
                    .checkbox(&mut self.settings.companion, "Begleitfenster anzeigen")
                    .changed()
                {
                    self.save_settings();
                }
                if self.logs_disabled {
                    ui.label(
                        RichText::new("„Detaillierte Logs“ sind in Arena aus – ohne sie bleiben Begleitfenster und Partien leer.")
                            .color(YELLOW),
                    );
                }
                ui.separator();
                ui.strong("MTG Arena");
                ui.label("Installationsordner (leer = automatisch erkennen):");
                ui.text_edit_singleline(&mut self.settings.mtga_dir);
                ui.horizontal(|ui| {
                    reload_db = ui.button("Kartendaten neu laden").clicked();
                    ui.label(RichText::new(&self.db_status).weak());
                });
                ui.separator();
                if ui.button("„Erste Schritte“ wieder anzeigen").clicked() {
                    self.settings.onboarding_hidden = false;
                }
            });
        if reload_db {
            self.save_settings();
            self.load_db();
        }
        if !open {
            self.show_settings = false;
            self.save_settings();
        }
    }

    /// Checkliste für den Einstieg, solange noch etwas fehlt.
    pub(super) fn onboarding(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.settings.onboarding_hidden {
            return;
        }
        let logs_known = self.games_scanned.is_some() && !self.games_loading;
        let steps = [
            (
                self.db.is_some(),
                "Kartendaten von MTG Arena gefunden",
                "Installationsordner in den Einstellungen angeben.",
            ),
            (
                self.collection.is_some(),
                "Sammlung gelesen",
                "Arena starten, ins Hauptmenü gehen und „Sammlung aktualisieren“ klicken.",
            ),
            (
                logs_known && !self.logs_disabled,
                "„Detaillierte Logs“ in Arena aktiv",
                "In Arena: Zahnrad > Konto > „Detaillierte Logs (Plugin-Unterstützung)“, dann Arena neu starten.",
            ),
            (
                !self.archetypes.is_empty(),
                "Turnierdecks geladen",
                "Internetverbindung prüfen, dann „Aktualisieren“ unter Turnierdecks.",
            ),
        ];
        if steps.iter().all(|(done, _, _)| *done) {
            return;
        }
        let mut sync = false;
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.strong("Erste Schritte");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("Ausblenden").clicked() {
                        self.settings.onboarding_hidden = true;
                    }
                });
            });
            for (i, (done, title, how)) in steps.iter().enumerate() {
                ui.horizontal_wrapped(|ui| {
                    if *done {
                        ui.label(RichText::new(format!("✔ {title}")).color(GREEN));
                    } else {
                        ui.label(RichText::new(format!("○ {title}")).strong());
                        ui.label(RichText::new(*how).weak());
                        if i == 1 && !self.syncing {
                            sync = ui.small_button("Jetzt lesen").clicked();
                        }
                    }
                });
            }
        });
        ui.add_space(6.0);
        if sync {
            self.sync_collection(ctx);
        }
    }
}
