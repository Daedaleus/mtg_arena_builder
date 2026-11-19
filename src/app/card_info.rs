//! Karten in der Oberfläche: Name in der eingestellten Sprache und Kartenbild
//! beim Überfahren mit der Maus.

use super::*;

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
        if resp.hovered()
            && !self.images.textures.contains_key(&key)
            && !self.images.missing.contains(&key)
            && self.images.requested.borrow_mut().insert(key.clone())
        {
            let _ = self.images.tx.send(key.clone());
        }
        resp.on_hover_ui(|ui| {
            if let Some(tex) = self.images.textures.get(&key) {
                ui.add(
                    egui::Image::new((tex.id(), tex.size_vec2()))
                        .fit_to_exact_size(egui::vec2(244.0, 340.0)),
                );
            } else if self.images.missing.contains(&key) {
                ui.label(RichText::new(tr!("deck_view.kein_bild_verfuegbar")).weak());
            } else {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(tr!("deck_view.bild_geladen"));
                });
            }
            if let Some(text) = &extra {
                ui.label(text);
            }
        })
    }
}
