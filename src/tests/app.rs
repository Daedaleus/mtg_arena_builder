use super::*;
use crate::games::Rank;
use crate::testutil::*;

fn render(games: &[GameRecord]) {
    let db = std_db();
    let ctx = egui::Context::default();
    // Zwei Durchläufe: egui legt Größen im ersten fest.
    for _ in 0..2 {
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| progress_view(ui, games, &db));
        });
    }
}

#[test]
fn progress_page_renders() {
    // Ohne Partien, mit einer Partie ohne Rang, mit Verlauf und Decks.
    render(&[]);
    render(&[GameRecord {
        won: Some(true),
        ..Default::default()
    }]);
    let games: Vec<GameRecord> = (0..24)
        .map(|i| GameRecord {
            started_ms: i,
            won: Some(i % 3 == 0),
            deck: vec![if i % 2 == 0 { BEAR } else { OGRE }; 10],
            rank_after: Some(Rank {
                season: 95,
                class: "Bronze".into(),
                level: 4 - (i as u32 / 8),
                step: i as u32 % 6,
                ..Default::default()
            }),
            ..Default::default()
        })
        .collect();
    render(&games);
}
