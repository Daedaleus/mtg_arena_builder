use super::*;
use crate::testutil::*;

fn deck(parts: &[(GrpId, usize)]) -> Vec<GrpId> {
    parts
        .iter()
        .flat_map(|(g, n)| std::iter::repeat_n(*g, *n))
        .collect()
}

fn run(parts: &[(GrpId, usize)]) -> SimReport {
    simulate(&deck(parts), &std_db(), 400, 42)
}

#[test]
fn healthy_deck_runs_smoothly() {
    let r = run(&[(FOREST, 24), (BEAR, 20), (ELF, 16)]);
    assert_eq!((r.size, r.lands, r.games), (60, 24, 400));
    assert!(r.on_curve[0] > 0.9, "{:?}", r.on_curve);
    assert!(r.on_curve[2] > 0.7, "{:?}", r.on_curve);
    assert_eq!(r.color_screw, 0.0);
    assert!(r.flood < 0.25);
    assert!(r.mana_used[1] > 1.0, "turn 2: mostly a bear");
    assert!(r.mana_used.iter().zip(&r.mana_avail).all(|(u, a)| u <= a));
    assert_eq!(r.advice.len(), 1);
    assert!(r.advice[0].contains("rund"), "{:?}", r.advice);
}

#[test]
fn too_few_lands() {
    let r = run(&[(FOREST, 12), (BEAR, 48)]);
    assert!(r.on_curve[2] < 0.6, "{:?}", r.on_curve);
    assert!(r.mulligan > 0.2);
    assert!(
        r.advice[0].starts_with("Zu wenige Länder"),
        "{:?}",
        r.advice
    );
}

#[test]
fn too_many_lands() {
    let r = run(&[(FOREST, 40), (BEAR, 20)]);
    assert!(r.flood >= 0.25, "{}", r.flood);
    assert!(
        r.advice.iter().any(|a| a.starts_with("Zu viele Länder")),
        "{:?}",
        r.advice
    );
}

#[test]
fn missing_colors_are_found() {
    let r = run(&[(FOREST, 24), (OGRE, 36)]);
    assert!(r.color_screw > 0.9, "{}", r.color_screw);
    assert_eq!(r.missing_color, Some(3), "Rot");
    assert!(
        r.advice.iter().any(|a| a.contains("meist Rot")),
        "{:?}",
        r.advice
    );
}

#[test]
fn expensive_curve() {
    let r = run(&[(FOREST, 24), (GIANT, 36)]);
    assert!(
        r.advice.iter().any(|a| a.starts_with("Kurve zu hoch")),
        "{:?}",
        r.advice
    );
}

#[test]
fn mana_creatures_and_tapped_lands_are_used() {
    // Elves bring mana for the giant earlier.
    let r = run(&[(FOREST, 22), (TAPLAND_UR, 2), (ELF, 20), (GIANT, 16)]);
    assert!(
        r.mana_avail[3] > 4.0,
        "turn 4 with elf more than 4 mana: {:?}",
        r.mana_avail
    );
}

#[test]
fn reproducible_and_edge_cases() {
    let d = deck(&[(FOREST, 24), (BEAR, 36)]);
    let db = std_db();
    assert_eq!(simulate(&d, &db, 50, 7), simulate(&d, &db, 50, 7));
    assert_ne!(simulate(&d, &db, 50, 7), simulate(&d, &db, 50, 8));
    let tiny = simulate(&[FOREST; 5], &db, 100, 1);
    assert_eq!(tiny.mulligan, 0.0);
    assert!(tiny.advice.is_empty());
    assert!(simulate(&d, &db, 0, 1).advice.is_empty());
}

#[test]
fn deck_from_names() {
    let db = std_db();
    let d = deck_from_entries([("Forest", 2), ("Grizzly Bears", 1), ("Unbekannt", 4)], &db);
    assert_eq!(d, vec![FOREST, FOREST, BEAR]);
}

