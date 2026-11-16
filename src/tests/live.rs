use super::*;
use crate::deck;
use crate::testutil::*;

fn c(power: i32, flying: bool, reach: bool, menace: bool) -> LiveCreature {
    LiveCreature {
        power,
        flying,
        reach,
        menace,
        ..Default::default()
    }
}

#[test]
fn combat_damage() {
    // Kein Blocker: alles kommt durch
    assert_eq!(
        unblocked_damage(&[c(2, false, false, false), c(3, false, false, false)], &[]),
        5
    );
    // Ein Blocker hält den stärksten Angreifer auf
    assert_eq!(
        unblocked_damage(
            &[c(2, false, false, false), c(3, false, false, false)],
            &[c(1, false, false, false)]
        ),
        2
    );
    // Flieger kann vom Bodenblocker nicht geblockt werden
    assert_eq!(
        unblocked_damage(
            &[c(4, true, true, false), c(1, false, false, false)],
            &[c(5, false, false, false)]
        ),
        4
    );
    // ... vom Blocker mit Reichweite schon
    assert_eq!(
        unblocked_damage(&[c(4, true, true, false)], &[c(1, false, true, false)]),
        0
    );
    // Bedrohlich braucht zwei Blocker
    assert_eq!(
        unblocked_damage(&[c(3, false, false, true)], &[c(2, false, false, false)]),
        3
    );
    assert_eq!(
        unblocked_damage(
            &[c(3, false, false, true)],
            &[c(2, false, false, false), c(1, false, true, false)]
        ),
        0
    );
}

// ------------------------------------------------------- Begleitfenster ----

fn base() -> LiveState {
    LiveState {
        turn: 3,
        my_turn: true,
        phase: "Phase_Main1".into(),
        life_me: 20,
        life_opp: 20,
        ..Default::default()
    }
}

fn hints(v: &CompanionView) -> String {
    v.hints
        .iter()
        .map(|(_, t)| t.as_str())
        .collect::<Vec<_>>()
        .join(" | ")
}

fn level_of(v: &CompanionView, needle: &str) -> Level {
    v.hints
        .iter()
        .find(|(_, t)| t.contains(needle))
        .unwrap_or_else(|| panic!("Hinweis {needle:?} fehlt in: {}", hints(v)))
        .0
}

fn creature_at(grp: u32, power: i32, toughness: i32) -> LiveCreature {
    LiveCreature {
        grp_id: grp,
        power,
        toughness,
        ..Default::default()
    }
}

#[test]
fn waiting_and_finished_states() {
    let db = std_db();
    let v = build_view(None, None, &db, &[]);
    assert_eq!(v.status, "Warte auf eine Partie …");
    assert!(v.life.is_none());
    for (won, text) in [
        (Some(true), "gewonnen"),
        (Some(false), "verloren"),
        (None, "Partie vorbei."),
    ] {
        let g = crate::games::GameRecord {
            won,
            ..Default::default()
        };
        let v = build_view(None, Some(&(g, Instant::now())), &db, &[]);
        assert!(v.status.contains(text), "{}", v.status);
    }
}

#[test]
fn mulligan_advice() {
    let db = std_db();
    let mut ls = base();
    ls.turn = 0;
    ls.mulligan = true;
    ls.hand = vec![MOUNTAIN, MOUNTAIN, FOREST, BEAR, OGRE, SHOCK, ELF];
    let v = build_view(Some(&ls), None, &db, &[]);
    assert_eq!(v.status, "Starthand");
    assert_eq!(level_of(&v, "behalten"), Level::Good);
    ls.hand = vec![MOUNTAIN, BEAR, OGRE, SHOCK, ELF, GIANT, DRAGON];
    let v = build_view(Some(&ls), None, &db, &[]);
    assert_eq!(level_of(&v, "Mulligan empfohlen"), Level::Warn);
}

