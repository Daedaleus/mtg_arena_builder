use super::*;
use crate::testutil::card;

fn db() -> CardDb {
    CardDb::from_cards(
        vec![
            card(1, "Ogre").cost("{2}{R}").creature(3, 3).build(),
            card(2, "Brute").cost("{2}{R}").creature(3, 3).build(),
            card(3, "Goblin").cost("{1}{R}").creature(2, 1).build(),
            card(4, "Bolt").cost("{R}").instant().removal().build(),
            card(5, "Elf").cost("{1}{G}").creature(1, 1).build(),
            card(6, "Mountain").land("R").basic().build(),
            card(7, "Dragon").cost("{4}{R}{R}").creature(5, 5).build(),
            card(8, "Illegal Ogre")
                .cost("{2}{R}")
                .creature(3, 3)
                .build(),
            card(9, "Deck Ogre").cost("{2}{R}").creature(3, 3).build(),
            card(10, "Murder")
                .cost("{1}{B}{B}")
                .instant()
                .removal()
                .build(),
            card(11, "Doom Blade")
                .cost("{1}{B}")
                .instant()
                .removal()
                .build(),
            card(12, "Pump").cost("{1}{B}").instant().build(),
            card(13, "Gate").land("BR").build(),
            card(14, "Crypt").land("BR").rarity(4).build(),
        ],
        "t",
    )
}

fn owned(keys: &[&str]) -> HashMap<String, u32> {
    keys.iter().map(|k| (k.to_string(), 2)).collect()
}

#[test]
fn suggests_similar_cards_in_colors() {
    let db = db();
    let all = owned(&[
        "brute",
        "goblin",
        "bolt",
        "elf",
        "mountain",
        "dragon",
        "illegal ogre",
        "deck ogre",
    ]);
    let legal: HashSet<String> = [
        "brute",
        "goblin",
        "bolt",
        "elf",
        "mountain",
        "dragon",
        "deck ogre",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let in_deck: HashSet<String> = ["ogre".to_string(), "deck ogre".to_string()].into();
    let red = 1 << 3;
    let s = suggest(&db, &all, Some(&legal), red, &in_deck, "ogre", 5);
    let names: Vec<&str> = s.iter().map(|x| x.name.as_str()).collect();
    // Same stats and costs first; other card types, colors, basic lands, too
    // expensive, illegal and already played cards drop out.
    assert_eq!(names, vec!["Brute", "Goblin"]);
    assert_eq!(s[0].owned, 2);
    assert_eq!(s[0].info, "{2}{R}");
    assert!(s[0].score > s[1].score);
    // Without a legality list the illegal card is allowed too.
    let s = suggest(&db, &all, None, red, &in_deck, "ogre", 1);
    assert_eq!(s.len(), 1);
    assert!(suggest(&db, &all, None, red, &in_deck, "unbekannt", 5).is_empty());
}

#[test]
fn removal_is_replaced_by_removal() {
    let db = db();
    let black = 1 << 2;
    let s = suggest(
        &db,
        &owned(&["doom blade", "pump"]),
        None,
        black,
        &HashSet::new(),
        "murder",
        5,
    );
    assert_eq!(s[0].name, "Doom Blade");
    assert_eq!(s[1].name, "Pump");
}

#[test]
fn lands_are_replaced_by_lands() {
    let db = db();
    let s = suggest(
        &db,
        &owned(&["gate", "mountain", "brute"]),
        None,
        0b01100,
        &HashSet::new(),
        "crypt",
        5,
    );
    let names: Vec<&str> = s.iter().map(|x| x.name.as_str()).collect();
    assert_eq!(names, vec!["Gate"]);
    assert_eq!(s[0].info, "", "lands have no mana costs");
}

#[test]
fn similarity_measure() {
    assert_eq!(jaccard::<u32>(&[], &[]), 0.0);
    assert_eq!(jaccard(&[1, 2], &[2, 3]), 1.0 / 3.0);
    assert_eq!(jaccard(&[1], &[1]), 1.0);
}

#[test]
fn suggestion_for_a_chosen_card() {
    let db = crate::testutil::std_db();
    let owned: HashMap<String, u32> = [("hill ogre".to_string(), 3)].into();
    let s = Suggestion::for_card(&db, &owned, "hill ogre").unwrap();
    assert_eq!(
        (s.name.as_str(), s.owned, s.info.as_str()),
        ("Hill Ogre", 3, "{2}{R}")
    );
    let basic = Suggestion::for_card(&db, &owned, "mountain").unwrap();
    assert_eq!((basic.owned, basic.info.as_str()), (0, "Standardland"));
    assert!(Suggestion::for_card(&db, &owned, "gibt es nicht").is_none());
}
