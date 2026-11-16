use super::*;
use crate::testutil::*;
use serde_json::{Value, json};

fn creature(power: i32, toughness: i32, keywords: &[u32]) -> super::Obj {
    super::Obj {
        creature: true,
        power,
        toughness,
        keywords: keywords.to_vec(),
        ..Default::default()
    }
}

#[test]
fn safe_attack_rules() {
    let bear = creature(2, 2, &[]);
    let wall = creature(0, 4, &[]);
    let ogre = creature(3, 3, &[]);
    let bird = creature(1, 1, &[KW_FLYING]);
    // Kein Blocker -> gefahrlos
    assert!(safe_attack(&bear, &[]));
    // Blocker ohne Kraft kann nicht töten
    assert!(safe_attack(&bear, &[&wall]));
    // 3/3 tötet 2/2
    assert!(!safe_attack(&bear, &[&ogre]));
    // Fliegender Angreifer kann vom Oger nicht geblockt werden
    assert!(safe_attack(&bird, &[&ogre]));
    // ... wohl aber von einem Blocker mit Reichweite
    assert!(!safe_attack(&bird, &[&creature(1, 3, &[KW_REACH])]));
    // Todesberührung tötet immer
    assert!(!safe_attack(&ogre, &[&creature(1, 1, &[KW_DEATHTOUCH])]));
    // Bedrohlich: ein einzelner Blocker reicht nicht
    assert!(safe_attack(&creature(2, 2, &[KW_MENACE]), &[&ogre]));
    // ... zwei schwache Blocker zusammen auch nicht, zwei starke schon
    let menace = creature(3, 3, &[KW_MENACE]);
    let elf = creature(1, 1, &[]);
    assert!(safe_attack(&menace, &[&elf, &elf]));
    assert!(!safe_attack(&menace, &[&ogre, &elf]));
    assert!(!safe_attack(
        &menace,
        &[&elf, &creature(1, 1, &[KW_DEATHTOUCH])]
    ));
}

// ------------------------------------------------- künstliche Partie ----

const MATCH: &str = "m-1";
const MATCH2: &str = "m-2";

/// Objekte der ersten Partie (Sitz 1 = ich, Sitz 2 = Gegner).
fn mountain(id: u32) -> crate::testutil::Obj {
    obj(id, MOUNTAIN, Z_HAND_1, 1).land()
}

fn line(p: &mut LogParser, text: &str) {
    p.feed(text.as_bytes());
}

fn state_msg(p: &mut LogParser, ts: u64, s: Value) {
    line(p, &gre(ts, vec![state(1, s)]));
}