// ------------------------------------------------------ opponent pressure ----

#[test]
fn slow_decks_get_run_over() {
    // Only expensive giants: the opponent attacks undisturbed.
    let slow = run(&[(FOREST, 24), (GIANT, 36)]);
    assert!(slow.pressure.low >= 0.4, "{:?}", slow.pressure);
    assert!(slow.pressure.life[5] < slow.pressure.life[1]);
    assert!(
        slow.advice
            .iter()
            .any(|a| a.starts_with("Gegen schnelle Decks")),
        "{:?}",
        slow.advice
    );
    // Cheap creatures hold against it and defeat attackers.
    let fast = run(&[(FOREST, 24), (BEAR, 20), (ELF, 16)]);
    assert!(
        fast.pressure.life[5] > slow.pressure.life[5],
        "{:?} vs {:?}",
        fast.pressure,
        slow.pressure
    );
    assert!(fast.pressure.kills > 0.5);
    // Removal helps as well.
    let removal = run(&[(SWAMP, 24), (MURDER, 20), (GIANT, 16)]);
    assert!(removal.pressure.kills > 1.0, "{:?}", removal.pressure);
    assert!(removal.pressure.life[5] > slow.pressure.life[5]);
    for r in [&slow, &fast, &removal] {
        assert!(r.pressure.dead <= r.pressure.low);
        assert!(
            r.pressure
                .life
                .iter()
                .all(|l| *l >= 0.0 && *l <= START_LIFE as f32)
        );
    }
}

#[test]
fn board_rules() {
    let db = std_db();
    let card = |g: GrpId| &db.cards[&g];
    let mut b = pressure::Board::new();
    // Opponent's turn 1: a creature arrives, but doesn't attack yet.
    assert!(b.opp_turn(1).is_empty());
    assert_eq!(b.life, START_LIFE);
    assert_eq!(b.opp_creatures().len(), 1);
    // Turn 2: the 2/1 attacks, no blocker – 2 damage.
    b.opp_turn(2);
    assert_eq!(b.life, START_LIFE - 2);
    // Own bear (new, not ready) blocks the 2/1 and kills it.
    b.start_my_turn();
    b.cast(card(BEAR));
    assert_eq!(b.mine.len(), 1);
    assert!(!b.mine[0].ready, "summoning sickness");
    assert!(b.my_attack().is_empty(), "not ready yet");
    b.opp_turn(3);
    assert_eq!(b.kills, 1, "2/1 dies to the bear");
    // The burn spell Shock (2) only kills creatures with toughness up to 2.
    let before = b.opp.len();
    b.cast(card(SHOCK));
    assert_eq!(b.opp.len(), before - 1);
    // Murder hits the strongest.
    b.opp_turn(4);
    let strongest = b.opp.iter().map(|u| u.c.power).max().unwrap();
    b.cast(card(MURDER));
    assert!(b.opp.iter().all(|u| u.c.power <= strongest));
    // A board wipe clears everything.
    b.cast(card(WRATH));
    assert!(b.opp.is_empty() && b.mine.is_empty());
    // Without a target nothing happens.
    let kills = b.kills;
    b.cast(card(MURDER));
    assert_eq!(b.kills, kills);
}

