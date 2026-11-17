use super::*;
use crate::deck::parse;
use crate::games::{Category, Finding, TurnStat};
use crate::testutil::*;

fn ids(parts: &[(u32, usize)]) -> Vec<u32> {
    parts
        .iter()
        .flat_map(|(g, n)| std::iter::repeat_n(*g, *n))
        .collect()
}

fn aggro() -> Vec<u32> {
    ids(&[(MOUNTAIN, 24), (ELF, 20), (BEAR, 16)])
}
fn control() -> Vec<u32> {
    ids(&[
        (ISLAND, 26),
        (COUNTER, 12),
        (MURDER, 10),
        (DIVINATION, 8),
        (GIANT, 4),
    ])
}
fn midrange() -> Vec<u32> {
    ids(&[(FOREST, 24), (OGRE, 16), (GIANT, 12), (SHOCK, 8)])
}

#[test]
fn deck_types() {
    let db = std_db();
    let a = DeckProfile::of_ids(&aggro(), &db);
    assert_eq!(a.style(), Style::Aggro);
    assert!((a.avg_cmc - (20.0 + 32.0) / 36.0).abs() < 0.01);
    assert_eq!(a.creatures, 1.0);
    assert_eq!(DeckProfile::of_ids(&control(), &db).style(), Style::Control);
    assert_eq!(
        DeckProfile::of_ids(&midrange(), &db).style(),
        Style::Midrange
    );
    // Nur Länder: neutrales Profil.
    assert_eq!(
        DeckProfile::of_ids(&[FOREST; 5], &db),
        DeckProfile::default()
    );
    // Aus einer Textliste
    let d = parse("20 Forest\n20 Llanowar Elves\n20 Grizzly Bears\n");
    assert_eq!(DeckProfile::of_deck(&d, &db).style(), Style::Aggro);
    for s in STYLES {
        assert!(!s.name().is_empty() && !s.description().is_empty());
    }
}

fn game(deck: Vec<u32>, won: bool) -> GameRecord {
    GameRecord {
        won: Some(won),
        deck,
        complete: true,
        turns: (1..=8)
            .map(|t| TurnStat {
                turn: t,
                mine: t % 2 == 1,
                attacked: t >= 5,
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }
}

fn report(my_turns: u32, mistakes: &[(&'static str, usize)]) -> Report {
    Report {
        my_turns,
        avg_turns: 8.0,
        categories: mistakes
            .iter()
            .map(|(title, n)| Category {
                title,
                advice: "",
                items: (0..*n)
                    .map(|_| Finding {
                        game: 1,
                        turn: 3,
                        text: String::new(),
                    })
                    .collect(),
            })
            .collect(),
        ..Default::default()
    }
}

#[test]
fn recommends_what_wins() {
    let db = std_db();
    let mut games: Vec<GameRecord> = (0..3).map(|_| game(control(), false)).collect();
    games.extend((0..4).map(|_| game(control(), true)));
    games.extend((0..3).map(|_| game(aggro(), true)));
    let s = analyze(&games, &db, &report(40, &[]));
    assert_eq!(s.games, 10);
    assert_eq!(s.records[0].style, Style::Control, "meistgespielt zuerst");
    assert_eq!((s.records[0].games, s.records[0].wins), (7, 4));
    assert_eq!(s.recommended, Style::Aggro);
    assert!(s.reason.contains("3 von 3"), "{}", s.reason);
    // Angriffe in Zug 5 und 7 von 3, 5, 7
    assert!((s.attack_rate - 2.0 / 3.0).abs() < 0.01);
    assert!(s.traits[0].contains("67 %"), "{:?}", s.traits);
    assert!(s.traits[1].contains("eher kurze Partien"));
}

#[test]
fn recommends_aggro_for_passive_players() {
    let db = std_db();
    let games: Vec<GameRecord> = (0..2).map(|_| game(control(), true)).collect();
    let s = analyze(
        &games,
        &db,
        &report(
            20,
            &[
                ("Mana ungenutzt", 3),
                ("Gefahrlosen Angriff ausgelassen", 4),
            ],
        ),
    );
    assert_eq!(s.recommended, Style::Aggro);
    assert!(s.reason.starts_with("Aggro-Decks sind am einfachsten"));
    assert!(s.traits.iter().any(|t| t.contains("Mana ungenutzt")));
    assert!(s.traits.iter().any(|t| t.contains("vorsichtig")));
}

#[test]
fn falls_back_to_favourite_or_default() {
    let db = std_db();
    let games = vec![game(midrange(), false)];
    let s = analyze(&games, &db, &report(10, &[]));
    assert_eq!(s.recommended, Style::Midrange);
    assert!(s.reason.contains("am liebsten Midrange"));
    let empty = analyze(&[], &db, &Report::default());
    assert_eq!(empty.recommended, Style::Aggro);
    assert!(empty.traits.is_empty());
    assert_eq!(empty.attack_rate, 0.0);
    assert_eq!(
        StyleRecord {
            style: Style::Aggro,
            games: 0,
            wins: 0
        }
        .rate(),
        0.0
    );
}

#[test]
fn recommends_cheapest_decks_of_style() {
    use crate::deck::Summary;
    let cand = |id, label: &str, style, rare| Candidate {
        deck_id: id,
        label: label.into(),
        source: "Turnierdecks",
        style,
        summary: Summary {
            total: 60,
            missing: rare,
            need: [0, 0, rare, 0],
            unavailable: 0,
        },
    };
    let cands = vec![
        cand(1, "Mono-Rot", Style::Aggro, 8),
        cand(2, "Mono-Rot", Style::Aggro, 2),
        cand(3, "Boros", Style::Aggro, 5),
        cand(4, "Dimir", Style::Control, 0),
        cand(5, "Gruul", Style::Aggro, 12),
    ];
    let r = recommend(&cands, Style::Aggro, [0, 0, 4, 0], 2);
    let ids: Vec<u32> = r.iter().map(|c| c.deck_id).collect();
    assert_eq!(
        ids,
        vec![2, 3],
        "günstigste zuerst, je Archetyp einmal, höchstens 2"
    );
    assert!(recommend(&cands, Style::Midrange, [0; 4], 5).is_empty());
}