#[test]
fn turn_suggestion_and_board() {
    let db = std_db();
    let mut ls = base();
    ls.hand = vec![MOUNTAIN, OGRE];
    ls.untapped_lands = vec![MOUNTAIN, MOUNTAIN];
    ls.lands_me = 2;
    ls.lands_untapped = 2;
    let v = build_view(Some(&ls), None, &db, &[]);
    assert_eq!(v.status, "Zug 3 · dein Zug · 1. Hauptphase");
    assert_eq!(v.life, Some((20, 20)));
    assert!(v.board.starts_with("Länder: 2 (2 frei)"), "{}", v.board);
    assert!(
        hints(&v).contains("Vorschlag: Mountain legen, dann Hill Ogre (3 von 3 Mana)"),
        "{}",
        hints(&v)
    );
    // In der 2. Hauptphase ist ein ungelegtes Land eine Warnung.
    ls.phase = "Phase_Main2".into();
    let v = build_view(Some(&ls), None, &db, &[]);
    assert_eq!(level_of(&v, "Vorschlag"), Level::Warn);
    // Im Kampf kein Zugvorschlag.
    ls.phase = "Phase_Combat".into();
    let v = build_view(Some(&ls), None, &db, &[]);
    assert!(!hints(&v).contains("Vorschlag"));
    assert_eq!(v.status, "Zug 3 · dein Zug · Kampf");
    ls.phase = String::new();
    let v = build_view(Some(&ls), None, &db, &[]);
    assert_eq!(v.status, "Zug 3 · dein Zug");
}

#[test]
fn suggestion_details() {
    let db = std_db();
    let mut ls = base();
    ls.land_played = true;
    // Nichts bezahlbar und kein Land: kein Vorschlag.
    ls.hand = vec![GIANT];
    ls.untapped_lands = vec![FOREST];
    let v = build_view(Some(&ls), None, &db, &[]);
    assert!(!hints(&v).contains("Vorschlag"), "{}", hints(&v));
    ls.land_played = false;
    ls.hand = vec![GIANT, FOREST];
    let v = build_view(Some(&ls), None, &db, &[]);
    assert!(hints(&v).contains("Forest legen – nichts Sinnvolles bezahlbar"));
    // Entfernung mit Ziel; Konter bleibt für den Gegnerzug.
    ls.land_played = true;
    ls.hand = vec![MURDER, COUNTER];
    ls.untapped_lands = vec![SWAMP, SWAMP, ISLAND, ISLAND, ISLAND, ISLAND];
    ls.opp_creatures = vec![(BEAR, 2), (DRAGON, 5)];
    let v = build_view(Some(&ls), None, &db, &[]);
    let h = hints(&v);
    assert!(h.contains("Murder auf Shivan Dragon"), "{h}");
    assert!(h.contains("Mana offen lassen für Cancel"), "{h}");
    // Gefahrlos angreifen: Kreaturen lieber nach dem Kampf.
    ls.hand = vec![BEAR];
    ls.untapped_lands = vec![FOREST, FOREST];
    ls.opp_creatures.clear();
    ls.safe_attackers = vec![OGRE];
    ls.attackers = vec![creature_at(OGRE, 3, 3)];
    let v = build_view(Some(&ls), None, &db, &[]);
    let h = hints(&v);
    assert!(h.contains("Gefahrlos angreifen: Hill Ogre"), "{h}");
    assert!(h.contains("Erst angreifen"), "{h}");
}