#[test]
fn racing_rules() {
    let db = std_db();
    let card = |g: GrpId| &db.cards[&g];
    // Safe attack: no blocker, counterattack survivable.
    let mut b = pressure::Board::new();
    b.cast(card(OGRE));
    b.start_my_turn();
    assert!(b.my_attack().is_empty());
    assert_eq!(b.opp_life, START_LIFE - 3);
    assert!(b.mine[0].tapped, "attacker is tapped");
    // Tapped attackers don't block in the opponent's turn.
    b.opp_turn(1);
    b.opp_turn(2);
    assert_eq!(b.life, START_LIFE - 2, "2/1 kam ungeblockt durch");

    // A blocker that kills the attacker and survives keeps it at home …
    let mut b = pressure::Board::new();
    b.cast(card(ELF));
    b.opp_turn(1);
    b.opp_turn(2); // 2/1 attacks and gets tapped, 2/2 comes in untapped
    b.start_my_turn();
    let life = b.opp_life;
    b.my_attack();
    assert_eq!(b.opp_life, life, "2/2 kills the elf and survives");
    // … a possible trade doesn't.
    let mut b = pressure::Board::new();
    b.cast(card(BEAR));
    b.opp_turn(1);
    b.opp_turn(2);
    b.start_my_turn();
    b.my_attack();
    assert!(
        b.mine.is_empty() || b.mine[0].tapped,
        "bear attacks into the 2/2"
    );

    // Lethal: everything attacks, even if the counterattack would be lethal.
    let mut b = pressure::Board::new();
    b.cast(card(OGRE));
    b.cast(card(BEAR));
    b.opp_turn(1); // 2/1 stays untapped and can block
    b.life = 1;
    b.opp_life = 2;
    b.start_my_turn();
    b.my_attack();
    assert!(b.opp_life <= 0, "einer kommt durch: {}", b.opp_life);
    assert!(b.over());
    // After the end of the game nothing happens any more.
    let life = b.life;
    assert!(b.opp_turn(2).is_empty());
    assert_eq!(b.life, life);
    assert!(b.my_attack().is_empty());

    // Burn: lethal to the face, without a target also to the face.
    let mut b = pressure::Board::new();
    b.cast(card(SPIKE));
    assert_eq!(b.opp_life, START_LIFE - 3, "no target");
    b.opp_turn(1); // 2/1 – Shock would kill it …
    b.opp_life = 2;
    b.cast(card(SHOCK));
    assert!(b.over(), "… but lethal burn goes to the face");

    // Little life: stay home and block.
    let mut b = pressure::Board::new();
    b.cast(card(OGRE));
    b.opp_turn(1);
    b.opp_turn(2);
    // The ogre blocked the 2/1; the 2/2 is left – counterattack 2.
    assert_eq!(b.opp_creatures().len(), 1);
    b.life = 2;
    b.start_my_turn();
    let opp_life = b.opp_life;
    b.my_attack();
    assert_eq!(b.opp_life, opp_life, "counterattack (2) would be lethal");
}

#[test]
fn fast_decks_win_races() {
    // Flyers get past the opponent and press their life – unlike expensive giants.
    let fast = run(&[(ISLAND, 24), (SPIRIT, 36)]);
    let slow = run(&[(FOREST, 24), (GIANT, 36)]);
    assert!(
        fast.pressure.opp_life[5] < slow.pressure.opp_life[5],
        "{:?} / {:?}",
        fast.pressure,
        slow.pressure
    );
    assert!(fast.pressure.ahead >= slow.pressure.ahead);
    assert!(fast.pressure.opp_life[5] < 16.0, "{:?}", fast.pressure);
    assert_eq!(slow.pressure.won, 0.0);
    assert!(fast.pressure.won <= fast.pressure.ahead);
    // Whoever is mostly ahead gets no "too slow" hint.
    if fast.pressure.ahead >= fast.pressure.low {
        assert!(
            !fast
                .advice
                .iter()
                .any(|a| a.starts_with("Gegen schnelle Decks"))
        );
    }
}

#[test]
fn creatures_from_cards() {
    let db = std_db();
    let spirit = pressure::creature_of(&db.cards[&SPIRIT], 7);
    assert_eq!((spirit.grp_id, spirit.power, spirit.toughness), (7, 2, 3));
    assert!(spirit.flying && spirit.reach);
    let odd = crate::testutil::card(1, "X").creature(0, 0).build();
    let mut odd = odd;
    odd.power = "*".into();
    odd.toughness = "*".into();
    let c = pressure::creature_of(&odd, 1);
    assert_eq!((c.power, c.toughness), (0, 1), "star stats as 0/1");
}

