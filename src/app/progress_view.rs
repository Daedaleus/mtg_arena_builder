//! „Meine Partien“ > „Verlauf“: Rang und Siegquote über die Zeit.

use super::*;

/// Gleitendes Fenster der Siegquote im Verlauf.
const TREND_WINDOW: usize = 10;

/// „Meine Partien“ > „Verlauf“: Rang und Siegquote über die Zeit.
pub(super) fn progress_view(ui: &mut egui::Ui, games: &[GameRecord], db: &CardDb) {
    use egui_plot::{Legend, Line, Plot, PlotPoints};
    egui::ScrollArea::vertical()
        .id_salt("progress_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            // --- Rang ---
            ui.heading("Rang");
            let latest = games
                .iter()
                .filter(|g| g.rank_after.is_some())
                .max_by_key(|g| g.started_ms)
                .and_then(|g| g.rank_after.as_ref());
            match latest {
                Some(r) => {
                    ui.label(
                        RichText::new(format!(
                            "Constructed: {} · Saison {}: {} Siege, {} Niederlagen in der Rangliste",
                            progress::rank_label(r),
                            r.season,
                            r.won,
                            r.lost
                        ))
                        .strong(),
                    );
                    ui.label(format!(
                        "Limited: {}",
                        progress::queue_label(r, progress::Queue::Limited)
                    ));
                }
                None => {
                    ui.label("Noch kein Rang bekannt – Arena meldet ihn nach jeder Partie im Log.");
                }
            }
            let ranks = progress::rank_series(games);
            if ranks.len() >= 2 {
                Plot::new("rank_plot")
                    .height(200.0)
                    .include_y(0.0)
                    .include_y(4.0)
                    .allow_scroll(false)
                    .y_axis_formatter(|mark, _| progress::points_label(mark.value))
                    .x_axis_label("Partie")
                    .legend(Legend::default())
                    .show(ui, |plot| {
                        plot.line(Line::new(PlotPoints::from(ranks)).name("Constructed").color(GREEN));
                        plot.line(
                            Line::new(PlotPoints::from(progress::queue_series(
                                games,
                                progress::Queue::Limited,
                            )))
                            .name("Limited")
                            .color(YELLOW),
                        );
                    });
            }

            // --- Siegquote ---
            ui.add_space(8.0);
            ui.heading("Siegquote");
            if let Some((recent, before)) = progress::trend(games, TREND_WINDOW) {
                let color = if recent > before + 0.5 {
                    GREEN
                } else if recent + 0.5 < before {
                    RED
                } else {
                    YELLOW
                };
                ui.label(
                    RichText::new(format!(
                        "Letzte {TREND_WINDOW} Partien: {recent:.0} % gewonnen – davor {before:.0} %"
                    ))
                    .color(color)
                    .strong(),
                );
            }
            ui.label(
                RichText::new(format!(
                    "Gleitend über die letzten {TREND_WINDOW} Partien; Decks mit mindestens 3 Partien einzeln."
                ))
                .weak(),
            );
            let overall = progress::winrate_series(games, TREND_WINDOW, |_| true);
            if overall.len() < 2 {
                ui.label("Für einen Verlauf braucht es mindestens 2 beendete Partien.");
                return;
            }
            let groups = deckcheck::group_decks(games, db);
            let decks = progress::deck_series(games, &groups, TREND_WINDOW, 3);
            Plot::new("winrate_plot")
                .height(260.0)
                .include_y(0.0)
                .include_y(100.0)
                .allow_scroll(false)
                .legend(Legend::default())
                .y_axis_formatter(|mark, _| format!("{:.0} %", mark.value))
                .x_axis_label("Partie")
                .show(ui, |plot| {
                    plot.line(
                        Line::new(PlotPoints::from(overall))
                            .name("Alle Partien")
                            .width(2.5_f32),
                    );
                    for (label, series) in decks {
                        plot.line(Line::new(PlotPoints::from(series)).name(label));
                    }
                });
        });
}