#[test]
fn lethal_and_low_life() {
    let db = std_db();
    let mut ls = base();
    ls.land_played = true;
    ls.attackers = vec![c(3, false, false, false)];
    ls.hand = vec![SPIKE];
    ls.untapped_lands = vec![MOUNTAIN];
    ls.lands_untapped = 1;
    ls.life_opp = 6;
    let v = build_view(Some(&ls), None, &db, &[]);
    let h = hints(&v);
    assert!(
        h.contains("Tödlich möglich! Angriff mit allem 3 + Brand 3 (Lava Spike)"),
        "{h}"
    );
    assert!(!h.contains("nicht aufgeben"));
    ls.life_opp = 9;
    let v = build_view(Some(&ls), None, &db, &[]);
    assert!(hints(&v).contains("Gegner fast besiegt: 9 Leben"));
    ls.life_opp = 5;
    ls.attackers.clear();
    ls.hand.clear();
    let v = build_view(Some(&ls), None, &db, &[]);
    assert!(hints(&v).contains("nur noch 5 Leben – nicht aufgeben"));
}

#[test]
fn opponent_turn_blocks_and_tricks() {
    let db = std_db();
    let mut ls = base();
    ls.my_turn = false;
    ls.phase = "Phase_Combat".into();
    ls.life_me = 4;
    ls.opp_attackers = 2;
    ls.opp_attacking = vec![creature_at(OGRE, 3, 3), creature_at(BEAR, 2, 2)];
    ls.my_blockers = vec![creature_at(GIANT, 6, 6)];
    ls.hand = vec![SHOCK];
    ls.untapped_lands = vec![MOUNTAIN];
    ls.opp_untapped_lands = 1;
    ls.opp_cards = vec![GROWTH];
    let v = build_view(Some(&ls), None, &db, &[]);
    let h = hints(&v);
    assert_eq!(v.status, "Zug 3 · Gegner am Zug · Kampf");
    assert!(h.contains("Jetzt spontan einsetzbar: Shock"), "{h}");
    assert!(h.contains("Stone Giant blockt Hill Ogre (gewinnt)"), "{h}");
    assert!(h.contains("Du nimmst 2 statt 5 Schaden"), "{h}");
    assert!(h.contains("möglich: Giant Growth (schon gesehen)"), "{h}");

    // Ohne Blocker und tödlich
    ls.my_blockers.clear();
    ls.life_me = 3;
    let v = build_view(Some(&ls), None, &db, &[]);
    assert!(hints(&v).contains("nur ein Trick rettet dich"));
    // Ohne Blocker, nicht tödlich
    ls.life_me = 20;
    ls.hand.clear();
    ls.opp_untapped_lands = 0;
    let v = build_view(Some(&ls), None, &db, &[]);
    assert!(
        hints(&v).contains("Kein lohnender Block – nicht blocken, du nimmst 5 Schaden"),
        "{}",
        hints(&v)
    );
    // Opfern, wenn es sonst tödlich wäre
    ls.life_me = 5;
    ls.my_blockers = vec![creature_at(ELF, 1, 1)];
    let v = build_view(Some(&ls), None, &db, &[]);
    assert_eq!(level_of(&v, "opfern, sonst tödlich"), Level::Warn);
    // Bedrohlich: zwei Blocker gemeinsam
    let mut menace = creature_at(OGRE, 3, 3);
    menace.menace = true;
    ls.opp_attacking = vec![menace];
    ls.my_blockers = vec![creature_at(BEAR, 2, 4), creature_at(ELF, 2, 4)];
    ls.life_me = 20;
    let v = build_view(Some(&ls), None, &db, &[]);
    assert!(
        hints(&v).contains("Grizzly Bears und Llanowar Elves blocken Hill Ogre (gewinnt)"),
        "{}",
        hints(&v)
    );
    ls.opp_attacking = vec![creature_at(OGRE, 3, 3), creature_at(BEAR, 2, 2)];
    // Gegner greift nicht an: nichts zu tun
    ls.opp_attackers = 0;
    ls.opp_attacking.clear();
    let v = build_view(Some(&ls), None, &db, &[]);
    assert_eq!(hints(&v), "Gerade nichts zu beachten.");
}

fn shares(name: &str, cards: &[(&str, f32)]) -> ArchShare {
    ArchShare {
        name: name.into(),
        share: cards.iter().map(|(k, s)| (k.to_string(), *s)).collect(),
    }
}