// ------------------------------------------------- personal sample hands ----

use crate::games::{GameRecord, TurnStat};

/// Game with opponent power at the end of each opponent turn and own turns.
fn history_game(opp_power: &[i32], my_turn: TurnStat) -> GameRecord {
    let mut turns = Vec::new();
    for (k, p) in opp_power.iter().enumerate() {
        turns.push(TurnStat {
            turn: (2 * k + 1) as u32,
            ..my_turn.clone()
        });
        turns.push(TurnStat {
            turn: (2 * k + 2) as u32,
            mine: false,
            opp_creatures: if *p > 0 { vec![(OGRE, *p)] } else { vec![] },
            ..Default::default()
        });
    }
    GameRecord {
        complete: true,
        won: Some(false),
        opp_cards: [(SHOCK, 1)].into(),
        turns,
        ..Default::default()
    }
}

fn my_turn(hand: &[GrpId], lands: &[GrpId], attacked: bool, safe_idle: &[GrpId]) -> TurnStat {
    TurnStat {
        mine: true,
        hand: hand.to_vec(),
        lands: lands.to_vec(),
        attacked,
        safe_idle: safe_idle.to_vec(),
        ..Default::default()
    }
}

#[test]
fn opponent_profile_from_history() {
    let db = std_db();
    let ok = my_turn(&[], &[FOREST], true, &[]);
    let few: Vec<GameRecord> = (0..2).map(|_| history_game(&[1, 2], ok.clone())).collect();
    assert!(opponent_from(&few, &db).is_none(), "too few games");
    let mut games: Vec<GameRecord> = (0..6)
        .map(|_| history_game(&[0, 2, 2, 5, 5, 6], ok.clone()))
        .collect();
    // One outlier with a huge battlefield on turn 2 hardly distorts the average.
    games[0] = history_game(&[0, 30, 2, 5, 5, 6], ok.clone());
    let o = opponent_from(&games, &db).unwrap();
    assert_eq!(o.games, 6);
    // Turn 2: the outlier only counts as 6 instead of 30 – (6 + 5 × 2) / 6.
    assert!((o.power[1] - 16.0 / 6.0).abs() < 0.01, "{:?}", o.power);
    let mut rest = o.power;
    rest[1] = 0.0;
    assert_eq!(
        rest,
        [0.0, 0.0, 0.0, 3.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0],
        "gain per turn, never negative; without data keep the last pace"
    );
    assert!(o.toughness.iter().zip(&o.power).all(|(t, p)| t >= p));
    assert_eq!(o.label, "deine Gegner (meist Mono-Rot)");
    // One Shock per game over 6 opponent turns; one Giant Growth in total.
    assert!((o.removal - 1.0 / 6.0).abs() < 0.01, "{}", o.removal);
    assert_eq!(o.tricks, 0.0);
    games[1].opp_cards.insert(GROWTH, 1);
    let o = opponent_from(&games, &db).unwrap();
    assert!((o.tricks - 1.0 / 36.0).abs() < 0.01, "{}", o.tricks);
    assert_eq!(o.flying, 0.0);
    // Share of flying creatures among the opposing creature cards seen.
    games[2].opp_cards.insert(ANGEL, 1);
    games[2].opp_cards.insert(BEAR, 3);
    let o = opponent_from(&games, &db).unwrap();
    assert!((o.flying - 0.25).abs() < 0.01, "{}", o.flying);
    // Fixed opponent for comparison
    let fast = Opponent::fast();
    assert_eq!(fast.games, 0);
    assert_eq!(fast.power[0], 2.0);
}

