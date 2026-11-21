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
    let bird = creature(1, 1, &[kw::FLYING]);
    // No blocker -> safe
    assert!(safe_attack(&bear, &[]));
    // A blocker without power can't kill
    assert!(safe_attack(&bear, &[&wall]));
    // 3/3 kills 2/2
    assert!(!safe_attack(&bear, &[&ogre]));
    // A flying attacker can't be blocked by the ogre
    assert!(safe_attack(&bird, &[&ogre]));
    // ... but by a blocker with reach
    assert!(!safe_attack(&bird, &[&creature(1, 3, &[kw::REACH])]));
    // Deathtouch always kills
    assert!(!safe_attack(&ogre, &[&creature(1, 1, &[kw::DEATHTOUCH])]));
    // Menace: a single blocker isn't enough
    assert!(safe_attack(&creature(2, 2, &[kw::MENACE]), &[&ogre]));
    // ... two weak blockers together aren't either, two strong ones are
    let menace = creature(3, 3, &[kw::MENACE]);
    let elf = creature(1, 1, &[]);
    assert!(safe_attack(&menace, &[&elf, &elf]));
    assert!(!safe_attack(&menace, &[&ogre, &elf]));
    assert!(!safe_attack(
        &menace,
        &[&elf, &creature(1, 1, &[kw::DEATHTOUCH])]
    ));
}

// ------------------------------------------------------ synthetic game ----

const MATCH: &str = "m-1";
const MATCH2: &str = "m-2";

/// Objects of the first game (seat 1 = me, seat 2 = opponent).
fn mountain(id: u32) -> crate::testutil::Obj {
    obj(id, MOUNTAIN, Z_HAND_1, 1).land()
}

fn line(p: &mut LogParser, text: &str) {
    p.feed(text.as_bytes());
}

fn state_msg(p: &mut LogParser, ts: u64, s: Value) {
    line(p, &gre(ts, vec![state(1, s)]));
}

/// Plays the first game up to shortly before its end and checks the live
/// state on the way. I lose by conceding on turn 4.
fn play_first_game(p: &mut LogParser) {
    let deck = [MOUNTAIN, MOUNTAIN, OGRE, SHOCK, BEAR, ELF];
    line(p, &room(MATCH, 1, "Ladder"));
    line(p, &gre(1, vec![connect(1, &deck)]));
    // Opening hand: 3 lands + 4 spells
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
    let live = p.live().expect("opening hand running");
    assert!(live.mulligan);
    assert_eq!(live.hand.len(), 7);

    // Turn 1 (me): mountain played
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

    // Turn 2 (opponent): forest and bear, the bear attacks for 2 damage
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
    assert!(
        live.mana_permanents.is_empty(),
        "ogre has summoning sickness"
    );

    // Turn 3 (me): ogre ready, no land played, Shock cast
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
    assert_eq!(live.safe_attackers, vec![OGRE], "bear can't kill the ogre");
    assert_eq!(live.attackers.len(), 1);
    assert_eq!(live.blockers.len(), 1);
    assert_eq!(live.opp_creatures, vec![(BEAR, 2)]);
    assert_eq!(live.opp_untapped_lands, 1);
    assert_eq!(live.creatures_me, 1);
    assert_eq!(live.mana_permanents, vec![OGRE], "ready, not tapped");
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
    // An outdated message (smaller gameStateId) is ignored.
    state_msg(
        p,
        1550,
        json!({ "gameStateId": 3, "players": players(1, 1) }),
    );
    assert_eq!(p.live().unwrap().life_me, 18);

    // Turn 4 (opponent): bear attacks, I could block with the ogre
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
    // Then Arena reports the rank.
    line(&mut p, RANK_LINE);
    assert!(p.live().is_none(), "Partie beendet");
    assert_eq!(p.rank.as_ref().unwrap().level, 4);
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
        "duplicate annotation counts once"
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
    assert_eq!(t3.lands, vec![MOUNTAIN]);
    assert_eq!(t3.opp_creatures, vec![(BEAR, 2)]);
    let t4 = &g.turns[3];
    assert_eq!(t4.my_untapped_creatures, 1);
    assert!(!t4.blocked);
    let rank = g.rank_after.as_ref().expect("rank after the game");
    assert_eq!(
        (rank.class.as_str(), rank.level, rank.step),
        ("Bronze", 4, 2)
    );
    assert_eq!((rank.won, rank.lost), (1, 11));
}

