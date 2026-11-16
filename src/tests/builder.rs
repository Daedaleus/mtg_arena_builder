use super::*;
use crate::testutil::card;

/// Sammlung mit genug roten und grünen Karten für je ein Deck.
fn collection() -> (CardDb, HashMap<String, u32>) {
    let mut cards = vec![
        card(1, "Plains").land("W").basic().build(),
        card(2, "Island").land("U").basic().build(),
        card(3, "Swamp").land("B").basic().build(),
        card(4, "Mountain").land("R").basic().build(),
        card(5, "Forest").land("G").basic().build(),
        card(6, "Rootbound Crag").land("RG").rarity(4).build(),
        card(7, "Cinder Glade").land("RG").rarity(3).build(),
        card(8, "Wrath").cost("{2}{R}{R}").sorcery().wipe().build(),
        card(9, "Back Face").cost("{1}{R}").creature(9, 9).build(),
        card(10, "Divination")
            .cost("{2}{U}")
            .sorcery()
            .draw()
            .build(),
    ];
    cards[8].back_face = true;
    let mut owned = HashMap::new();
    let mut grp = 100;
    for (color, letter) in [("Red", "R"), ("Green", "G")] {
        for cmc in 1..=6u32 {
            for n in 0..3 {
                let generic = if cmc > 1 {
                    format!("{{{}}}", cmc - 1)
                } else {
                    String::new()
                };
                let name = format!("{color} Creature {cmc}{n}");
                let mut c = card(grp, &name)
                    .cost(&format!("{generic}{{{letter}}}"))
                    .creature(cmc as i32 + 1, cmc as i32);
                if n == 0 {
                    c = c.rarity(4);
                }
                cards.push(c.build());
                owned.insert(name.to_lowercase(), 4);
                grp += 1;
            }
        }
        for n in 0..3 {
            let name = format!("{color} Removal {n}");
            cards.push(
                card(grp, &name)
                    .cost(&format!("{{{letter}}}"))
                    .instant()
                    .removal()
                    .build(),
            );
            owned.insert(name.to_lowercase(), 4);
            grp += 1;
        }
        for n in 0..3 {
            let name = format!("{color} Trick {n}");
            cards.push(
                card(grp, &name)
                    .cost(&format!("{{1}}{{{letter}}}"))
                    .instant()
                    .build(),
            );
            owned.insert(name.to_lowercase(), 4);
            grp += 1;
        }
    }
    for k in [
        "rootbound crag",
        "cinder glade",
        "wrath",
        "back face",
        "divination",
    ] {
        owned.insert(k.to_string(), 4);
    }
    (CardDb::from_cards(cards, "t"), owned)
}

fn count(b: &Built, name: &str) -> u32 {
    b.deck
        .main
        .iter()
        .filter(|e| e.name == name)
        .map(|e| e.qty)
        .sum()
}

#[test]
fn builds_decks_for_owned_colors() {
    let (db, owned) = collection();
    let built = build_all(&db, &owned, None, &HashMap::new());
    let labels: Vec<&str> = built.iter().map(|b| b.label.as_str()).collect();
    assert!(labels.contains(&"Mono-Rot"), "{labels:?}");
    assert!(labels.contains(&"Rot-Grün"), "{labels:?}");
    assert!(
        !labels.iter().any(|l| l.contains("Blau")),
        "zu wenig blaue Karten"
    );
    assert!(built.windows(2).all(|w| w[0].score >= w[1].score));

    for b in &built {
        let total: u32 = b.deck.main.iter().map(|e| e.qty).sum();
        assert_eq!(total, DECK_SIZE, "{}", b.label);
        assert!(b.creatures >= 12);
        // Höchstens 4 Exemplare je Karte (außer Standardländern).
        assert!(
            b.deck
                .main
                .iter()
                .all(|e| e.qty <= 4 || BASICS.contains(&e.name.as_str()))
        );
        // Flächenräumer und Rückseiten kommen nicht ins Deck.
        assert_eq!(count(b, "Wrath"), 0);
        assert_eq!(count(b, "Back Face"), 0);
        assert!(b.avg_cmc > 0.0);
    }
    let gruul = built.iter().find(|b| b.label == "Rot-Grün").unwrap();
    // Eigene Zweifarbländer, seltenere zuerst, dazu Standardländer.
    assert_eq!(count(gruul, "Rootbound Crag"), 4);
    assert_eq!(count(gruul, "Cinder Glade"), 4);
    assert_eq!(count(gruul, "Mountain") + count(gruul, "Forest"), 16);
    assert!(count(gruul, "Mountain") >= 5 && count(gruul, "Forest") >= 5);
    let red = built.iter().find(|b| b.label == "Mono-Rot").unwrap();
    assert_eq!(count(red, "Mountain"), LANDS);
    assert!(red.removal > 0);
    assert!(red.rares > 0);
}

#[test]
fn respects_legality_and_meta() {
    let (db, owned) = collection();
    // Nur grüne Karten legal: kein rotes Deck.
    let legal: HashSet<String> = owned
        .keys()
        .filter(|k| k.starts_with("green"))
        .cloned()
        .collect();
    let built = build_all(&db, &owned, Some(&legal), &HashMap::new());
    assert!(built.iter().all(|b| !b.label.contains("Rot")));
    assert!(built.iter().any(|b| b.label == "Mono-Grün"));
    // Turnierkarten werden bevorzugt und gezählt.
    let mut meta = HashMap::new();
    meta.insert("red trick 0".to_string(), 12);
    let built = build_all(&db, &owned, None, &meta);
    let red = built.iter().find(|b| b.label == "Mono-Rot").unwrap();
    assert_eq!(count(red, "Red Trick 0"), 4);
    assert_eq!(red.meta_cards, 4);
}

#[test]
fn too_small_collection_builds_nothing() {
    let (db, mut owned) = collection();
    owned.retain(|k, _| k.contains("removal"));
    assert!(build_all(&db, &owned, None, &HashMap::new()).is_empty());
}

#[test]
fn rating_rules() {
    let rg = 0b11000;
    let common = card(1, "C").cost("{2}{R}").creature(3, 3).build();
    let rare = card(2, "R").cost("{2}{R}").creature(3, 3).rarity(4).build();
    assert!(rating(&rare, 0, rg, 1) > rating(&common, 0, rg, 1));
    assert!(
        rating(&common, 5, rg, 1) > rating(&common, 0, rg, 1),
        "Turnierkarte"
    );
    assert!(
        rating(&common, 0, rg, 4) > rating(&common, 0, rg, 1),
        "mehr Exemplare"
    );
    let gold = card(3, "G").cost("{1}{R}{G}").creature(3, 3).build();
    let mono = card(4, "M").cost("{2}{R}").creature(3, 3).build();
    assert!(rating(&gold, 0, rg, 1) > rating(&mono, 0, rg, 1));
    let heavy = card(5, "H").cost("{R}{R}{R}").creature(3, 3).build();
    assert!(
        rating(&heavy, 0, rg, 1) < rating(&mono, 0, rg, 1),
        "viele Farbsymbole"
    );
    let big = card(6, "B").cost("{5}{R}").creature(6, 6).build();
    let fair = card(7, "F").cost("{2}{R}").creature(3, 3).build();
    assert!(rating(&big, 0, 1 << 3, 1) < rating(&fair, 0, 1 << 3, 1) + 1.0);
    assert_eq!(curve_bucket(0), 0);
    assert_eq!(curve_bucket(9), 5);
    assert_eq!(label_of(1), "Mono-Weiß");
    assert_eq!(label_of(0b00110), "Blau-Schwarz");
}
