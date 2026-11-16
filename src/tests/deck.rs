use super::*;
use crate::testutil::{self, card};

#[test]
fn parses_mtgo_and_arena_formats() {
    let d = parse("4 Dream Beavers\n3 Swamp\nSideboard\n2 Duress\n");
    assert_eq!((d.main.len(), d.side.len()), (2, 1));

    let d = parse(
        "Deck\n4 Watery Grave (EOE) 261\n2 Fire // Ice (MH2) 290\n\nSideboard\n1 Ghost Vacuum (DSK) 248\n",
    );
    assert_eq!(d.main[0].name, "Watery Grave");
    assert_eq!(d.main[1].name, "Fire // Ice");
    assert_eq!(d.side[0].name, "Ghost Vacuum");

    let d = parse("4 Island\n\n2 Negate\n");
    assert_eq!((d.main.len(), d.side.len()), (1, 1));
}

#[test]
fn parse_edge_cases() {
    let d = parse(
        "Commander\n1 Atraxa\nDeck:\n4x Shock\n2X Opt\n0 Nothing\nabc\nfour Islands\n\
         3 Card (with note)\n1 Thing (TOOLONGSET) 1\n\nCompanion\n1 Lurrus\n",
    );
    let main: Vec<(u32, &str)> = d.main.iter().map(|e| (e.qty, e.name.as_str())).collect();
    assert_eq!(
        main,
        vec![
            (1, "Atraxa"),
            (4, "Shock"),
            (2, "Opt"),
            // Klammern ohne Set-Kürzel gehören zum Namen.
            (3, "Card (with note)"),
            (1, "Thing (TOOLONGSET) 1"),
        ]
    );
    assert_eq!(d.side.len(), 1);
    assert_eq!(d.side[0].name, "Lurrus");
    // „Deck“ nach einer Leerzeile beginnt wieder das Hauptdeck.
    let d = parse("Deck\n4 Shock\n\nDeck\n2 Opt\n");
    assert_eq!((d.main.len(), d.side.len()), (2, 0));
    assert!(parse("").main.is_empty());
}

fn owned(pairs: &[(&str, u32)]) -> HashMap<String, u32> {
    pairs.iter().map(|(k, n)| (k.to_string(), *n)).collect()
}

#[test]
fn evaluates_against_collection() {
    let db = testutil::std_db();
    let deck =
        parse("4 Shock\n2 Shock\n20 Mountain\n4 Hill Ogre\n2 Unknown Card\nSideboard\n3 Murder\n");
    let eval = evaluate(&deck, &db, &owned(&[("shock", 5), ("hill ogre", 4)]));
    let l = &eval.lines;
    // Zwei Zeilen derselben Karte teilen sich den Besitz.
    assert_eq!((l[0].owned, l[0].missing), (5, 0));
    assert_eq!((l[1].owned, l[1].missing), (5, 1));
    assert!(l[2].basic);
    assert_eq!(l[2].missing, 0);
    assert_eq!(l[3].missing, 0);
    assert_eq!(l[4].key, None);
    assert_eq!(l[4].missing, 2);
    assert!(l[5].side);
    assert_eq!(l[5].rarity, 2);

    let s = eval.summary(true, |_| 0);
    assert_eq!(s.total, 35);
    assert_eq!(s.missing, 1 + 2 + 3);
    assert_eq!(s.need, [4, 0, 0, 0]);
    assert_eq!(s.unavailable, 2);
    assert!(!s.complete());
    // Karten, die es in Arena nicht gibt, lassen sich nicht herstellen.
    assert!(!s.craftable([99, 99, 99, 99]));

    let main_only = eval.summary(false, |_| 0);
    assert_eq!(main_only.total, 32);
    assert_eq!(main_only.need, [1, 0, 0, 0]);
    let without_unknown = eval.summary(true, |l| if l.key.is_none() { l.missing } else { 0 });
    assert!(without_unknown.craftable([4, 0, 0, 0]));
    assert!(!without_unknown.craftable([3, 0, 0, 0]));

    // Ersatzkarten decken fehlende Exemplare ab.
    let replaced = eval.summary(true, |l| l.missing);
    assert_eq!(replaced.missing, 0);
    assert!(replaced.complete());
    assert_eq!(replaced.percent(), 100.0);
}

#[test]
fn summary_counts_rarities() {
    let db = testutil::std_db();
    let deck = parse("2 Shivan Dragon\n3 Serra Angel\n4 Stone Giant\n");
    let s = evaluate(&deck, &db, &HashMap::new()).summary(true, |_| 0);
    assert_eq!(s.need, [0, 4, 3, 2]);
    assert_eq!(s.percent(), 0.0);
    assert_eq!(Summary::default().percent(), 0.0);
    assert!(Summary::default().complete());
    let half = Summary {
        total: 10,
        missing: 5,
        ..Default::default()
    };
    assert_eq!(half.percent(), 50.0);
}

#[test]
fn exports_for_arena() {
    let db = crate::carddb::CardDb::from_cards(
        vec![
            card(1, "Shock").cost("{R}").set("M21", "159").build(),
            card(2, "Shock").cost("{R}").set("SPG", "7").build(),
            card(3, "Mountain").land("R").basic().set("", "").build(),
            card(4, "Lava Spike").cost("{R}").set("CHK", "1").build(),
            card(5, "Duress").cost("{B}").set("M21", "96").build(),
        ],
        "t",
    );
    let deck = parse("4 Shock\n20 Mountain\n2 Missing Card\nSideboard\n2 Duress\n");
    let eval = evaluate(&deck, &db, &owned(&[("shock", 2)]));
    let mut coll = HashMap::new();
    coll.insert(1u32, 2u32);

    // Mit Set-Kürzeln: bevorzugt den eigenen Druck; Ersatz für fehlende Shock.
    let text = export_arena(&eval, &db, Some(&coll), true, true, |l| {
        (l.name == "Shock").then(|| ("Lava Spike".to_string(), 5))
    });
    assert_eq!(
        text,
        "Deck\n2 Shock (M21) 159\n2 Lava Spike (CHK) 1\n20 Mountain\n2 Missing Card\n\nSideboard\n2 Duress (M21) 96\n"
    );

    // Ohne Set-Kürzel und ohne Sideboard; ohne Sammlung gilt der neueste Druck.
    let text = export_arena(&eval, &db, None, false, false, |_| None);
    assert_eq!(text, "Deck\n4 Shock\n20 Mountain\n2 Missing Card\n");
}