const RANK_LINE: &str = r#"{"constructedSeasonOrdinal":95,"constructedLevel":4,"constructedStep":2,"constructedMatchesWon":1,"constructedMatchesLost":11,"limitedSeasonOrdinal":95,"limitedLevel":4}"#;

#[test]
fn rank_snapshots() {
    let r = Rank::parse(RANK_LINE).unwrap();
    assert_eq!(r.season, 95);
    assert_eq!(r.class, "Bronze", "fehlende Klasse = Bronze");
    assert_eq!(
        (r.limited_class.as_str(), r.limited_level, r.limited_step),
        ("Bronze", 4, 0)
    );
    let silver = Rank::parse(
        r#"{"constructedSeasonOrdinal":96,"constructedClass":"Silver","constructedLevel":2,"limitedClass":"Gold","limitedLevel":1,"limitedStep":3}"#,
    )
    .unwrap();
    assert_eq!(
        (silver.class.as_str(), silver.level, silver.step),
        ("Silver", 2, 0)
    );
    assert_eq!(
        (
            silver.limited_class.as_str(),
            silver.limited_level,
            silver.limited_step
        ),
        ("Gold", 1, 3)
    );
    assert!(Rank::parse(r#"{"limitedLevel":4}"#).is_none());
    assert!(Rank::parse("kaputt").is_none());

    // A rank before any game (game start) is assigned to no game.
    let mut p = LogParser::default();
    line(&mut p, RANK_LINE);
    play_first_game(&mut p);
    end_first_game(&mut p);
    let games = p.finish();
    assert!(games[0].rank_after.is_none());
}

#[test]
fn key_moments_compare_with_suggestion() {
    let db = std_db();
    // Two forests + mountain: ogre and elf would have worked, nothing was cast.
    let t = TurnStat {
        turn: 4,
        mine: true,
        hand: vec![OGRE, ELF, GIANT],
        lands: vec![FOREST, FOREST, MOUNTAIN, MOUNTAIN],
        ..Default::default()
    };
    let moment = key_moment(&t, &db).unwrap();
    // The suggested cards come along, for the images in the interface.
    assert!(!moment.cards.is_empty() && moment.cards.iter().all(|g| t.hand.contains(g)));
    let text = moment.text;
    assert!(
        text.starts_with("0 von 4 Mana genutzt – möglich:"),
        "{text}"
    );
    assert!(text.ends_with("(4 Mana)"), "{text}");
    // Played well: ogre cast, only 1 mana left.
    let good = TurnStat {
        hand: vec![ELF, GIANT],
        cast: vec![OGRE],
        ..t.clone()
    };
    assert!(key_moment(&good, &db).is_none());
    // Colors missing: nothing possible.
    let wrong = TurnStat {
        hand: vec![SPIRIT, COUNTER],
        lands: vec![MOUNTAIN, MOUNTAIN, MOUNTAIN],
        ..t.clone()
    };
    assert!(key_moment(&wrong, &db).is_none());
    // Opponent's turn or older recording without lands: no statement.
    assert!(
        key_moment(
            &TurnStat {
                mine: false,
                ..t.clone()
            },
            &db
        )
        .is_none()
    );
    assert!(
        key_moment(
            &TurnStat {
                lands: vec![],
                ..t.clone()
            },
            &db
        )
        .is_none()
    );

    // An opponent turn afterwards: the last turn of a game is not judged.
    let mut g = game("x", Some(false), vec![t.clone(), turn(2, false)]);
    g.turns[0].lands_me = 4;
    let r = report_all(&[g], &db);
    let cat = r
        .categories
        .iter()
        .find(|c| c.kind == Mistake::BetterTurn)
        .unwrap();
    assert_eq!(cat.items.len(), 1);
    let unused = r
        .categories
        .iter()
        .find(|c| c.kind == Mistake::UnusedMana)
        .unwrap();
    assert!(
        unused.items.is_empty(),
        "the new check replaces the old one"
    );
}

#[test]
fn next_game_after_leftover_message() {
    let mut p = LogParser::default();
    play_first_game(&mut p);
    end_first_game(&mut p);
    // Arena reports the end of the game twice and then still sends states without an id.
    end_first_game(&mut p);
    // The rank is attached to the (worse) duplicate report and must be kept.
    line(&mut p, RANK_LINE);
    state_msg(
        &mut p,
        1800,
        json!({ "gameStateId": 10, "turnInfo": turn_info(4, 2, "Phase_Ending", "") }),
    );
    // New game: the counter starts at 1 again.
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
    let live = p.live().expect("new game is followed");
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
    assert!(
        first.rank_after.is_some(),
        "rank taken over from the duplicate report"
    );
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
    // Recording starts in the middle of the game (turn 5): not complete.
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
    let synced = sync(&store, &[partial.clone(), log.clone()]);
    assert!(synced.problem.is_none());
    let games = synced.games;
    assert_eq!(games.len(), 1);
    assert_eq!(games[0].parser, PARSER_VERSION);
    assert!(store.exists());
    let written = std::fs::metadata(&store).unwrap().modified().unwrap();
    // A second sync without new games writes nothing.
    let again = sync(&store, std::slice::from_ref(&log)).games;
    assert_eq!(again.len(), 1);
    assert_eq!(
        std::fs::metadata(&store).unwrap().modified().unwrap(),
        written
    );
    // The older storage format (list only) is taken over and recomputed.
    let mut legacy = games.clone();
    legacy[0].turns.clear();
    std::fs::write(&store, serde_json::to_vec(&legacy).unwrap()).unwrap();
    let upgraded = sync(&store, std::slice::from_ref(&log)).games;
    assert_eq!(upgraded[0].turns.len(), games[0].turns.len());
    // Without logs stored games are kept – with their old version.
    let mut old = games.clone();
    old[0].parser = 0;
    let file = serde_json::json!({ "version": 2, "games": old });
    std::fs::write(&store, serde_json::to_vec(&file).unwrap()).unwrap();
    let kept = sync(&store, &[]).games;
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].parser, 2, "version from the old file");
    // With a log it is recomputed, even with equal quality.
    assert_eq!(
        sync(&store, std::slice::from_ref(&log)).games[0].parser,
        PARSER_VERSION
    );
    // Unreadable (folder instead of file): reported, nothing overwritten.
    let blocked = dir.path("blocked.json");
    std::fs::create_dir(&blocked).unwrap();
    let failed = sync(&blocked, std::slice::from_ref(&log));
    assert!(failed.problem.is_some());
    assert_eq!(failed.games.len(), 1, "games from the log are there anyway");
    assert!(blocked.is_dir());
}

