//! Top bar, side panel, settings and "Getting started".

use super::*;

impl App {
    // ---------------------------------------------------------------- UI ----

    pub(super) fn top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.heading(RichText::new(tr!("chrome.mtg_arena_builder")).strong());
                self.mode_switch(ui);
                ui.separator();
                collection_summary(ui, self.collection.as_ref());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    self.header_controls(ui);
                });
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| self.area_bar(ui));
            if self.tab.area() == nav::Area::Decks {
                ui.horizontal(|ui| self.sub_bar(ui));
            }
            ui.add_space(4.0);
        });
        self.status_bar(ctx);
    }

    /// Top right: settings, companion, read collection.
    fn header_controls(&mut self, ui: &mut egui::Ui) {
        if ui.button(tr!("chrome.settings")).clicked() {
            self.show_settings = !self.show_settings;
        }
        if ui
            .toggle_value(&mut self.settings.companion, tr!("chrome.companion"))
            .on_hover_text(tr!("chrome.small_window_gives_live"))
            .changed()
        {
            self.save_settings();
        }
        let button = egui::Button::new(if self.syncing {
            tr!("chrome.reading")
        } else {
            tr!("chrome.update_collection")
        });
        let hint = tr!("chrome.reads_cards_wildcards_running");
        if ui
            .add_enabled(!self.syncing, button)
            .on_hover_text(hint)
            .clicked()
        {
            self.sync_collection();
        }
    }

    /// Status line at the bottom: running fetches, message, card data.
    fn status_bar(&self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if self.tour.pending > 0 || self.syncing {
                    ui.spinner();
                }
                if self.tour.pending > 0 {
                    ui.label(tr!("chrome.requests_queued", pending = self.tour.pending));
                }
                ui.label(&self.status);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    note(ui, self.db_status());
                });
            });
        });
    }

    pub(super) fn side_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("archetypes")
            .default_width(310.0)
            .show(ctx, |ui| {
                ui.add_space(6.0);
                ui.strong(tr!("chrome.archetypes", v = self.settings.format.name()));
                ui.horizontal(|ui| self.archetype_buttons(ui));
                ui.checkbox(&mut self.sort_by_fit, tr!("chrome.cheapest_first"))
                    .on_hover_text(tr!("chrome.sorted_required_wildcards_rares"));
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
                            note(ui, tr!("chrome.no_archetypes_loaded_yet"));
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
                                                    "chrome.cheapest_list_loaded_far",
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

    /// "Rate all" and "Refresh" above the archetype list.
    fn archetype_buttons(&mut self, ui: &mut egui::Ui) {
        let todo: Vec<Archetype> = self
            .tour
            .archetypes
            .iter()
            .filter(|a| !self.tour.decks.contains_key(&a.id))
            .cloned()
            .collect();
        let hint = tr!("chrome.loads_newest_tournament_deck");
        if ui
            .add_enabled(!todo.is_empty(), egui::Button::new(tr!("chrome.rate")))
            .on_hover_text(hint)
            .clicked()
        {
            self.status = tr!("chrome.rating_archetypes", todo_n = todo.len());
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
            .button(tr!("chrome.refresh"))
            .on_hover_text(tr!("chrome.reload_metagame_mtgtop8"))
            .clicked()
        {
            self.load_format(true);
        }
    }

    /// Choose an archetype; load its decks or fetch more lists.
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
            self.status = tr!("chrome.loading_decks", name = arch.name);
            let _ = self.tx_job.send(Job::Decks {
                format: self.settings.format,
                arch,
                urgent: true,
            });
        }
    }

    /// Checklist for getting started, as long as something is missing.
    pub(super) fn onboarding(&mut self, ui: &mut egui::Ui) {
        if self.settings.onboarding_hidden {
            return;
        }
        let logs_known = self.games.scanned.is_some() && !self.games.loading;
        let steps = [
            (
                self.db.is_some(),
                tr!("chrome.mtg_arena_card_data"),
                tr!("chrome.set_installation_folder_settings"),
            ),
            (
                self.collection.is_some(),
                tr!("chrome.collection_read_2"),
                tr!("chrome.start_arena_go_main"),
            ),
            (
                logs_known && !self.games.logs_disabled,
                tr!("chrome.detailed_logs_active_arena"),
                tr!("chrome.arena_gear_account_detailed"),
            ),
            (
                !self.tour.archetypes.is_empty(),
                tr!("chrome.tournament_decks_loaded"),
                tr!("chrome.check_internet_connection_refresh"),
            ),
        ];
        if steps.iter().all(|(done, _, _)| *done) {
            return;
        }
        let mut sync = false;
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.strong(tr!("chrome.getting_started"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button(tr!("chrome.hide")).clicked() {
                        self.settings.onboarding_hidden = true;
                    }
                });
            });
            for (i, (done, title, how)) in steps.iter().enumerate() {
                ui.horizontal_wrapped(|ui| {
                    if *done {
                        toned(ui, Tone::Good, tr!("chrome.text", title = title));
                    } else {
                        ui.label(RichText::new(tr!("chrome.text_2", title = title)).strong());
                        note(ui, how.as_str());
                        if i == 1 && !self.syncing {
                            sync = ui.small_button(tr!("chrome.read_now")).clicked();
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

/// Top bar: card count and wildcards of the collection.
fn collection_summary(ui: &mut egui::Ui, collection: Option<&Collection>) {
    match collection {
        Some(c) => {
            ui.label(tr!("chrome.cards", v = c.total_cards()))
                .on_hover_text(tr!("chrome.collection_read", v = c.age_text()));
            note(ui, tr!("chrome.wildcards_2"));
            for (i, n) in c.wildcards.as_array().iter().enumerate() {
                ui.label(
                    RichText::new(format!("{} {n}", ["C", "U", "R", "M"][i]))
                        .color(theme::rarity(ui.ctx(), Rarity::WILDCARDS[i]))
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
            toned(ui, Tone::Warn, tr!("chrome.collection_read_yet"));
        }
    }
}
