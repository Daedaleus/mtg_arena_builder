//! Begleitfenster als eigenes, immer sichtbares Fenster.

use super::*;

impl App {
    /// Öffnet bzw. pflegt das Begleitfenster (eigenes Fenster, immer im Vordergrund).
    pub(super) fn companion(&mut self, ctx: &egui::Context) {
        if !self.settings.companion {
            return;
        }
        let id = egui::ViewportId::from_hash_of("companion");
        if self.live.is_none()
            && let Some(db) = &self.db
        {
            let shared = std::sync::Arc::new(live::Shared::default());
            live::spawn(db.clone(), shared.clone(), ctx.clone(), id);
            self.live = Some(shared);
        }
        let Some(shared) = self.live.clone() else {
            return;
        };
        // Turnierlisten für die Gegner-Erkennung weitergeben, wenn neue geladen wurden.
        if self.lists.len() != self.live_meta_lists
            && let Some(db) = &self.db
        {
            self.live_meta_lists = self.lists.len();
            let shares = opponents::arch_shares(&self.meta_archetypes(), db);
            *shared.meta.write().unwrap() = shares;
        }
        if shared
            .close_requested
            .swap(false, std::sync::atomic::Ordering::Relaxed)
        {
            self.settings.companion = false;
            self.save_settings();
            return;
        }
        let mut builder = egui::ViewportBuilder::default()
            .with_title(tr!("companion.mtg_begleiter"))
            .with_inner_size([360.0, 330.0])
            .with_min_inner_size([260.0, 180.0])
            .with_always_on_top();
        if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../../assets/icon.png"))
        {
            builder = builder.with_icon(icon);
        }
        ctx.show_viewport_deferred(id, builder, move |ctx, _class| {
            if ctx.input(|i| i.viewport().close_requested()) {
                shared
                    .close_requested
                    .store(true, std::sync::atomic::Ordering::Relaxed);
            }
            egui::CentralPanel::default().show(ctx, |ui| {
                let view = shared.view.lock().unwrap().clone();
                live::draw(ui, &view);
            });
        });
    }
}