#[test]
fn corrupt_store_is_backed_up_not_lost() {
    let dir = TempDir::new("games_corrupt");
    let log = write_log(&dir, "Player.log", true);
    let store = dir.path("games.json");
    std::fs::write(&store, b"{ kaputt").unwrap();
    let synced = sync(&store, std::slice::from_ref(&log));
    assert_eq!(synced.games.len(), 1);
    let problem = synced.problem.unwrap();
    assert!(problem.contains("gesichert"), "{problem}");
    // The damaged file lies next to it, the new one is valid.
    let backups: Vec<_> = std::fs::read_dir(&dir.0)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("games.json.corrupt-")
        })
        .collect();
    assert_eq!(backups.len(), 1);
    assert_eq!(std::fs::read(backups[0].path()).unwrap(), b"{ kaputt");
    assert_eq!(sync(&store, &[]).games.len(), 1, "neu gespeichert");
}

// -------------------------------------------------------------- analysis ----

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

/// Analysis with numbers over exactly these games.
fn report_all(games: &[GameRecord], db: &CardDb) -> Report {
    report(games, &GameNumbers::new(games), db)
}

#[test]
fn game_numbers_stay_when_a_newer_game_arrives() {
    // Stored newest first; the number follows time.
    let mut first = game("first", Some(false), vec![]);
    first.started_ms = 1_000;
    let mut second = game("second", Some(true), vec![]);
    second.started_ms = 2_000;
    let before = GameNumbers::new(std::slice::from_ref(&first));
    let after = GameNumbers::new(&[second.clone(), first.clone()]);
    assert_eq!(before.of(&first), 1);
    assert_eq!(after.of(&first), 1, "the first game stays #1");
    assert_eq!(after.of(&second), 2);
    assert_eq!(after.of(&game("unbekannt", None, vec![])), 0);

    // Findings carry the fixed number, also in a selection.
    let mut t = turn(1, true);
    t.hand = vec![MOUNTAIN];
    t.land_played = false;
    first.turns = vec![t, turn(2, false)];
    let all = [second, first.clone()];
    let r = report(
        std::slice::from_ref(&first),
        &GameNumbers::new(&all),
        &std_db(),
    );
    let missed = r
        .categories
        .iter()
        .find(|c| c.kind == Mistake::MissedLand)
        .unwrap();
    assert_eq!(missed.items[0].game, 1);
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
    // Game 1: land forgotten, mana unused, safe attack skipped, no creature on
    // turn 3, card held for long, didn't block at low life.
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
    let mut t6 = turn(6, false);
    t6.life_me = 6;
    let mut g1 = game("a", Some(false), vec![t1, t3, t4, t5, t6]);
    g1.opening_hand = vec![MOUNTAIN, OGRE, OGRE, OGRE, OGRE, OGRE, OGRE];
    g1.reason = EndReason::Concede;
    g1.opp_cards.insert(BEAR, 2);
    g1.opp_cards.insert(FOREST, 3);
    g1.damage_taken.insert(BEAR, 14);
    g1.discards_me = 1;

    // Game 2: won, good opening hand, partial recording (no turn analysis)
    let mut g2 = game("b", Some(true), vec![turn(7, true)]);
    g2.complete = false;
    g2.on_play = Some(false);
    g2.opening_hand = vec![MOUNTAIN, MOUNTAIN, MOUNTAIN, OGRE, OGRE, OGRE, OGRE];
    g2.opp_cards.insert(BEAR, 1);
    g2.damage_taken.insert(SHOCK, 2);

    let r = report_all(&[g1, g2], &db);
    assert_eq!((r.games, r.wins, r.losses, r.conceded), (2, 1, 1, 1));
    assert_eq!(r.on_play, (0, 1));
    assert_eq!(r.on_draw, (1, 1));
    assert_eq!(r.good_keeps, (1, 1));
    assert_eq!(r.bad_keeps, (0, 1));
    assert_eq!(r.discards, 1);
    assert_eq!(r.my_turns, 3);
    assert_eq!(r.avg_turns, 6.5);
    assert_eq!(r.opp_cards, vec![("Grizzly Bears".to_string(), 2)]);
    assert_eq!(
        r.damage_sources,
        vec![("Grizzly Bears".to_string(), 14), ("Shock".to_string(), 2)]
    );

    let cat = |kind: Mistake| {
        r.categories
            .iter()
            .find(|c| c.kind == kind)
            .unwrap_or_else(|| panic!("{kind:?}"))
    };
    assert_eq!(cat(Mistake::MissedLand).items.len(), 1);
    assert_eq!(cat(Mistake::SkippedAttack).items[0].turn, 3);
    assert_eq!(cat(Mistake::UnusedMana).items.len(), 1);
    assert_eq!(cat(Mistake::EmptyBoard).items.len(), 1);
    assert_eq!(cat(Mistake::RiskyKeep).items[0].turn, 0);
    assert_eq!(cat(Mistake::NoBlock).items.len(), 1);
    let held = &cat(Mistake::HeldCard).items;
    assert_eq!(held.len(), 1);
    assert!(held[0].text.starts_with("Hill Ogre lag 3"));
    assert_eq!(held[0].cards, vec![OGRE]);
    assert_eq!(cat(Mistake::MissedLand).items[0].cards, vec![MOUNTAIN]);
    assert!(cat(Mistake::EarlyConcede).items[0].cards.is_empty());
    assert_eq!(cat(Mistake::EarlyConcede).items.len(), 1);
    // Most frequent kind of mistake first.
    assert!(
        r.categories
            .windows(2)
            .all(|w| w[0].items.len() >= w[1].items.len())
    );
}