#[test]
fn habits_from_history() {
    let db = std_db();
    // Ogre in hand, 4 lands, nothing cast: better play possible.
    let lazy = my_turn(
        &[OGRE, ELF],
        &[MOUNTAIN, MOUNTAIN, FOREST, FOREST],
        false,
        &[BEAR],
    );
    let good = my_turn(&[], &[FOREST], true, &[]);
    let mut games = Vec::new();
    for _ in 0..4 {
        games.push(history_game(&[1, 1, 1], lazy.clone()));
        games.push(history_game(&[1, 1, 1], good.clone()));
    }
    let h = habits_from(&games, &db).unwrap();
    assert_eq!(h.turns, 24);
    assert!((h.weaker_turn - 0.5).abs() < 0.01, "{h:?}");
    assert!((h.skip_attack - 0.5).abs() < 0.01, "{h:?}");
    assert!(habits_from(&games[..2], &db).is_none(), "too few turns");
}

#[test]
fn profile_ignores_brawl_and_limited() {
    let db = std_db();
    let ok = my_turn(&[], &[FOREST], true, &[]);
    let mut games: Vec<GameRecord> = (0..6)
        .map(|_| history_game(&[2, 2, 2, 2], ok.clone()))
        .collect();
    let constructed = opponent_from(&games, &db).unwrap();
    assert_eq!(constructed.games, 6);
    let habits = habits_from(&games, &db).unwrap();
    // Brawl and draft games with a much faster opponent change nothing.
    for (i, event) in ["Play_Brawl", "QuickDraft_FDN_20261101"].iter().enumerate() {
        let mut g = history_game(&[6, 6, 6, 6], ok.clone());
        g.event = (*event).into();
        games.push(g);
        assert_eq!(opponent_from(&games, &db), Some(constructed.clone()), "{i}");
        assert_eq!(habits_from(&games, &db), Some(habits), "{i}");
    }
    // Only Brawl games: no profile at all.
    for g in &mut games {
        g.event = "Play_Brawl".into();
    }
    assert!(opponent_from(&games, &db).is_none());
    assert!(habits_from(&games, &db).is_none());
}

#[test]
fn personal_simulation() {
    let db = std_db();
    let d = deck(&[(FOREST, 24), (BEAR, 20), (ELF, 16)]);
    let slow = Opponent {
        power: [0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0],
        toughness: [0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0],
        label: "langsam".into(),
        games: 10,
        turns: TURNS,
        removal: 0.0,
        tricks: 0.0,
        flying: 0.0,
    };
    let fast = simulate(&d, &db, 300, 3);
    let best = simulate_with(&d, &db, 300, 3, &slow, None);
    assert_eq!(best.opponent, "langsam");
    assert!(!best.with_habits);
    assert!(
        best.pressure.ahead > fast.pressure.ahead,
        "{:?} / {:?}",
        best.pressure,
        fast.pressure
    );
    // Habits: often playing weaker and skipping attacks costs.
    let bad = Habits {
        weaker_turn: 0.8,
        skip_attack: 0.9,
        turns: 100,
    };
    let yours = simulate_with(&d, &db, 300, 3, &slow, Some(&bad));
    assert!(yours.with_habits);
    assert!(
        yours.pressure.opp_life[5] > best.pressure.opp_life[5],
        "{:?} / {:?}",
        yours.pressure,
        best.pressure
    );
    assert!(yours.mana_used.iter().sum::<f32>() < best.mana_used.iter().sum::<f32>());
    // The opponent's removal and tricks cost games.
    let hard = Opponent {
        removal: 0.5,
        tricks: 0.5,
        ..slow.clone()
    };
    let tough = simulate_with(&d, &db, 300, 3, &hard, None);
    assert!(
        tough.pressure.ahead < best.pressure.ahead,
        "{:?} / {:?}",
        tough.pressure,
        best.pressure
    );
}