#[test]
fn recognizes_opponent_deck() {
    let db = std_db();
    let meta = vec![
        shares(
            "Gruul Aggro",
            &[
                ("grizzly bears", 1.0),
                ("hill ogre", 0.8),
                ("giant growth", 0.6),
            ],
        ),
        shares("Dimir Control", &[("cancel", 1.0), ("murder", 1.0)]),
    ];
    let mut ls = base();
    ls.opp_cards = vec![BEAR, OGRE, FOREST];
    let v = build_view(Some(&ls), None, &db, &meta);
    assert_eq!(v.opponent, "vermutlich Gruul Aggro");
    assert_eq!(v.opp_cards, vec!["Grizzly Bears", "Hill Ogre"]);
    // Offenes Mana: häufiger Trick des erkannten Decks.
    ls.opp_untapped_lands = 2;
    let v = build_view(Some(&ls), None, &db, &meta);
    assert!(hints(&v).contains("Giant Growth (60 %)"), "{}", hints(&v));
    // Unbekannt: Farben aus den Karten.
    ls.opp_cards = vec![SPIRIT];
    let v = build_view(Some(&ls), None, &db, &meta);
    assert_eq!(v.opponent, crate::opponents::color_label("U"));
    ls.opp_cards.clear();
    let v = build_view(Some(&ls), None, &db, &meta);
    assert_eq!(v.opponent, "noch unbekannt");
}

#[test]
fn name_lists_are_shortened() {
    let db = std_db();
    assert_eq!(
        names(&db, &[BEAR, BEAR, OGRE], 4),
        "Grizzly Bears, Hill Ogre"
    );
    assert_eq!(
        names(&db, &[BEAR, OGRE, ELF], 2),
        "Grizzly Bears, Hill Ogre (+1)"
    );
    assert_eq!(names(&db, &[999], 2), "");
    assert_eq!(phase_label("Phase_Beginning"), "Zugbeginn");
    assert_eq!(phase_label("Phase_Ending"), "Zugende");
    assert_eq!(phase_label("x"), "");
}

#[test]
fn burn_from_hand() {
    let db = std_db();
    let hand: Vec<&Card> = [SHOCK, SPIKE, BEAR].iter().map(|g| &db.cards[g]).collect();
    assert_eq!(
        best_burn(&hand, 2),
        (5, vec!["Shock".to_string(), "Lava Spike".to_string()])
    );
    assert_eq!(best_burn(&hand, 1).0, 3);
    assert_eq!(best_burn(&hand, 0).0, 0);
}

#[test]
fn shares_from_tournament_lists() {
    let db = std_db();
    let a = deck::parse("4 Shock\n4 Hill Ogre\nSideboard\n2 Shock\n");
    let b = deck::parse("4 Shock\n4 Unknown\n");
    let archs = vec![
        crate::deckcheck::MetaArchetype {
            id: 1,
            name: "Mono Red".into(),
            lists: vec![&a, &b],
        },
        crate::deckcheck::MetaArchetype {
            id: 2,
            name: "Leer".into(),
            lists: vec![],
        },
    ];
    let s = arch_shares(&archs, &db);
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].share["shock"], 1.0, "pro Liste nur einmal gezählt");
    assert_eq!(s[0].share["hill ogre"], 0.5);
    assert!(!s[0].share.contains_key("unknown"));
}

