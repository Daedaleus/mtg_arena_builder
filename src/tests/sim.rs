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
    assert!(r.mana_used[1] > 1.0, "Zug 2: meist ein Bär");
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
    // Elfen bringen Mana für den Riesen früher.
    let r = run(&[(FOREST, 22), (TAPLAND_UR, 2), (ELF, 20), (GIANT, 16)]);
    assert!(
        r.mana_avail[3] > 4.0,
        "Zug 4 mit Elf mehr als 4 Mana: {:?}",
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

// ---------------------------------------------------------- Gegner-Druck ----

#[test]
fn slow_decks_get_run_over() {
    // Nur teure Riesen: der Gegner greift ungestört an.
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
    // Günstige Kreaturen halten dagegen und besiegen Angreifer.
    let fast = run(&[(FOREST, 24), (BEAR, 20), (ELF, 16)]);
    assert!(
        fast.pressure.life[5] > slow.pressure.life[5],
        "{:?} vs {:?}",
        fast.pressure,
        slow.pressure
    );
    assert!(fast.pressure.kills > 0.5);
    // Entfernung hilft ebenfalls.
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
    // Zug 1 des Gegners: Kreatur kommt, greift aber noch nicht an.
    assert!(b.opp_turn(1).is_empty());
    assert_eq!(b.life, START_LIFE);
    assert_eq!(b.opp_creatures().len(), 1);
    // Zug 2: die 2/1 greift an, kein Blocker – 2 Schaden.
    b.opp_turn(2);
    assert_eq!(b.life, START_LIFE - 2);
    // Eigener Bär (neu, nicht bereit) blockt die 2/1 und tötet sie.
    b.start_my_turn();
    b.cast(card(BEAR));
    assert_eq!(b.mine.len(), 1);
    assert!(!b.mine[0].ready, "Einsatzverzögerung");
    assert!(b.my_attack().is_empty(), "noch nicht bereit");
    b.opp_turn(3);
    assert_eq!(b.kills, 1, "2/1 stirbt am Bären");
    // Brandzauber Shock (2) tötet nur Kreaturen mit Widerstand bis 2.
    let before = b.opp.len();
    b.cast(card(SHOCK));
    assert_eq!(b.opp.len(), before - 1);
    // Murder trifft die stärkste.
    b.opp_turn(4);
    let strongest = b.opp.iter().map(|u| u.c.power).max().unwrap();
    b.cast(card(MURDER));
    assert!(b.opp.iter().all(|u| u.c.power <= strongest));
    // Flächenzauber räumt alles ab.
    b.cast(card(WRATH));
    assert!(b.opp.is_empty() && b.mine.is_empty());
    // Ohne Ziel passiert nichts.
    let kills = b.kills;
    b.cast(card(MURDER));
    assert_eq!(b.kills, kills);
}

#[test]
fn racing_rules() {
    let db = std_db();
    let card = |g: GrpId| &db.cards[&g];
    // Gefahrloser Angriff: kein Blocker, Gegenschlag überlebbar.
    let mut b = pressure::Board::new();
    b.cast(card(OGRE));
    b.start_my_turn();
    assert!(b.my_attack().is_empty());
    assert_eq!(b.opp_life, START_LIFE - 3);
    assert!(b.mine[0].tapped, "Angreifer ist getappt");
    // Getappte Angreifer blocken im Gegnerzug nicht.
    b.opp_turn(1);
    b.opp_turn(2);
    assert_eq!(b.life, START_LIFE - 2, "2/1 kam ungeblockt durch");

    // Ein Blocker, der den Angreifer tötet und überlebt, hält ihn zu Hause …
    let mut b = pressure::Board::new();
    b.cast(card(ELF));
    b.opp_turn(1);
    b.opp_turn(2); // 2/1 greift an und wird getappt, 2/2 kommt ungetappt
    b.start_my_turn();
    let life = b.opp_life;
    b.my_attack();
    assert_eq!(b.opp_life, life, "2/2 tötet den Elf und überlebt");
    // … ein möglicher Tausch nicht.
    let mut b = pressure::Board::new();
    b.cast(card(BEAR));
    b.opp_turn(1);
    b.opp_turn(2);
    b.start_my_turn();
    b.my_attack();
    assert!(
        b.mine.is_empty() || b.mine[0].tapped,
        "Bär greift in den 2/2 an"
    );

    // Tödlich: alles greift an, auch wenn der Gegenschlag tödlich wäre.
    let mut b = pressure::Board::new();
    b.cast(card(OGRE));
    b.cast(card(BEAR));
    b.opp_turn(1); // 2/1 bleibt ungetappt und kann blocken
    b.life = 1;
    b.opp_life = 2;
    b.start_my_turn();
    b.my_attack();
    assert!(b.opp_life <= 0, "einer kommt durch: {}", b.opp_life);
    assert!(b.over());
    // Nach Spielende passiert nichts mehr.
    let life = b.life;
    assert!(b.opp_turn(2).is_empty());
    assert_eq!(b.life, life);
    assert!(b.my_attack().is_empty());

    // Brand: tödlich ins Gesicht, ohne Ziel ebenfalls ins Gesicht.
    let mut b = pressure::Board::new();
    b.cast(card(SPIKE));
    assert_eq!(b.opp_life, START_LIFE - 3, "kein Ziel");
    b.opp_turn(1); // 2/1 – Shock würde sie töten …
    b.opp_life = 2;
    b.cast(card(SHOCK));
    assert!(b.over(), "… aber tödlicher Brand geht ins Gesicht");

    // Wenig Leben: zu Hause bleiben und blocken.
    let mut b = pressure::Board::new();
    b.cast(card(OGRE));
    b.opp_turn(1);
    b.opp_turn(2);
    // Der Oger hat die 2/1 geblockt; übrig ist die 2/2 – Gegenschlag 2.
    assert_eq!(b.opp_creatures().len(), 1);
    b.life = 2;
    b.start_my_turn();
    let opp_life = b.opp_life;
    b.my_attack();
    assert_eq!(b.opp_life, opp_life, "Gegenschlag (2) wäre tödlich");
}

#[test]
fn fast_decks_win_races() {
    // Flieger kommen am Gegner vorbei und drücken sein Leben – anders als teure Riesen.
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
    // Wer meist vorne liegt, bekommt keinen „zu langsam“-Hinweis.
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
    assert_eq!((c.power, c.toughness), (0, 1), "Sternchen-Werte als 0/1");
}

// ------------------------------------------------- persönliche Probehände ----

use crate::games::{GameRecord, TurnStat};

/// Partie mit Gegnerkraft am Ende jedes Gegnerzugs und eigenen Zügen.
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
    assert!(opponent_from(&few, &db).is_none(), "zu wenige Partien");
    let mut games: Vec<GameRecord> = (0..6)
        .map(|_| history_game(&[0, 2, 2, 5, 5, 6], ok.clone()))
        .collect();
    // Ein Ausreißer mit riesigem Feld in Zug 2 verzerrt den Schnitt kaum.
    games[0] = history_game(&[0, 30, 2, 5, 5, 6], ok.clone());
    let o = opponent_from(&games, &db).unwrap();
    assert_eq!(o.games, 6);
    // Zug 2: Ausreißer zählt nur mit 6 statt 30 – (6 + 5 × 2) / 6.
    assert!((o.power[1] - 16.0 / 6.0).abs() < 0.01, "{:?}", o.power);
    let mut rest = o.power;
    rest[1] = 0.0;
    assert_eq!(
        rest,
        [0.0, 0.0, 0.0, 3.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0],
        "Zuwachs je Zug, nie negativ; ohne Daten weiter im letzten Tempo"
    );
    assert!(o.toughness.iter().zip(&o.power).all(|(t, p)| t >= p));
    assert_eq!(o.label, "deine Gegner (meist Mono-Rot)");
    // Je Partie ein Shock bei 6 Gegnerzügen; ein Giant Growth insgesamt.
    assert!((o.removal - 1.0 / 6.0).abs() < 0.01, "{}", o.removal);
    assert_eq!(o.tricks, 0.0);
    games[1].opp_cards.insert(GROWTH, 1);
    let o = opponent_from(&games, &db).unwrap();
    assert!((o.tricks - 1.0 / 36.0).abs() < 0.01, "{}", o.tricks);
    // Fester Gegner zum Vergleich
    let fast = Opponent::fast();
    assert_eq!(fast.games, 0);
    assert_eq!(fast.power[0], 2.0);
}

