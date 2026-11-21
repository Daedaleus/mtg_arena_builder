use super::*;
use crate::games::TurnStat;
use crate::testutil::*;

/// My turn with hand and lands in play.
fn my_turn(n: u32, lands: u32, hand: &[GrpId], cast: &[GrpId]) -> TurnStat {
    TurnStat {
        turn: n,
        mine: true,
        lands_me: lands,
        hand: hand.to_vec(),
        cast: cast.to_vec(),
        land_played: true,
        ..Default::default()
    }
}

fn deck() -> Vec<GrpId> {
    let mut d = vec![MOUNTAIN; 10];
    d.extend([FOREST; 7]);
    d.extend([OGRE; 4]);
    d.extend([GIANT; 2]);
    d.extend([BEAR; 4]);
    d.extend([SHOCK; 3]);
    d
}

fn game(won: bool, event: &str, turns: Vec<TurnStat>) -> GameRecord {
    GameRecord {
        id: format!("g{}", turns.len()),
        won: Some(won),
        event: event.into(),
        complete: true,
        deck: deck(),
        turns,
        ..Default::default()
    }
}

#[test]
fn groups_games_by_deck() {
    let db = std_db();
    let mut other = game(false, "Play", vec![]);
    other.deck = vec![ISLAND, SPIRIT];
    let mut reprint = game(true, "Ladder", vec![]);
    // Another printing of the same card counts as the same deck.
    let mut alt = deck();
    alt.reverse();
    reprint.deck = alt;
    let mut no_deck = game(true, "Ladder", vec![]);
    no_deck.deck.clear();
    let games = vec![
        game(false, "Ladder", vec![]),
        other,
        reprint,
        no_deck,
        game(true, "WelcomeDeckDuels_X", vec![]),
    ];
    let groups = group_decks(&games, &db);
    assert_eq!(groups.len(), 2);
    let main = &groups[0];
    assert_eq!(main.games, vec![0, 2, 4]);
    assert_eq!(main.wins, 2);
    assert!(!main.fixed, "not only fixed events");
    assert!(
        main.label.starts_with("RG Rangliste – Stone Giant"),
        "{}",
        main.label
    );
    assert!(main.label.ends_with("(30 Karten, 3 Partien)"));
    assert!(groups[1].label.starts_with("U Spielen"));

    let fixed = group_decks(&[game(true, "ColorChallenge_R", vec![])], &db);
    assert!(fixed[0].fixed);
    assert!(fixed[0].label.contains("Farb-Challenge"));
    assert_eq!(event_label("DualColorPrecons"), "Starterdeck");
    assert_eq!(event_label("Quick_Draft"), "Sonstiges");
    assert!(is_fixed_event("DualColorPrecons"));
}

#[test]
fn card_usage_and_swaps() {
    let db = std_db();
    // The giant is drawn, sits there affordable and is never cast;
    // the ogre is cast.
    let turns = || {
        vec![
            my_turn(1, 6, &[GIANT, OGRE], &[OGRE]),
            my_turn(3, 6, &[GIANT], &[]),
            my_turn(5, 6, &[GIANT], &[]),
            // Closing opponent turn: the last turn is not judged.
            TurnStat {
                turn: 6,
                ..Default::default()
            },
        ]
    };
    let mut g1 = game(false, "Ladder", turns());
    g1.damage_dealt.insert(OGRE, 7);
    let g2 = game(true, "Ladder", turns());
    // A game without data on cast cards doesn't count.
    let g3 = game(true, "Ladder", vec![my_turn(1, 1, &[GIANT], &[])]);
    let games = vec![g1, g2, g3];
    let group = &group_decks(&games, &db)[0];
    let owned: HashMap<String, u32> = [
        ("hill ogre".to_string(), 4),
        ("shivan dragon".to_string(), 2),
    ]
    .into();
    let r = check(group, &games, &db, &owned, None, &[]);
    assert_eq!((r.games, r.complete, r.tracked, r.wins), (3, 3, 2, 2));
    assert!(!r.show_winrates());
    let giant = r.cards.iter().find(|c| c.name == "Stone Giant").unwrap();
    assert_eq!(
        (giant.copies, giant.seen, giant.cast, giant.held),
        (2, 2, 0, 2)
    );
    assert_eq!(giant.won_when_seen, 1);
    let ogre = r.cards.iter().find(|c| c.name == "Hill Ogre").unwrap();
    assert_eq!((ogre.cast, ogre.damage), (2, 7));
    // Lands are at the end of the list.
    assert!(r.cards.last().unwrap().land);
    assert_eq!(r.swaps.len(), 1);
    let swap = &r.swaps[0];
    assert_eq!(swap.out, "Stone Giant");
    assert!(swap.reason.contains("nie gespielt") && swap.reason.contains("lange ungespielt"));
    assert_eq!(
        swap.replacement.as_ref().map(|r| r.0.as_str()),
        Some("Shivan Dragon")
    );
    assert!(r.meta.is_none());
}

