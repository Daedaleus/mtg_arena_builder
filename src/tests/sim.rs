use super::*;
use crate::testutil::*;

fn deck(parts: &[(u32, usize)]) -> Vec<u32> {
    parts
        .iter()
        .flat_map(|(g, n)| std::iter::repeat_n(*g, *n))
        .collect()
}

fn run(parts: &[(u32, usize)]) -> SimReport {
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
    let card = |g: u32| &db.cards[&g];
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
    let card = |g: u32| &db.cards[&g];
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