/// Spielt die erste Partie bis kurz vor Spielende ein und prüft unterwegs
/// den Live-Zustand. Ich verliere durch Aufgabe in Zug 4.
fn play_first_game(p: &mut LogParser) {
    let deck = [MOUNTAIN, MOUNTAIN, OGRE, SHOCK, BEAR, ELF];
    line(p, &room(MATCH, 1, "Ladder"));
    line(p, &gre(1, vec![connect(1, &deck)]));
    // Starthand: 3 Länder + 4 Zauber
    let hand: Vec<Value> = vec![
        mountain(100).json(),
        mountain(101).json(),
        mountain(102).json(),
        obj(103, OGRE, Z_HAND_1, 1).creature(3, 3).json(),
        obj(104, SHOCK, Z_HAND_1, 1).json(),
        obj(105, BEAR, Z_HAND_1, 1).creature(2, 2).json(),
        obj(106, ELF, Z_HAND_1, 1).creature(1, 1).json(),
    ];
    state_msg(
        p,
        1000,
        json!({
            "type": "GameStateType_Full",
            "gameStateId": 1,
            "gameInfo": game_info(MATCH, 1),
            "zones": zones(&[100, 101, 102, 103, 104, 105, 106], &[], &[]),
            "players": players(20, 20),
            "gameObjects": hand,
        }),
    );
    line(p, &gre(1001, vec![mulligan(1)]));
    let live = p.live().expect("Starthand läuft");
    assert!(live.mulligan);
    assert_eq!(live.hand.len(), 7);

    // Zug 1 (ich): Berg gelegt
    let mut m100 = mountain(100);
    m100.zone = Z_BATTLEFIELD;
    state_msg(
        p,
        1100,
        json!({
            "gameStateId": 2,
            "turnInfo": turn_info(1, 1, "Phase_Main1", ""),
            "zones": zones(&[101, 102, 103, 104, 105, 106], &[], &[100]),
            "gameObjects": [m100.json()],
            "annotations": [zone_transfer(1, 100, "PlayLand")],
        }),
    );
    let live = p.live().unwrap();
    assert!(!live.mulligan && live.my_turn && live.land_played);
    assert_eq!(live.lands_untapped, 1);
    assert_eq!(live.untapped_lands, vec![MOUNTAIN]);

    // Zug 2 (Gegner): Wald und Bär, der Bär greift an und macht 2 Schaden
    state_msg(
        p,
        1200,
        json!({
            "gameStateId": 3,
            "turnInfo": turn_info(2, 2, "Phase_Main1", ""),
            "zones": zones(&[101, 102, 103, 104, 105, 106], &[], &[100, 201, 200]),
            "gameObjects": [
                obj(201, FOREST, Z_BATTLEFIELD, 2).land().json(),
                obj(200, BEAR, Z_BATTLEFIELD, 2).creature(2, 2).sick().json(),
            ],
            "annotations": [zone_transfer(2, 201, "PlayLand"), zone_transfer(3, 200, "CastSpell")],
        }),
    );
    state_msg(
        p,
        1300,
        json!({
            "gameStateId": 4,
            "turnInfo": turn_info(2, 2, "Phase_Combat", "Step_CombatDamage"),
            "players": players(18, 20),
            "zones": zones(&[101, 102, 104, 105, 106], &[], &[100, 201, 200, 103]),
            "gameObjects": [
                obj(200, BEAR, Z_BATTLEFIELD, 2).creature(2, 2).attacking().json(),
                obj(103, OGRE, Z_BATTLEFIELD, 1).creature(3, 3).sick().json(),
            ],
            "annotations": [damage(4, 200, 1, 2), damage(4, 200, 1, 2)],
        }),
    );
    let live = p.live().unwrap();
    assert!(!live.my_turn);
    assert_eq!(live.opp_attackers, 1);
    assert_eq!(live.opp_attacking.len(), 1);
    assert_eq!(live.life_me, 18);

    // Zug 3 (ich): Oger liegt bereit, kein Land gelegt, Shock gewirkt
    let mut m100_tapped = mountain(100).tapped();
    m100_tapped.zone = Z_BATTLEFIELD;
    state_msg(
        p,
        1400,
        json!({
            "gameStateId": 5,
            "turnInfo": turn_info(3, 1, "Phase_Main1", ""),
            "zones": zones(&[101, 102, 104, 105, 106], &[], &[100, 201, 200, 103]),
            "gameObjects": [
                obj(103, OGRE, Z_BATTLEFIELD, 1).creature(3, 3).json(),
                obj(200, BEAR, Z_BATTLEFIELD, 2).creature(2, 2).json(),
            ],
        }),
    );
    let live = p.live().unwrap();
    assert_eq!(live.turn, 3);
    assert_eq!(
        live.safe_attackers,
        vec![OGRE],
        "Bär kann den Oger nicht töten"
    );
    assert_eq!(live.attackers.len(), 1);
    assert_eq!(live.blockers.len(), 1);
    assert_eq!(live.opp_creatures, vec![(BEAR, 2)]);
    assert_eq!(live.opp_untapped_lands, 1);
    assert_eq!(live.creatures_me, 1);
    assert!(!live.land_played);
    state_msg(
        p,
        1500,
        json!({
            "gameStateId": 6,
            "turnInfo": turn_info(3, 1, "Phase_Main1", ""),
            "zones": zones(&[101, 102, 105, 106], &[], &[100, 201, 200, 103]),
            "gameObjects": [m100_tapped.json(), obj(104, SHOCK, Z_STACK, 1).json()],
            "annotations": [zone_transfer(5, 104, "CastSpell")],
        }),
    );
    // Veraltete Nachricht (kleinere gameStateId) wird ignoriert.
    state_msg(
        p,
        1550,
        json!({ "gameStateId": 3, "players": players(1, 1) }),
    );
    assert_eq!(p.live().unwrap().life_me, 18);

    // Zug 4 (Gegner): Bär greift an, ich könnte mit dem Oger blocken
    state_msg(
        p,
        1590,
        json!({ "gameStateId": 7, "turnInfo": turn_info(4, 2, "Phase_Main1", "") }),
    );
    state_msg(
        p,
        1600,
        json!({
            "gameStateId": 8,
            "turnInfo": turn_info(4, 2, "Phase_Combat", "Step_DeclareAttack"),
            "gameObjects": [obj(200, BEAR, Z_BATTLEFIELD, 2).creature(2, 2).attacking().json()],
            "annotations": [zone_transfer(6, 106, "Discard")],
        }),
    );
    let live = p.live().unwrap();
    assert_eq!(live.my_blockers.len(), 1);
    assert_eq!(live.opp_attacking[0].power, 2);
}

