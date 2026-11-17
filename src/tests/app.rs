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

#[test]
fn sim_page_renders() {
    let db = std_db();
    let deck: Vec<u32> = std::iter::repeat_n(FOREST, 24)
        .chain(std::iter::repeat_n(BEAR, 36))
        .collect();
    let report = crate::sim::simulate(&deck, &db, 50, 1);
    let ctx = egui::Context::default();
    let _ = ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| super::widgets::sim_view(ui, &report));
    });
}

#[test]
fn wildcard_plan_renders() {
    let db = std_db();
    let deck = crate::deck::parse("4 Serra Angel\n4 Hill Ogre\n1 Unbekannt\n");
    let eval = crate::deck::evaluate(&deck, &db, &HashMap::new());
    let plan = budget::wildcard_plan(&eval, &db, [4, 0, 0, 0], true);
    let ctx = egui::Context::default();
    let _ = ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            // Zugeklappt und aufgeklappt zeichnen.
            egui::CollapsingHeader::new("x")
                .default_open(true)
                .show(ui, |_| {});
            super::widgets::wildcard_plan_view(ui, &plan)
        });
    });
    assert_eq!(
        super::widgets::wildcards_text(&[0, 0, 2, 1]),
        "2 Rare, 1 Mythic"
    );
    assert_eq!(super::widgets::wildcards_text(&[0; 4]), "keine");
}
