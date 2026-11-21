//! Companion as its own, always visible window.

use super::*;

impl App {
    /// Opens or maintains the companion (own window, always on top).
    pub(super) fn companion(&mut self, ctx: &egui::Context) {
        if !self.settings.companion {
            // Window off: stop following.
            if let Some(shared) = self.live.take() {
                shared
                    .stop
                    .store(true, std::sync::atomic::Ordering::Relaxed);
            }
            return;
        }
        let id = egui::ViewportId::from_hash_of("companion");
        if self.live.is_none()
            && let Some(db) = &self.db
        {
            let shared = std::sync::Arc::new(live::Shared::default());
            let ctx = ctx.clone();
            live::spawn(db.clone(), shared.clone(), move || {
                ctx.request_repaint_of(id)
            });
            self.live = Some(shared);
        }
        let Some(shared) = self.live.clone() else {
            return;
        };
        // Pass on tournament lists for opponent recognition when new ones were loaded.
        if self.lists.len() != self.live_meta_lists
            && let Some(db) = &self.db
        {
            self.live_meta_lists = self.lists.len();
            let shares = opponents::arch_shares(&self.meta_archetypes(), db);
            *shared.meta.write().unwrap_or_else(|e| e.into_inner()) = shares;
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
            .with_title(tr!("companion.mtg_companion"))
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
                let view = shared
                    .view
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clone();
                draw(ui, &view);
            });
        });
    }
}

/// Draws the companion window.
pub(super) fn draw(ui: &mut egui::Ui, view: &live::CompanionView) {
    let green = theme::tone(ui.ctx(), Tone::Good);
    let yellow = theme::tone(ui.ctx(), Tone::Warn);
    ui.heading(&view.status);
    if let Some((me, opp)) = view.life {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(tr!("live_draw.you", me = me))
                    .strong()
                    .size(18.0),
            );
            ui.label(egui::RichText::new(tr!("live_draw.opponent", opp = opp)).size(18.0));
        });
    }
    if !view.board.is_empty() {
        ui.label(egui::RichText::new(&view.board).weak());
    }
    ui.separator();
    for (level, text) in &view.hints {
        let rich = egui::RichText::new(text);
        ui.label(match level {
            live::Level::Good => rich.color(green).strong(),
            live::Level::Warn => rich.color(yellow).strong(),
            live::Level::Info => rich,
        });
    }
    if !view.opponent.is_empty() {
        ui.separator();
        ui.label(
            egui::RichText::new(tr!("live_draw.opponent_2", opponent = view.opponent)).strong(),
        );
        if !view.opp_cards.is_empty() {
            ui.label(egui::RichText::new(view.opp_cards.join(", ")).weak());
        }
    }
}
