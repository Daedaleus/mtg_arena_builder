//! Window "Settings": language, display, export, companion, Arena folder.

use super::*;

impl App {
    /// Language: like the operating system or chosen explicitly. Applies right
    /// away; computed analyses are recreated.
    fn language_choice(&mut self, ui: &mut egui::Ui) {
        use crate::i18n::{Language, Locale};
        let label = |l: Language| match l {
            Language::System => tr!(
                "chrome.same_operating_system",
                lang = Locale::system().native_name()
            ),
            Language::Fixed(locale) => locale.native_name().to_string(),
        };
        let before = self.settings.language;
        ui.horizontal(|ui| {
            ui.label(tr!("chrome.language"));
            egui::ComboBox::from_id_salt("language")
                .selected_text(label(self.settings.language))
                .show_ui(ui, |ui| {
                    let options =
                        std::iter::once(Language::System).chain(Locale::ALL.map(Language::Fixed));
                    for l in options {
                        ui.selectable_value(&mut self.settings.language, l, label(l));
                    }
                });
        });
        if self.settings.language != before {
            crate::i18n::set(self.settings.language.resolve());
            self.on_language_changed();
            self.save_settings();
        }
    }

    pub(super) fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.show_settings {
            return;
        }
        let mut open = true;
        let mut reload_db = false;
        egui::Window::new(tr!("chrome.settings_2"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(440.0)
            .show(ctx, |ui| {
                ui.strong(tr!("chrome.display"));
                self.language_choice(ui);
                let mut zoom = self.settings.zoom;
                if ui
                    .add(
                        egui::Slider::new(&mut zoom, 0.8..=1.6)
                            .step_by(0.05)
                            .text(tr!("chrome.text_size")),
                    )
                    .changed()
                {
                    self.settings.zoom = zoom;
                    ctx.set_zoom_factor(zoom);
                }
                ui.separator();
                ui.strong(tr!("chrome.export_decks"));
                ui.checkbox(
                    &mut self.settings.include_side,
                    tr!("chrome.include_sideboard"),
                )
                .on_hover_text(tr!("chrome.turn_off_best_one"));
                ui.checkbox(&mut self.settings.with_sets, tr!("chrome.export_set_codes"))
                    .on_hover_text(tr!("chrome.recommended_arena_running_english"));
                ui.separator();
                ui.strong(tr!("chrome.during_game"));
                if ui
                    .checkbox(&mut self.settings.companion, tr!("chrome.show_companion"))
                    .changed()
                {
                    self.save_settings();
                }
                if self.games.logs_disabled {
                    toned(ui, Tone::Warn, tr!("chrome.detailed_logs_off_arena"));
                }
                ui.separator();
                ui.strong(tr!("chrome.mtg_arena"));
                ui.label(tr!("chrome.installation_folder_empty_detect"));
                ui.text_edit_singleline(&mut self.settings.mtga_dir);
                ui.horizontal(|ui| {
                    reload_db = ui.button(tr!("chrome.reload_card_data")).clicked();
                    note(ui, self.db_status());
                });
                ui.separator();
                if ui
                    .button(tr!("chrome.show_getting_started_again"))
                    .clicked()
                {
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
}
