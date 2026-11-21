use super::*;

fn rank(class: &str, level: u32, step: u32) -> Rank {
    Rank {
        season: 95,
        class: class.into(),
        level,
        step,
        ..Default::default()
    }
}

fn game(ms: u64, won: Option<bool>, rank: Option<Rank>) -> GameRecord {
    GameRecord {
        started_ms: ms,
        won,
        rank_after: rank,
        ..Default::default()
    }
}

#[test]
fn rank_names_and_points() {
    assert_eq!(rank_label(&rank("Bronze", 4, 2)), "Bronze 4, Schritt 2");
    assert_eq!(rank_label(&rank("Platinum", 1, 0)), "Platin 1, Schritt 0");
    assert_eq!(rank_label(&rank("Mythic", 1, 0)), "Mythisch");
    assert_eq!(class_name("Silver"), "Silber");
    assert_eq!(class_name("Diamond"), "Diamant");
    assert_eq!(class_name("Gold"), "Gold");

    assert_eq!(rank_points(&rank("Bronze", 4, 0)), 0.0);
    assert_eq!(rank_points(&rank("Bronze", 1, 0)), 3.0);
    assert_eq!(rank_points(&rank("Silver", 4, 3)), 4.5);
    assert!(
        rank_points(&rank("Bronze", 4, 99)) < 1.0,
        "step stays within the tier"
    );
    assert_eq!(rank_points(&rank("Unbekannt", 9, 0)), 0.0);
    // Ascending over all classes
    let order: Vec<f64> = CLASSES
        .iter()
        .map(|c| rank_points(&rank(c, 4, 0)))
        .collect();
    assert!(order.windows(2).all(|w| w[0] < w[1]));

    assert_eq!(points_label(0.0), "Bronze 4");
    assert_eq!(points_label(3.5), "Bronze 1");
    assert_eq!(points_label(4.0), "Silber 4");
    assert_eq!(points_label(-1.0), "Bronze 4");
    assert_eq!(points_label(20.0), "Mythisch");
}

#[test]
fn series_are_chronological() {
    // Stored newest first – the progress runs the other way.
    let games = vec![
        game(300, Some(true), Some(rank("Bronze", 3, 1))),
        game(200, None, None),
        game(100, Some(false), Some(rank("Bronze", 4, 2))),
    ];
    assert_eq!(
        rank_series(&games),
        vec![[1.0, 4.0 / 12.0], [3.0, 1.0 + 1.0 / 6.0]]
    );
    // Draws/aborted games don't count, the numbering continues.
    assert_eq!(
        winrate_series(&games, 10, |_| true),
        vec![[1.0, 0.0], [3.0, 50.0]]
    );
}

#[test]
fn rolling_window_and_decks() {
    let results = [true, false, false, true, true, true];
    let games: Vec<GameRecord> = results
        .iter()
        .enumerate()
        .map(|(i, w)| game(i as u64, Some(*w), None))
        .collect();
    let s = winrate_series(&games, 2, |_| true);
    let rates: Vec<f64> = s.iter().map(|p| p[1]).collect();
    assert_eq!(rates, vec![100.0, 50.0, 0.0, 50.0, 100.0, 100.0]);
    // Window 0 is treated like 1.
    assert_eq!(winrate_series(&games, 0, |_| true)[1][1], 0.0);

    let groups = vec![
        DeckGroup {
            label: "A".into(),
            deck: vec![],
            games: vec![0, 2, 4],
            wins: 2,
            fixed: false,
        },
        DeckGroup {
            label: "B".into(),
            deck: vec![],
            games: vec![1],
            wins: 0,
            fixed: false,
        },
    ];
    let decks = deck_series(&games, &groups, 10, 3);
    assert_eq!(decks.len(), 1, "deck B has too few games");
    assert_eq!(decks[0].0, "A");
    let xs: Vec<f64> = decks[0].1.iter().map(|p| p[0]).collect();
    assert_eq!(xs, vec![1.0, 3.0, 5.0], "x = number over all games");
}

#[test]
fn trend_compares_recent_games() {
    let mk = |results: &[bool]| -> Vec<GameRecord> {
        results
            .iter()
            .enumerate()
            .map(|(i, w)| game(i as u64, Some(*w), None))
            .collect()
    };
    let games = mk(&[false, false, true, true]);
    assert_eq!(trend(&games, 2), Some((100.0, 0.0)));
    assert_eq!(trend(&games, 3), None, "too few games");
    assert_eq!(trend(&games, 0), None);
}

#[test]
fn limited_queue() {
    let r = Rank {
        class: "Silver".into(),
        level: 2,
        limited_class: "Gold".into(),
        limited_level: 3,
        limited_step: 3,
        ..Default::default()
    };
    assert_eq!(queue_label(&r, Queue::Limited), "Gold 3, Schritt 3");
    assert_eq!(queue_label(&r, Queue::Constructed), "Silber 2, Schritt 0");
    assert_eq!(queue_points(&r, Queue::Limited), 9.5);
    assert_eq!(queue_points(&r, Queue::Constructed), 6.0);
    let games = vec![game(1, Some(true), Some(r))];
    assert_eq!(queue_series(&games, Queue::Limited), vec![[1.0, 9.5]]);
}