#[test]
fn report_of_nothing() {
    let r = report_all(&[], &std_db());
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

#[test]
fn end_reasons_keep_arena_values() {
    for value in [
        "ResultReason_Game",
        "ResultReason_Concede",
        "ResultReason_Timeout",
        "Etwas_Neues",
        "",
    ] {
        let r = EndReason::from_arena(value);
        assert_eq!(r.to_arena(), value);
        let json = serde_json::to_string(&r).unwrap();
        assert_eq!(json, format!("\"{value}\""), "unchanged in games.json");
        assert_eq!(serde_json::from_str::<EndReason>(&json).unwrap(), r);
    }
    assert_eq!(
        EndReason::from_arena("ResultReason_Concede"),
        EndReason::Concede
    );
    assert!(matches!(EndReason::from_arena("X"), EndReason::Other(_)));
}

#[test]
fn last_turn_of_a_game_is_not_judged() {
    // The game ends in my turn 19 (opponent concedes) before I play a land.
    let mut t18 = turn(18, false);
    t18.life_me = 12;
    let mut t19 = turn(19, true);
    t19.hand = vec![FOREST];
    t19.land_played = false;
    t19.safe_idle = vec![BEAR];
    t19.life_me = 12;
    let mut g = game("ende", Some(true), vec![t18, t19.clone()]);
    g.reason = EndReason::Concede;
    let r = report_all(&[g.clone()], &std_db());
    assert!(r.categories.iter().all(|c| c.items.is_empty()));
    assert_eq!(r.my_turns, 1, "the turn still counts as played");

    // The same turn in the middle of the game is judged.
    g.turns.push(turn(20, false));
    let r = report_all(&[g], &std_db());
    let kinds: Vec<Mistake> = r
        .categories
        .iter()
        .filter(|c| !c.items.is_empty())
        .map(|c| c.kind)
        .collect();
    assert!(kinds.contains(&Mistake::MissedLand) && kinds.contains(&Mistake::SkippedAttack));
}

#[test]
fn legendary_already_in_play_is_not_suggested() {
    let db = std_db();
    // Isamaru is in play, a second one in hand: no "better play".
    let mut t = turn(3, true);
    t.hand = vec![LEGEND, LEGEND];
    t.lands = vec![PLAINS, PLAINS, PLAINS];
    t.lands_me = 3;
    t.board = vec![LEGEND];
    assert!(key_moment(&t, &db).is_none());
    // Without Isamaru in play it would be a suggestion (one, not both).
    t.board.clear();
    t.hand = vec![LEGEND, LEGEND, BEAR];
    t.lands = vec![PLAINS, PLAINS, FOREST];
    let moment = key_moment(&t, &db).unwrap();
    assert_eq!(moment.cards.iter().filter(|g| **g == LEGEND).count(), 1);
    let text = moment.text;
    assert_eq!(text.matches("Isamaru").count(), 1, "{text}");

    // "Held for long" doesn't count the second copy either.
    let mut held = turn(1, true);
    held.hand = vec![LEGEND];
    held.lands_me = 1;
    held.board = vec![LEGEND];
    let g = game(
        "legende",
        Some(false),
        vec![
            held.clone(),
            held.clone(),
            held.clone(),
            held,
            turn(5, false),
        ],
    );
    assert!(held_cards(&g, &db).is_empty());
}

#[test]
fn skipped_attacks_respect_blockers_and_tap_abilities() {
    let db = std_db();
    let mut t = turn(5, true);
    t.life_me = 10;
    // The elf has a tap ability, the bear doesn't.
    t.safe_idle = vec![ELF, BEAR];
    t.opp_creatures = vec![(OGRE, 3)];
    let mut g = game("angriff", Some(false), vec![t.clone(), turn(6, false)]);
    let skipped = |g: &GameRecord| {
        report_all(std::slice::from_ref(g), &db)
            .categories
            .into_iter()
            .find(|c| c.kind == Mistake::SkippedAttack)
            .map(|c| c.items)
            .unwrap_or_default()
    };
    let found = skipped(&g);
    assert_eq!(found.len(), 1);
    assert!(found[0].text.contains("Grizzly Bears") && !found[0].text.contains("Llanowar"));
    assert_eq!(found[0].cards, vec![BEAR], "cards for the images");
    // If lethal damage threatens, holding back is right.
    g.turns[0].opp_creatures = vec![(OGRE, 3), (DRAGON, 7)];
    assert!(skipped(&g).is_empty());
}

#[test]
fn game_modes_group_arena_events() {
    use GameMode::*;
    for (event, mode) in [
        ("Ladder", Ranked),
        ("Traditional_Ladder", Ranked),
        ("Historic_Ladder", Ranked),
        ("Play", Play),
        ("Traditional_Play", Play),
        ("Play_Brawl", Brawl),
        ("DemoBrawlDecks_TRK_20261110", Brawl),
        ("QuickDraft_FDN_20261101", Limited),
        ("Sealed_TRK_20261110", Limited),
        ("WelcomeDeckDuels_TRK_20261110", Event),
        ("DualColorPrecons", Event),
        ("Jump_In_2024", Event),
        ("", Event),
    ] {
        assert_eq!(GameMode::of(event), mode, "{event}");
    }
    assert!(GameMode::ALL.iter().all(|m| !m.label().is_empty()));
    let constructed: Vec<GameMode> = GameMode::ALL
        .into_iter()
        .filter(|m| m.is_constructed())
        .collect();
    assert_eq!(constructed, vec![Ranked, Play, Event]);
}

#[test]
fn games_filter_by_mode_and_deck() {
    let db = std_db();
    let with = |id: &str, event: &str, deck: Vec<GrpId>| GameRecord {
        id: id.into(),
        event: event.into(),
        deck,
        ..Default::default()
    };
    let red = vec![MOUNTAIN, OGRE, SHOCK];
    let brawl = with("b", "Play_Brawl", vec![FOREST, BEAR]);
    let ranked = with("r", "Ladder", red.clone());
    // Different order, same cards = same deck.
    let ranked_again = with("r2", "Traditional_Ladder", vec![SHOCK, MOUNTAIN, OGRE]);
    let no_deck = with("x", "Ladder", vec![]);

    let all = GamesFilter::default();
    assert!(
        [&brawl, &ranked, &no_deck]
            .iter()
            .all(|g| all.matches(g, &db))
    );

    let filter = GamesFilter::like(&ranked, &db);
    assert_eq!(filter.mode, Some(GameMode::Ranked));
    assert!(filter.matches(&ranked, &db) && filter.matches(&ranked_again, &db));
    assert!(!filter.matches(&brawl, &db), "Brawl doesn't mix in");
    assert!(
        !filter.matches(&no_deck, &db),
        "no deck match without a decklist"
    );

    let mode_only = GamesFilter {
        deck: None,
        ..filter
    };
    assert!(mode_only.matches(&no_deck, &db) && !mode_only.matches(&brawl, &db));
    assert_eq!(GamesFilter::like(&no_deck, &db).deck, None);
    assert_eq!(deck_key(&red, &db), deck_key(&ranked_again.deck, &db));
}
