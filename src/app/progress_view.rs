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
            ui.heading(tr!("progress_view.rang"));
            let latest = games
                .iter()
                .filter(|g| g.rank_after.is_some())
                .max_by_key(|g| g.started_ms)
                .and_then(|g| g.rank_after.as_ref());
            match latest {
                Some(r) => {
                    ui.label(
                        RichText::new(tr!("progress_view.constructed_saison_siege_niederlagen", v = progress::rank_label(r), season = r.season, won = r.won, lost = r.lost))
                        .strong(),
                    );
                    ui.label(tr!("progress_view.limited", v = progress::queue_label(r, progress::Queue::Limited)));
                }
                None => {
                    ui.label(tr!("progress_view.noch_kein_rang_bekannt"));
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
                    .x_axis_label(tr!("progress_view.partie"))
                    .legend(Legend::default())
                    .show(ui, |plot| {
                        plot.line(Line::new(PlotPoints::from(ranks)).name(tr!("progress_view.constructed")).color(GREEN));
                        plot.line(
                            Line::new(PlotPoints::from(progress::queue_series(
                                games,
                                progress::Queue::Limited,
                            )))
                            .name(tr!("progress_view.limited_2"))
                            .color(YELLOW),
                        );
                    });
            }

            // --- Siegquote ---
            ui.add_space(8.0);
            ui.heading(tr!("progress_view.siegquote"));
            if let Some((recent, before)) = progress::trend(games, TREND_WINDOW) {
                let color = if recent > before + 0.5 {
                    GREEN
                } else if recent + 0.5 < before {
                    RED
                } else {
                    YELLOW
                };
                ui.label(
                    RichText::new(tr!("progress_view.letzte_partien_gewonnen_davor", TREND_WINDOW = TREND_WINDOW, recent = recent : {:.0}, before = before : {:.0}))
                    .color(color)
                    .strong(),
                );
            }
            ui.label(
                RichText::new(tr!("progress_view.gleitend_ueber_letzten_partien", TREND_WINDOW = TREND_WINDOW))
                .weak(),
            );
            let overall = progress::winrate_series(games, TREND_WINDOW, |_| true);
            if overall.len() < 2 {
                ui.label(tr!("progress_view.einen_verlauf_braucht_mindestens"));
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
                .x_axis_label(tr!("progress_view.partie"))
                .show(ui, |plot| {
                    plot.line(
                        Line::new(PlotPoints::from(overall))
                            .name(tr!("progress_view.alle_partien"))
                            .width(2.5_f32),
                    );
                    for (label, series) in decks {
                        plot.line(Line::new(PlotPoints::from(series)).name(label));
                    }
                });
        });
}
