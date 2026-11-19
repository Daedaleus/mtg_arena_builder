//! Fenster „Einstellungen“: Sprache, Darstellung, Export, Begleitfenster, Arena-Ordner.

use super::*;

impl App {
    /// Sprache: wie das Betriebssystem oder fest gewählt. Gilt sofort;
    /// berechnete Auswertungen werden neu erstellt.
    fn language_choice(&mut self, ui: &mut egui::Ui) {
        use crate::i18n::{Language, Locale};
        let label = |l: Language| match l {
            Language::System => tr!(
                "chrome.sprache_system",
                lang = Locale::system().native_name()
            ),
            Language::Fixed(locale) => locale.native_name().to_string(),
        };
        let before = self.settings.language;
        ui.horizontal(|ui| {
            ui.label(tr!("chrome.sprache"));
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
        egui::Window::new(tr!("chrome.einstellungen_2"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(440.0)
            .show(ctx, |ui| {
                ui.strong(tr!("chrome.darstellung"));
                self.language_choice(ui);
                let mut zoom = self.settings.zoom;
                if ui
                    .add(
                        egui::Slider::new(&mut zoom, 0.8..=1.6)
                            .step_by(0.05)
                            .text(tr!("chrome.textgroesse")),
                    )
                    .changed()
                {
                    self.settings.zoom = zoom;
                    ctx.set_zoom_factor(zoom);
                }
                ui.separator();
                ui.strong(tr!("chrome.decks_exportieren"));
                ui.checkbox(
                    &mut self.settings.include_side,
                    tr!("chrome.sideboard_einbeziehen"),
                )
                .on_hover_text(tr!("chrome.best_of_one_ausschalten"));
                ui.checkbox(
                    &mut self.settings.with_sets,
                    tr!("chrome.set_kuerzeln_exportieren"),
                )
                .on_hover_text(tr!("chrome.empfohlen_wenn_arena_nicht"));
                ui.separator();
                ui.strong(tr!("chrome.waehrend_partie"));
                if ui
                    .checkbox(
                        &mut self.settings.companion,
                        tr!("chrome.begleitfenster_anzeigen"),
                    )
                    .changed()
                {
                    self.save_settings();
                }
                if self.games.logs_disabled {
                    ui.label(
                        RichText::new(tr!("chrome.detaillierte_logs_arena_aus")).color(YELLOW),
                    );
                }
                ui.separator();
                ui.strong(tr!("chrome.mtg_arena"));
                ui.label(tr!("chrome.installationsordner_leer_automatisch_erkennen"));
                ui.text_edit_singleline(&mut self.settings.mtga_dir);
                ui.horizontal(|ui| {
                    reload_db = ui.button(tr!("chrome.kartendaten_neu_laden")).clicked();
                    ui.label(RichText::new(self.db_status()).weak());
                });
                ui.separator();
                if ui
                    .button(tr!("chrome.erste_schritte_wieder_anzeigen"))
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