fn end_first_game(p: &mut LogParser) {
    state_msg(
        p,
        1700,
        json!({ "gameStateId": 9, "gameInfo": game_over(MATCH, 1, 2, "ResultReason_Concede") }),
    );
}

#[test]
fn records_a_complete_game() {
    let mut p = LogParser::default();
    play_first_game(&mut p);
    end_first_game(&mut p);
    assert!(p.live().is_none(), "Partie beendet");
    assert_eq!(p.last_finished.as_ref().unwrap().id, "m-1#1");
    let games = p.finish();
    assert_eq!(games.len(), 1);
    let g = &games[0];
    assert_eq!(g.id, "m-1#1");
    assert_eq!(g.event, "Ladder");
    assert_eq!(g.won, Some(false));
    assert!(g.conceded());
    assert!(g.complete);
    assert_eq!(g.on_play, Some(true));
    assert_eq!(g.mulligans, 0);
    assert_eq!(g.deck.len(), 6);
    assert_eq!(g.opening_hand.len(), 7);
    assert_eq!(g.last_turn(), 4);
    assert_eq!(g.final_life(), (18, 20));
    assert_eq!(
        g.damage_taken.get(&BEAR),
        Some(&2),
        "doppelte Annotation zählt einmal"
    );
    assert_eq!(g.discards_me, 1);
    assert!(g.opp_cards.contains_key(&BEAR));
    assert!(g.cast_cards().contains(&SHOCK));
    assert!(g.seen_cards().contains(&OGRE));

    let t1 = &g.turns[0];
    assert!(t1.mine && t1.land_played);
    assert_eq!(t1.untapped_lands, 1);
    let t2 = &g.turns[1];
    assert!(!t2.mine && t2.attacked && t2.land_played);
    assert_eq!(t2.spells, 1);
    let t3 = &g.turns[2];
    assert!(t3.mine && !t3.land_played && !t3.attacked);
    assert_eq!(t3.safe_idle, vec![OGRE]);
    assert_eq!(t3.cast, vec![SHOCK]);
    assert_eq!(t3.creatures_at_start, 1);
    let t4 = &g.turns[3];
    assert_eq!(t4.my_untapped_creatures, 1);
    assert!(!t4.blocked);
}

#[test]
fn next_game_after_leftover_message() {
    let mut p = LogParser::default();
    play_first_game(&mut p);
    end_first_game(&mut p);
    // Arena meldet das Spielende doppelt und schickt danach noch Zustände ohne Kennung.
    end_first_game(&mut p);
    state_msg(
        &mut p,
        1800,
        json!({ "gameStateId": 10, "turnInfo": turn_info(4, 2, "Phase_Ending", "") }),
    );
    // Neue Partie: der Zähler beginnt wieder bei 1.
    line(&mut p, &room(MATCH2, 1, "Play"));
    state_msg(
        &mut p,
        2000,
        json!({
            "type": "GameStateType_Full",
            "gameStateId": 1,
            "gameInfo": game_info(MATCH2, 1),
            "zones": zones(&[300], &[], &[]),
            "players": players(20, 20),
            "gameObjects": [mountain(300).json()],
        }),
    );
    state_msg(
        &mut p,
        2100,
        json!({ "gameStateId": 2, "turnInfo": turn_info(1, 2, "Phase_Main1", "") }),
    );
    let live = p.live().expect("neue Partie wird verfolgt");
    assert_eq!(live.turn, 1);
    assert!(!live.my_turn);
    state_msg(
        &mut p,
        2200,
        json!({ "gameStateId": 3, "turnInfo": turn_info(2, 1, "Phase_Main1", "") }),
    );
    state_msg(
        &mut p,
        2300,
        json!({ "gameStateId": 4, "gameInfo": game_over(MATCH2, 1, 1, "ResultReason_Game") }),
    );
    let games = p.finish();
    assert_eq!(games.len(), 2, "Doppelmeldung zusammengefasst");
    let first = games.iter().find(|g| g.id == "m-1#1").unwrap();
    assert_eq!(first.turns.len(), 4, "beste Aufzeichnung behalten");
    assert_eq!(first.deck.len(), 6);
    let second = games.iter().find(|g| g.id == "m-2#1").unwrap();
    assert_eq!(second.won, Some(true));
    assert_eq!(second.event, "Play");
    assert_eq!(second.on_play, Some(false));
    assert_eq!(second.turns.len(), 2);
}