#[test]
fn habits_from_history() {
    let db = std_db();
    // Oger auf der Hand, 4 Länder, nichts gespielt: besserer Zug möglich.
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
    assert!(habits_from(&games[..2], &db).is_none(), "zu wenige Züge");
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
    // Gewohnheiten: oft schwächer spielen und Angriffe auslassen kostet.
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
    // Entfernung und Tricks des Gegners kosten Partien.
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
    // Entfernung trifft die stärkste eigene Kreatur.
    let mut b = pressure::Board::new();
    b.cast(card(ELF));
    b.cast(card(OGRE));
    assert_eq!(b.opp_turn_with(None, true, false), vec![OGRE]);
    assert_eq!(b.mine.len(), 1);
    assert_eq!(b.mine[0].card, ELF);

    // Ohne Trick stirbt die angreifende 2/2 am Oger, mit Trick der Oger.
    for (trick, ogre_lives) in [(false, true), (true, false)] {
        let mut b = pressure::Board::new();
        b.cast(card(OGRE));
        b.opp_turn_with(Some((2, 2)), false, false);
        let died = b.opp_turn_with(None, false, trick);
        assert_eq!(died.is_empty(), ogre_lives, "Trick {trick}");
        assert_eq!(b.opp.len(), usize::from(trick), "Trick {trick}");
        assert_eq!(b.life, START_LIFE);
    }

    // Ungeblockt bringt der Trick Zusatzschaden.
    let mut b = pressure::Board::new();
    b.opp_turn_with(Some((2, 1)), false, false);
    b.opp_turn_with(None, false, true);
    assert_eq!(b.life, START_LIFE - 2 - pressure::TRICK_BONUS);

    // Trick für den Blocker: die 2/2 hält den tödlichen Oger auf und überlebt.
    let mut b = pressure::Board::new();
    b.cast(card(OGRE));
    b.opp_turn_with(Some((2, 2)), false, false);
    b.start_my_turn();
    b.opp_life = 3;
    assert_eq!(b.my_attack_with(false, true), vec![OGRE]);
    assert_eq!(b.opp_life, 3);
    assert_eq!(b.opp.len(), 1);
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
    // Ohne Verlauf: nur der feste Gegner.
    let t = trial(&d, &db, &[], 50, 1);
    assert!(t.mine.is_none() && t.mine_habits.is_none() && t.habits.is_none());
    assert_eq!(t.profile_games, 0);
    assert_eq!(t.fast.opponent, "schneller Gegner");
    assert_eq!(t.fast.turns, FAST_TURNS, "Stresstest nur für den Start");
    assert_eq!(t.fast.mana_avail[FAST_TURNS], 0.0, "danach nicht gespielt");
    // Mit Verlauf: alle drei Varianten.
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
        "eigene Gegner bis Zug 10"
    );
    assert!(t.habits.is_some());
}
