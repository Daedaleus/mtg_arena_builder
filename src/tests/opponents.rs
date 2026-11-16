use super::*;
use crate::deck::parse;
use crate::testutil::*;

fn opp_game(won: Option<bool>, cards: &[u32], turns: u32) -> GameRecord {
    GameRecord {
        won,
        opp_cards: cards.iter().map(|g| (*g, 1)).collect(),
        turns: (1..=turns)
            .map(|t| crate::games::TurnStat {
                turn: t,
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }
}

#[test]
fn color_labels() {
    assert_eq!(color_label(""), "Unbekannt");
    assert_eq!(color_label("R"), "Mono-Rot");
    assert_eq!(color_label("WUBG"), "Weiß-Blau-Schwarz-Grün");
    assert_eq!(color_label("X"), "Mono-?");
}

#[test]
fn shares_and_matching() {
    let db = std_db();
    let gruul =
        parse("4 Grizzly Bears\n4 Hill Ogre\n4 Llanowar Elves\nSideboard\n2 Giant Growth\n");
    let gruul2 = parse("4 Grizzly Bears\n4 Stone Giant\n");
    let control = parse("4 Cancel\n4 Divination\n4 Murder\n");
    let meta = vec![
        MetaArchetype {
            id: 1,
            name: "Gruul".into(),
            lists: vec![&gruul, &gruul2],
        },
        MetaArchetype {
            id: 2,
            name: "Control".into(),
            lists: vec![&control],
        },
        MetaArchetype {
            id: 3,
            name: "Ohne Listen".into(),
            lists: vec![],
        },
    ];
    let shares = arch_shares(&meta, &db);
    assert_eq!(shares.len(), 2);
    assert_eq!(shares[0].share["grizzly bears"], 1.0);
    assert_eq!(shares[0].share["hill ogre"], 0.5);
    assert_eq!(shares[0].share["giant growth"], 0.5, "Nebendeck zählt mit");

    let seen = seen_keys(&[BEAR, OGRE, FOREST], &db);
    assert_eq!(seen.len(), 2, "Länder zählen nicht");
    assert_eq!(match_archetype(&seen, &shares).unwrap().name, "Gruul");
    // Eine einzelne passende Karte reicht nicht.
    let one = seen_keys(&[BEAR], &db);
    assert!(match_archetype(&one, &shares).is_none());
    // Drei Treffer reichen auch bei vielen anderen Karten.
    let many = seen_keys(
        &[COUNTER, DIVINATION, MURDER, SHOCK, SPIKE, DRAGON, ANGEL],
        &db,
    );
    assert_eq!(match_archetype(&many, &shares).unwrap().name, "Control");
    assert!(match_archetype(&seen, &[]).is_none());

    assert_eq!(
        classify(&opp_game(Some(true), &[BEAR, OGRE], 5), &db, &shares),
        "Gruul (erkannt)"
    );
    assert_eq!(
        classify(&opp_game(Some(true), &[SPIRIT], 5), &db, &shares),
        "Mono-Blau"
    );
}

#[test]
fn groups_by_opponent() {
    let db = std_db();
    let mut lost_to_red = opp_game(Some(false), &[SHOCK, OGRE], 6);
    lost_to_red.damage_taken.insert(OGRE, 9);
    lost_to_red.damage_taken.insert(SHOCK, 4);
    lost_to_red.damage_taken.insert(9999, 1);
    let mut lost_again = opp_game(Some(false), &[SPIKE], 4);
    lost_again.damage_taken.insert(SPIKE, 3);
    let won_vs_blue = opp_game(Some(true), &[SPIRIT], 8);
    let unfinished = opp_game(None, &[SPIRIT], 2);
    let games = vec![won_vs_blue, lost_to_red, unfinished, lost_again];
    let groups = analyze(&games, &db, &[]);
    assert_eq!(groups.len(), 2);
    let red = &groups[0];
    assert_eq!(red.name, "Mono-Rot");
    assert_eq!(red.games, vec![1, 3]);
    assert_eq!((red.wins, red.losses()), (0, 2));
    assert_eq!(red.avg_turns, 5.0);
    assert_eq!(
        red.threats,
        vec![
            ("Hill Ogre".to_string(), 9),
            ("Shock".to_string(), 4),
            ("Lava Spike".to_string(), 3)
        ]
    );
    assert_eq!(groups[1].name, "Mono-Blau");
    assert_eq!(groups[1].losses(), 0);
}