#[test]
fn ignores_noise_and_partial_games() {
    let mut p = LogParser::default();
    line(&mut p, "[UnityCrossThreadLogger] irgendwas");
    line(&mut p, "{ kein json");
    line(&mut p, r#"{ "andere": "Nachricht" }"#);
    assert!(p.live().is_none());
    // Mitschnitt beginnt mitten in der Partie (Zug 5): nicht vollständig.
    state_msg(
        &mut p,
        1,
        json!({
            "gameStateId": 50,
            "gameInfo": game_info("m-3", 1),
            "turnInfo": turn_info(5, 1, "Phase_Main1", "")
        }),
    );
    state_msg(
        &mut p,
        2,
        json!({
            "gameStateId": 51,
            "gameInfo": { "matchID": "m-3", "gameNumber": 1, "stage": "GameStage_GameOver",
                          "results": [ { "scope": "MatchScope_Game", "result": "ResultType_Draw" } ] }
        }),
    );
    let games = p.finish();
    assert_eq!(games.len(), 1);
    assert!(!games[0].complete);
    assert_eq!(games[0].won, None);
    assert_eq!(games[0].on_play, None);
}

fn write_log(dir: &TempDir, name: &str, finished: bool) -> PathBuf {
    let mut lines: Vec<String> = Vec::new();
    lines.push("DETAILED LOGS: ENABLED".into());
    lines.push(room(MATCH, 1, "Ladder"));
    lines.push(gre(1, vec![connect(1, &[MOUNTAIN, OGRE])]));
    lines.push(gre(
        2,
        vec![state(
            1,
            json!({
                "gameStateId": 1,
                "gameInfo": game_info(MATCH, 1),
                "turnInfo": turn_info(1, 1, "Phase_Main1", ""),
                "zones": zones(&[100], &[], &[]),
                "gameObjects": [mountain(100).json()],
            }),
        )],
    ));
    lines.push(gre(
        3,
        vec![state(
            1,
            json!({ "gameStateId": 2, "turnInfo": turn_info(2, 2, "Phase_Main1", "") }),
        )],
    ));
    if finished {
        lines.push(gre(
            4,
            vec![state(
                1,
                json!({ "gameStateId": 3, "gameInfo": game_over(MATCH, 1, 1, "ResultReason_Game") }),
            )],
        ));
    }
    let path = dir.path(name);
    std::fs::write(&path, lines.join("\r\n")).unwrap();
    path
}

#[test]
fn parses_log_files_and_syncs_store() {
    let dir = TempDir::new("games");
    let partial = write_log(&dir, "Player-prev.log", false);
    let log = write_log(&dir, "Player.log", true);
    assert!(parse_log(&partial).is_empty(), "unvollendete Partie");
    assert_eq!(parse_log(&log).len(), 1);
    assert!(parse_log(&dir.path("fehlt.log")).is_empty());
    assert!(!detailed_logs_disabled(&log));
    std::fs::write(dir.path("off.log"), "x\nDETAILED LOGS: DISABLED\n").unwrap();
    assert!(detailed_logs_disabled(&dir.path("off.log")));
    assert!(!detailed_logs_disabled(&dir.path("fehlt.log")));

    let store = dir.path("games.json");
    let games = sync(&store, &[partial.clone(), log.clone()]);
    assert_eq!(games.len(), 1);
    assert!(store.exists());
    let written = std::fs::metadata(&store).unwrap().modified().unwrap();
    // Zweiter Abgleich ohne neue Partien schreibt nichts.
    let again = sync(&store, std::slice::from_ref(&log));
    assert_eq!(again.len(), 1);
    assert_eq!(
        std::fs::metadata(&store).unwrap().modified().unwrap(),
        written
    );
    // Ältere Speicherform (nur Liste) wird übernommen und neu berechnet.
    let mut legacy = games.clone();
    legacy[0].turns.clear();
    std::fs::write(&store, serde_json::to_vec(&legacy).unwrap()).unwrap();
    let upgraded = sync(&store, &[log]);
    assert_eq!(upgraded[0].turns.len(), games[0].turns.len());
    // Ohne Logs bleiben gespeicherte Partien erhalten.
    assert_eq!(sync(&store, &[]).len(), 1);
}

// ------------------------------------------------------------ Auswertung ----

fn turn(n: u32, mine: bool) -> TurnStat {
    TurnStat {
        turn: n,
        mine,
        life_me: 20,
        life_opp: 20,
        land_played: true,
        ..Default::default()
    }
}

fn game(id: &str, won: Option<bool>, turns: Vec<TurnStat>) -> GameRecord {
    GameRecord {
        id: id.into(),
        won,
        complete: true,
        on_play: Some(true),
        turns,
        ..Default::default()
    }
}

#[test]
fn report_finds_mistakes() {
    let db = std_db();
    // Partie 1: Land vergessen, Mana ungenutzt, gefahrloser Angriff ausgelassen,
    // keine Kreatur in Zug 3, Karte lange gehalten, bei wenig Leben nicht geblockt.
    let mut t1 = turn(1, true);
    t1.hand = vec![OGRE, MOUNTAIN];
    t1.land_played = false;
    t1.lands_me = 3;
    let mut t3 = turn(3, true);
    t3.hand = vec![OGRE];
    t3.untapped_lands = 3;
    t3.lands_me = 3;
    t3.safe_idle = vec![BEAR];
    t3.life_me = 9;
    let mut t4 = turn(4, false);
    t4.life_me = 6;
    t4.my_untapped_creatures = 1;
    let mut t5 = turn(5, true);
    t5.hand = vec![OGRE, SHOCK];
    t5.lands_me = 3;
    t5.life_me = 6;
    t5.creatures_me = 1;
    let mut g1 = game("a", Some(false), vec![t1, t3, t4, t5]);
    g1.opening_hand = vec![MOUNTAIN, OGRE, OGRE, OGRE, OGRE, OGRE, OGRE];
    g1.reason = "ResultReason_Concede".into();
    g1.opp_cards.insert(BEAR, 2);
    g1.opp_cards.insert(FOREST, 3);
    g1.damage_taken.insert(BEAR, 14);
    g1.discards_me = 1;

    // Partie 2: gewonnen, gute Starthand, Teil-Mitschnitt (keine Zugauswertung)
    let mut g2 = game("b", Some(true), vec![turn(7, true)]);
    g2.complete = false;
    g2.on_play = Some(false);
    g2.opening_hand = vec![MOUNTAIN, MOUNTAIN, MOUNTAIN, OGRE, OGRE, OGRE, OGRE];
    g2.opp_cards.insert(BEAR, 1);
    g2.damage_taken.insert(SHOCK, 2);

    let r = report(&[g1, g2], &db);
    assert_eq!((r.games, r.wins, r.losses, r.conceded), (2, 1, 1, 1));
    assert_eq!(r.on_play, (0, 1));
    assert_eq!(r.on_draw, (1, 1));
    assert_eq!(r.good_keeps, (1, 1));
    assert_eq!(r.bad_keeps, (0, 1));
    assert_eq!(r.discards, 1);
    assert_eq!(r.my_turns, 3);
    assert_eq!(r.avg_turns, 6.0);
    assert_eq!(r.opp_cards, vec![("Grizzly Bears".to_string(), 2)]);
    assert_eq!(
        r.damage_sources,
        vec![("Grizzly Bears".to_string(), 14), ("Shock".to_string(), 2)]
    );

    let cat = |title: &str| {
        r.categories
            .iter()
            .find(|c| c.title == title)
            .unwrap_or_else(|| panic!("{title}"))
    };
    assert_eq!(cat("Land nicht gespielt").items.len(), 1);
    assert_eq!(cat("Gefahrlosen Angriff ausgelassen").items[0].turn, 3);
    assert_eq!(cat("Mana ungenutzt").items.len(), 1);
    assert_eq!(cat("Keine Kreatur auf dem Feld").items.len(), 1);
    assert_eq!(cat("Riskante Starthand behalten").items[0].turn, 0);
    assert_eq!(cat("Bei wenig Leben nicht geblockt").items.len(), 1);
    let held = &cat("Karte lange nicht gespielt").items;
    assert_eq!(held.len(), 1);
    assert!(held[0].text.starts_with("Hill Ogre lag 3"));
    assert_eq!(cat("Früh aufgegeben").items.len(), 1);
    // Häufigste Fehlerart zuerst.
    assert!(
        r.categories
            .windows(2)
            .all(|w| w[0].items.len() >= w[1].items.len())
    );
}

#[test]
fn report_of_nothing() {
    let r = report(&[], &std_db());
    assert_eq!(r.games, 0);
    assert_eq!(r.avg_turns, 0.0);
    assert!(r.categories.iter().all(|c| c.items.is_empty()));
    assert_eq!(GameRecord::default().final_life(), (20, 20));
    assert_eq!(GameRecord::default().last_turn(), 0);
}

#[test]
fn colors_from_cards() {
    let db = std_db();
    assert_eq!(deck_colors(&[OGRE, BEAR, SHOCK, MOUNTAIN], &db), "RG");
    assert_eq!(deck_colors(&[], &db), "");
    let mut g = GameRecord::default();
    g.opp_cards.insert(MURDER, 1);
    g.opp_cards.insert(SPIRIT, 1);
    assert_eq!(opp_colors(&g, &db), "UB");
}