#[test]
fn log_tail_follows_file() {
    use std::io::Write;
    let dir = TempDir::new("tail");
    let path = dir.path("Player.log");
    let mut tail = LogTail::default();
    let mut got: Vec<String> = Vec::new();
    let read = |tail: &mut LogTail, got: &mut Vec<String>| {
        tail.read(&path, |l| got.push(String::from_utf8_lossy(l).into()))
    };
    // Datei fehlt noch
    assert!(!read(&mut tail, &mut got));
    std::fs::write(&path, "eins\r\nzwei\r\ndr").unwrap();
    assert!(!read(&mut tail, &mut got));
    assert_eq!(got, vec!["eins", "zwei"], "halbe Zeile wartet");
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    f.write_all(b"ei\n\nvier\n").unwrap();
    drop(f);
    assert!(!read(&mut tail, &mut got));
    assert_eq!(got, vec!["eins", "zwei", "drei", "vier"]);
    // Arena beginnt ein neues Log: Datei wird kürzer.
    std::fs::write(&path, "neu\n").unwrap();
    assert!(read(&mut tail, &mut got));
    assert!(!read(&mut tail, &mut got));
    assert_eq!(got.last().unwrap(), "neu");
}

#[test]
fn draws_companion_window() {
    let db = std_db();
    let mut ls = base();
    ls.hand = vec![MOUNTAIN, OGRE];
    ls.untapped_lands = vec![MOUNTAIN, MOUNTAIN];
    ls.opp_cards = vec![BEAR];
    ls.opp_untapped_lands = 1;
    ls.life_opp = 4;
    let view = build_view(Some(&ls), None, &db, &[]);
    assert!(view.hints.iter().any(|(l, _)| *l == Level::Good));
    let ctx = egui::Context::default();
    let _ = ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| draw(ui, &view));
    });
    let _ = ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| draw(ui, &CompanionView::default()));
    });
}

/// Logzeilen einer kurzen Partie, die Sitz `winner` gewinnt.
fn game_lines(match_id: &str, winner: u32, finished: bool) -> Vec<String> {
    let mut lines = vec![
        gre(
            1,
            vec![state(
                1,
                serde_json::json!({
                    "gameStateId": 1,
                    "gameInfo": game_info(match_id, 1),
                    "turnInfo": turn_info(1, 1, "Phase_Main1", ""),
                    "players": players(20, 20)
                }),
            )],
        ),
        gre(
            2,
            vec![state(
                1,
                serde_json::json!({ "gameStateId": 2, "turnInfo": turn_info(2, 2, "Phase_Main1", "") }),
            )],
        ),
    ];
    if finished {
        lines.push(gre(
            3,
            vec![state(
                1,
                serde_json::json!({ "gameStateId": 3, "gameInfo": game_over(match_id, 1, winner, "ResultReason_Game") }),
            )],
        ));
    }
    lines
}

fn append(path: &Path, lines: &[String]) {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .unwrap();
    for l in lines {
        writeln!(f, "{l}").unwrap();
    }
}

#[test]
fn follower_reports_new_results_only() {
    let db = std_db();
    let dir = TempDir::new("follow");
    let log = dir.path("Player.log");
    // Beim Start steht schon eine beendete Partie im Log: nicht melden.
    append(&log, &game_lines("alt", 1, true));
    let mut f = Follower::default();
    let v = f.poll(&log, &db, &[]);
    assert!(v.status.starts_with("Warte"), "{}", v.status);

    // Neue Partie läuft …
    append(&log, &game_lines("neu", 2, false));
    let v = f.poll(&log, &db, &[]);
    assert_eq!(v.status, "Zug 2 · Gegner am Zug · 1. Hauptphase");
    // … und endet mit einer Niederlage.
    append(&log, &game_lines("neu", 2, true)[2..]);
    let v = f.poll(&log, &db, &[]);
    assert_eq!(v.status, "Partie vorbei – verloren.");
    // Nichts Neues: Anzeige bleibt.
    assert_eq!(f.poll(&log, &db, &[]), v);

    // Arena-Neustart: neues, kürzeres Log mit einer alten Partie.
    std::fs::write(&log, "").unwrap();
    append(&log, &game_lines("x", 1, false));
    let v = f.poll(&log, &db, &[]);
    assert!(v.status.starts_with("Zug 2"), "{}", v.status);
}
