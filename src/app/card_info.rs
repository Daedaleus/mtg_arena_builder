//! Karten in der Oberfläche: Name in der eingestellten Sprache und Kartenbild
//! beim Überfahren mit der Maus.

use super::*;

/// So viele Kartenbilder zeigt ein Hinweis höchstens nebeneinander.
const MAX_HOVER_CARDS: usize = 3;

impl App {
    /// Kartenname in der eingestellten Sprache.
    pub(super) fn shown(&self, name: &str) -> String {
        self.db
            .as_ref()
            .map_or_else(|| name.to_string(), |db| db.display_name(name))
    }

    /// Zeigt beim Überfahren eines Kartennamens das Kartenbild (lädt es bei Bedarf).
    pub(super) fn card_hover(
        &self,
        resp: egui::Response,
        name: &str,
        extra: Option<String>,
    ) -> egui::Response {
        let key = images::key(name, crate::i18n::current());
        if resp.hovered() {
            self.request_image(&key);
        }
        resp.on_hover_ui(|ui| {
            self.card_image(ui, &key);
            if let Some(text) = &extra {
                ui.label(text);
            }
        })
    }

    /// Zeigt beim Überfahren die Bilder mehrerer Karten nebeneinander
    /// (höchstens [`MAX_HOVER_CARDS`]); ohne Karten bleibt `resp` unverändert.
    pub(super) fn cards_hover(&self, resp: egui::Response, cards: &[GrpId]) -> egui::Response {
        let Some(db) = self.db.as_ref() else {
            return resp;
        };
        let mut keys: Vec<String> = cards
            .iter()
            .filter_map(|g| db.cards.get(g))
            .map(|c| images::key(&c.name, crate::i18n::current()))
            .collect();
        keys.dedup();
        keys.truncate(MAX_HOVER_CARDS);
        if keys.is_empty() {
            return resp;
        }
        if resp.hovered() {
            keys.iter().for_each(|k| self.request_image(k));
        }
        resp.on_hover_ui(|ui| {
            ui.horizontal(|ui| keys.iter().for_each(|k| self.card_image(ui, k)));
        })
    }

    /// Fordert ein Kartenbild an, falls es weder da ist noch fehlt; nach einem
    /// gescheiterten Abruf erst nach einer Pause erneut.
    fn request_image(&self, key: &str) {
        if self.images.textures.contains_key(key) || self.images.missing.contains(key) {
            return;
        }
        let retry = {
            let mut failed = self.images.failed.borrow_mut();
            let due = failed
                .get(key)
                .is_some_and(|t| t.elapsed() >= super::parts::IMAGE_RETRY);
            if due {
                failed.remove(key);
            }
            due
        };
        if self.images.requested.borrow_mut().insert(key.to_string()) || retry {
            let _ = self.images.tx.send(key.to_string());
        }
    }

    /// Kartenbild oder ein Hinweis, warum es (noch) fehlt.
    fn card_image(&self, ui: &mut egui::Ui, key: &str) {
        if let Some(tex) = self.images.textures.get(key) {
            ui.add(
                egui::Image::new((tex.id(), tex.size_vec2()))
                    .fit_to_exact_size(super::theme::CARD_IMAGE),
            );
        } else if self.images.failed.borrow().contains_key(key) {
            note(ui, tr!("card_info.image_reachable_right_now"));
        } else if self.images.missing.contains(key) {
            note(ui, tr!("deck_view.no_image_available"));
        } else {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(tr!("deck_view.loading_image"));
            });
        }
    }
}