#[test]
fn opponent_removal_and_tricks() {
    let db = std_db();
    let card = |g: GrpId| &db.cards[&g];
    // Removal hits the strongest own creature.
    let mut b = pressure::Board::new();
    b.cast(card(ELF));
    b.cast(card(OGRE));
    let removal = pressure::OppTurn {
        removal: true,
        ..Default::default()
    };
    let quiet = pressure::OppTurn::default();
    let with_trick = pressure::OppTurn {
        trick: true,
        ..Default::default()
    };
    assert_eq!(b.opp_turn_with(None, removal), vec![OGRE]);
    assert_eq!(b.mine.len(), 1);
    assert_eq!(b.mine[0].card, ELF);

    // Without a trick the attacking 2/2 dies to the ogre, with a trick the ogre.
    for (how, ogre_lives) in [(quiet, true), (with_trick, false)] {
        let mut b = pressure::Board::new();
        b.cast(card(OGRE));
        b.opp_turn_with(Some((2, 2)), quiet);
        let died = b.opp_turn_with(None, how);
        assert_eq!(died.is_empty(), ogre_lives, "{how:?}");
        assert_eq!(b.opp.len(), usize::from(how.trick), "{how:?}");
        assert_eq!(b.life, START_LIFE);
    }

    // Unblocked, the trick brings extra damage.
    let mut b = pressure::Board::new();
    b.opp_turn_with(Some((2, 1)), quiet);
    b.opp_turn_with(None, with_trick);
    assert_eq!(b.life, START_LIFE - 2 - pressure::TRICK_BONUS);

    // Trick for the blocker: the 2/2 stops the lethal ogre and survives.
    let mut b = pressure::Board::new();
    b.cast(card(OGRE));
    b.opp_turn_with(Some((2, 2)), quiet);
    b.start_my_turn();
    b.opp_life = 3;
    assert_eq!(b.my_attack_with(false, true, false), vec![OGRE]);
    assert_eq!(b.opp_life, 3);
    assert_eq!(b.opp.len(), 1);
}

#[test]
fn own_combat_tricks() {
    let db = std_db();
    let card = |g: GrpId| &db.cards[&g];
    let quiet = pressure::OppTurn::default();
    // Both sides pump: the ogre (3/3 + 2) beats the pumped 2/2 (4/4).
    let mut b = pressure::Board::new();
    b.cast(card(OGRE));
    b.opp_turn_with(Some((2, 2)), quiet);
    b.start_my_turn();
    b.opp_life = 3;
    assert!(b.my_attack_with(false, true, true).is_empty());
    assert!(b.trick_used);
    assert!(b.opp.is_empty());

    // Own trick for a blocker: the ogre survives the pumped 2/2 attacker.
    let mut b = pressure::Board::new();
    b.cast(card(OGRE));
    b.opp_turn_with(Some((2, 2)), quiet);
    let how = pressure::OppTurn {
        trick: true,
        own_trick: true,
        ..Default::default()
    };
    assert!(b.opp_turn_with(None, how).is_empty());
    assert!(b.trick_used);
    assert!(b.opp.is_empty());
    assert_eq!(b.mine.len(), 1);

    // Without a blocker the own trick stays in the hand.
    let mut b = pressure::Board::new();
    b.opp_turn_with(Some((2, 2)), quiet);
    let how = pressure::OppTurn {
        own_trick: true,
        ..Default::default()
    };
    b.opp_turn_with(None, how);
    assert!(!b.trick_used);
    assert_eq!(b.life, START_LIFE - 2);
}

#[test]
fn prowess_grows_with_spells() {
    let db = std_db();
    let monk = crate::testutil::card(GrpId(900), "Monastery Swiftspear")
        .cost("{R}")
        .creature(1, 2)
        .abilities(&[crate::carddb::keyword::PROWESS])
        .build();
    let mut b = pressure::Board::new();
    b.cast(&monk);
    b.start_my_turn();
    b.cast(&db.cards[&DIVINATION]);
    b.my_attack();
    assert_eq!(b.opp_life, START_LIFE - 2, "1 power + 1 for the spell");
    // The bonus lasts only for the turn.
    b.start_my_turn();
    b.my_attack();
    assert_eq!(b.opp_life, START_LIFE - 3);
    // A used combat trick counts as a spell too.
    b.start_my_turn();
    b.my_attack_with(false, false, true);
    assert_eq!(b.opp_life, START_LIFE - 3 - 2 - pressure::TRICK_BONUS);
}