#[test]
fn mana_screw_and_flood() {
    let db = std_db();
    let screw = || {
        vec![
            my_turn(1, 1, &[OGRE], &[OGRE]),
            my_turn(3, 2, &[OGRE], &[]),
            my_turn(5, 2, &[GIANT], &[]),
        ]
    };
    let flood = || {
        let mut t: Vec<TurnStat> = (0..5)
            .map(|i| my_turn(i * 2 + 1, 3 + i, &[OGRE], &[OGRE]))
            .collect();
        t[4].hand = vec![MOUNTAIN, FOREST, OGRE];
        t
    };
    let games = vec![
        game(false, "Ladder", screw()),
        game(false, "Ladder", screw()),
        game(false, "Ladder", screw()),
        game(true, "Ladder", vec![my_turn(1, 1, &[], &[OGRE])]),
    ];
    let group = &group_decks(&games, &db)[0];
    let r = check(group, &games, &db, &HashMap::new(), None, &[]);
    assert_eq!((r.mana.games, r.mana.screw, r.mana.flood), (3, 3, 0));
    assert_eq!((r.mana.lands, r.mana.size), (17, 30));
    assert_eq!(r.mana.verdict, LandVerdict::TooFew);

    let games = vec![
        game(false, "Ladder", flood()),
        game(false, "Ladder", flood()),
        game(false, "Ladder", flood()),
    ];
    let group = &group_decks(&games, &db)[0];
    let r = check(group, &games, &db, &HashMap::new(), None, &[]);
    assert_eq!(r.mana.flood, 3);
    assert_eq!(r.mana.verdict, LandVerdict::TooMany);

    let few = vec![game(false, "Ladder", flood())];
    let group = &group_decks(&few, &db)[0];
    let r = check(group, &few, &db, &HashMap::new(), None, &[]);
    assert_eq!(r.mana.verdict, LandVerdict::TooFewGames);
    assert!(!r.mana.verdict.needs_change());
    assert!(r.mana.verdict.advice().contains("mindestens 3"));

    let mixed = vec![
        game(false, "Ladder", flood()),
        game(false, "Ladder", screw()),
        game(false, "Ladder", vec![my_turn(1, 3, &[], &[OGRE]); 3]),
    ];
    let group = &group_decks(&mixed, &db)[0];
    let r = check(group, &mixed, &db, &HashMap::new(), None, &[]);
    assert_eq!(r.mana.verdict, LandVerdict::Fine);
}

#[test]
fn compares_with_tournament_archetype() {
    let db = std_db();
    let games = vec![game(true, "Ladder", vec![my_turn(1, 1, &[], &[BEAR])])];
    let group = &group_decks(&games, &db)[0];
    let list_a = crate::deck::parse("4 Hill Ogre\n4 Grizzly Bears\n4 Giant Growth\n20 Forest\n");
    let list_b =
        crate::deck::parse("4 Hill Ogre\n4 Grizzly Bears\n4 Giant Growth\n4 Llanowar Elves\n");
    let other = crate::deck::parse("4 Cancel\n4 Divination\n");
    let meta = vec![
        MetaArchetype {
            id: 7,
            name: "Gruul".into(),
            lists: vec![&list_a, &list_b],
        },
        MetaArchetype {
            id: 8,
            name: "Control".into(),
            lists: vec![&other],
        },
        MetaArchetype {
            id: 9,
            name: "Leer".into(),
            lists: vec![],
        },
    ];
    let owned: HashMap<String, u32> = [("giant growth".to_string(), 2)].into();
    let r = check(group, &games, &db, &owned, None, &meta);
    let m = r.meta.expect("Gruul passt");
    assert_eq!((m.id, m.archetype.as_str(), m.lists), (7, "Gruul", 2));
    assert!(m.overlap >= 0.2);
    // Missing tournament cards: from 50 % of the lists, not basic lands.
    assert_eq!(
        m.missing,
        vec![
            ("Giant Growth".to_string(), 1.0, 2),
            ("Llanowar Elves".to_string(), 0.5, 0)
        ]
    );
    let shock = r.cards.iter().find(|c| c.name == "Shock").unwrap();
    assert_eq!(shock.meta_share, Some(0.0));
    assert!(
        r.swaps
            .iter()
            .any(|s| s.out == "Shock" && s.reason.contains("kaum jemand"))
    );
}

#[test]
fn deck_as_text() {
    let db = std_db();
    assert_eq!(
        deck_text(&[OGRE, OGRE, MOUNTAIN, GrpId(9999)], &db),
        "Deck\n2 Hill Ogre\n1 Mountain\n1 #9999\n"
    );
}
