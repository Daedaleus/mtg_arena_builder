use super::*;
use crate::games::Rank;
use crate::testutil::*;

fn render(games: &[GameRecord]) {
    let db = std_db();
    let ctx = egui::Context::default();
    // Two passes: egui fixes sizes in the first one.
    for _ in 0..2 {
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| progress_view(ui, games, &db));
        });
    }
}

#[test]
fn progress_page_renders() {
    // Without games, with a game without rank, with progress and decks.
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
    let deck: Vec<GrpId> = std::iter::repeat_n(FOREST, 24)
        .chain(std::iter::repeat_n(BEAR, 36))
        .collect();
    let report = crate::sim::trial(&deck, &db, &[], 50, 1, crate::sim::Rules::default());
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
            // Draw collapsed and expanded.
            egui::CollapsingHeader::new("x")
                .default_open(true)
                .show(ui, |_| {});
            super::widgets::wildcard_plan_view(ui, &plan, &db)
        });
    });
    assert_eq!(
        super::widgets::wildcards_text(&[0, 0, 2, 1]),
        "2 Rare, 1 Mythic"
    );
    assert_eq!(super::widgets::wildcards_text(&[0; 4]), "keine");
}

/// Guard against broken line continuations in texts: if the `\` at the end
/// of a line gets lost while editing, long runs of spaces end up in the
/// displayed text.
#[test]
fn texts_have_no_broken_line_continuations() {
    fn visit(dir: &std::path::Path, bad: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                visit(&path, bad);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let text = std::fs::read_to_string(&path).unwrap();
                for (n, line) in text.lines().enumerate() {
                    let trimmed = line.trim_start();
                    if trimmed.starts_with("//") || !trimmed.contains('"') {
                        continue;
                    }
                    if trimmed.trim_end().contains(&" ".repeat(8)) {
                        bad.push(format!("{}:{}", path.display(), n + 1));
                    }
                }
            }
        }
    }
    let mut bad = Vec::new();
    visit(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut bad,
    );
    assert!(bad.is_empty(), "Leerzeichen-Folgen in Texten: {bad:?}");
}

#[test]
fn personal_sim_renders() {
    let db = std_db();
    let deck: Vec<GrpId> = std::iter::repeat_n(FOREST, 24)
        .chain(std::iter::repeat_n(BEAR, 36))
        .collect();
    let base = crate::sim::trial(&deck, &db, &[], 30, 1, crate::sim::Rules::default());
    let habits = crate::sim::Habits {
        weaker_turn: 0.2,
        skip_attack: 0.1,
        turns: 50,
    };
    let full = crate::sim::Trial {
        mine: Some(base.fast.clone()),
        mine_habits: Some(base.fast.clone()),
        habits: Some(habits),
        profile_games: 12,
        ..base.clone()
    };
    let no_habits = crate::sim::Trial {
        mine: Some(base.fast.clone()),
        profile_games: 12,
        ..base.clone()
    };
    let ctx = egui::Context::default();
    for t in [&base, &full, &no_habits] {
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| super::widgets::sim_view(ui, t));
        });
    }
}

#[test]
fn draws_companion_window() {
    let view = live::CompanionView {
        status: "Zug 3".into(),
        life: Some((20, 4)),
        board: "Länder: 3".into(),
        hints: vec![
            (live::Level::Good, "gut".into()),
            (live::Level::Warn, "Vorsicht".into()),
            (live::Level::Info, "Info".into()),
        ],
        opponent: "Mono-Rot".into(),
        opp_cards: vec!["Shock".into()],
    };
    let ctx = egui::Context::default();
    for v in [view, live::CompanionView::default()] {
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| super::companion::draw(ui, &v));
        });
    }
}

#[test]
fn tabs_roundtrip_through_settings() {
    for tab in Tab::SAVED {
        assert!(Tab::from_saved(tab.saved()) == tab);
    }
    assert!(Tab::from_saved(99) == Tab::Meta, "unbekannt: erster Reiter");
}

#[test]
fn replaced_lists_drop_old_trials() {
    let mut s = super::parts::SimState::default();
    assert_eq!(s.list_gen(DeckId::Top8(7)), 0);
    s.decks.insert(
        DeckId::Top8(7),
        sim::Trial {
            fast: Default::default(),
            mine: None,
            mine_habits: None,
            habits: None,
            profile_games: 0,
            opponent: None,
        },
    );
    s.list_replaced(DeckId::Top8(7));
    assert!(s.decks.is_empty(), "old sample hands gone");
    assert_eq!(
        s.list_gen(DeckId::Top8(7)),
        1,
        "running computation with state 0 is discarded"
    );
    assert_eq!(s.list_gen(DeckId::Top8(8)), 0, "other decks untouched");
}

#[test]
fn deck_ids_roundtrip_through_settings() {
    for id in [
        DeckId::Pasted,
        DeckId::Built(0),
        DeckId::Built(98),
        DeckId::Top8(100),
        DeckId::Top8(812_345),
        DeckId::Gg(crate::magicgg::deck_id("event", 3)),
    ] {
        assert_eq!(DeckId::from_raw(id.to_raw()), id, "{id:?}");
    }
    assert!(DeckId::Top8(5).is_tournament() && DeckId::Gg(0x8000_0001).is_tournament());
    assert!(!DeckId::Pasted.is_tournament() && DeckId::Built(1).is_built());
}

#[test]
fn tabs_fit_the_game_mode() {
    use crate::games::GameFormat;
    let shown = |mode: GameFormat| -> Vec<u8> {
        Tab::SAVED
            .into_iter()
            .filter(|t| t.fits(mode))
            .map(Tab::saved)
            .collect()
    };
    // Saved order: Meta, Paste, Built, Games, DeckCheck, Events.
    assert_eq!(shown(GameFormat::Standard), vec![0, 1, 2, 3, 4, 5]);
    assert_eq!(shown(GameFormat::Brawl), vec![1, 2, 3, 4]);
    assert_eq!(shown(GameFormat::Timeless), vec![1, 3, 4]);
    assert_eq!(shown(GameFormat::Limited), vec![3]);
}

#[test]
fn areas_group_the_pages() {
    use super::nav::Area;
    use crate::games::GameFormat;
    for tab in Tab::SAVED {
        let first = tab.area().first_tab(GameFormat::Standard);
        assert!(first.is_some_and(|t| t.area() == tab.area()));
    }
    assert!(Area::Decks.first_tab(GameFormat::Standard) == Some(Tab::Meta));
    // Brawl: the Decks area offers the collection and the import.
    assert!(Area::Decks.first_tab(GameFormat::Brawl) == Some(Tab::Built));
    // Limited: neither decks nor deck checks.
    assert!(Area::Decks.first_tab(GameFormat::Limited).is_none());
    assert!(Area::Improve.first_tab(GameFormat::Limited).is_none());
    assert!(Area::Games.first_tab(GameFormat::Limited) == Some(Tab::Games));
}