#[test]
fn flying_opponents_pass_ground_blockers() {
    let db = std_db();
    let flyer = pressure::OppTurn {
        flying: true,
        ..Default::default()
    };
    let quiet = pressure::OppTurn::default();
    // The bear can't block the flying 2/2.
    let mut b = pressure::Board::new();
    b.cast(&db.cards[&BEAR]);
    b.opp_turn_with(Some((2, 2)), flyer);
    b.opp_turn_with(None, quiet);
    assert_eq!(b.life, START_LIFE - 2);
    // The angel flies too and stops it.
    let mut b = pressure::Board::new();
    b.cast(&db.cards[&ANGEL]);
    b.opp_turn_with(Some((2, 2)), flyer);
    b.opp_turn_with(None, quiet);
    assert_eq!(b.life, START_LIFE);
}

#[test]
fn simulation_uses_tricks_from_hand() {
    let db = std_db();
    // The same deck once more, but Giant Growth isn't recognised as a trick.
    let mut blank = std_db();
    blank.cards.get_mut(&GROWTH).unwrap().roles = 0;
    let d = deck(&[(FOREST, 24), (BEAR, 26), (GROWTH, 10)]);
    let with = simulate(&d, &db, 300, 5);
    let without = simulate(&d, &blank, 300, 5);
    assert!(
        with.pressure.life[5] > without.pressure.life[5],
        "{:?} / {:?}",
        with.pressure,
        without.pressure
    );
}

#[test]
fn random_rounding() {
    let mut rng = Rng(7);
    let total: i32 = (0..10_000).map(|_| rng.round(0.3)).sum();
    assert!((2_700..3_300).contains(&total), "{total}");
    assert_eq!(rng.round(2.0), 2);
    assert_eq!(rng.round(-1.0), 0);
    assert!(!rng.chance(0.0));
    assert!(rng.chance(1.0));
}

#[test]
fn trial_runs_all_variants() {
    let db = std_db();
    let d = deck(&[(FOREST, 24), (BEAR, 20), (ELF, 16)]);
    // Without history: only the fixed opponent.
    let t = trial(&d, &db, &[], 50, 1);
    assert!(t.mine.is_none() && t.mine_habits.is_none() && t.habits.is_none());
    assert_eq!(t.profile_games, 0);
    assert_eq!(t.fast.opponent, "schneller Gegner");
    assert_eq!(t.fast.turns, FAST_TURNS, "stress test only for the start");
    assert_eq!(t.fast.mana_avail[FAST_TURNS], 0.0, "not played afterwards");
    // With history: all three variants.
    let lazy = my_turn(
        &[OGRE],
        &[MOUNTAIN, MOUNTAIN, FOREST, FOREST],
        false,
        &[BEAR],
    );
    let history: Vec<GameRecord> = (0..8)
        .map(|_| history_game(&[1, 1, 2], lazy.clone()))
        .collect();
    let t = trial(&d, &db, &history, 50, 1);
    assert_eq!(t.profile_games, 8);
    assert!(t.opponent.as_ref().unwrap().removal > 0.0, "Shock gesehen");
    assert!(
        t.mine
            .as_ref()
            .unwrap()
            .opponent
            .starts_with("deine Gegner")
    );
    assert!(t.mine_habits.as_ref().unwrap().with_habits);
    assert_eq!(
        t.mine.as_ref().unwrap().turns,
        TURNS,
        "own opponents up to turn 10"
    );
    assert!(t.habits.is_some());
}
