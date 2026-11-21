use super::*;
use crate::testutil::{self, TempDir, card};

#[test]
fn burn_from_text() {
    assert_eq!(burn_of("Kicker {o4}"), 0);
    assert_eq!(
        burn_of(
            "CARDNAME deals 2 damage to any target. If this spell was kicked, it deals 4 damage instead."
        ),
        2
    );
    assert_eq!(
        burn_of("CARDNAME deals 3 damage to any target. Tap up to one target creature."),
        3
    );
    assert_eq!(
        burn_of("CARDNAME deals 3 damage to up to one target creature or planeswalker."),
        0
    );
    assert_eq!(burn_of("Lightning deals 2 damage to each opponent."), 2);
    assert_eq!(
        burn_of("<i>Flavor</i> CARDNAME deals 5 damage to target player."),
        5
    );
    assert_eq!(burn_of("CARDNAME deals X damage to any target."), 0);
}

#[test]
fn roles_from_text() {
    assert_eq!(roles_of("Destroy target creature."), ROLE_REMOVAL);
    assert_eq!(roles_of("Destroy target land."), 0);
    assert_eq!(
        roles_of("Exile target creature an opponent controls."),
        ROLE_REMOVAL
    );
    assert_eq!(roles_of("Exile target card from a graveyard."), 0);
    assert_eq!(
        roles_of("Target creature gets -3/-3 until end of turn."),
        ROLE_REMOVAL
    );
    assert_eq!(roles_of("Destroy all creatures."), ROLE_REMOVAL | ROLE_WIPE);
    assert_eq!(roles_of("Counter target spell."), ROLE_COUNTER);
    assert_eq!(roles_of("Draw two cards."), ROLE_DRAW);
    assert_eq!(
        roles_of("Counter target spell. Draw a card."),
        ROLE_COUNTER | ROLE_DRAW
    );
    assert_eq!(roles_of("Target creature gets +3/+3."), 0);
    assert_eq!(roles_of("<b>Destroy target creature.</b>"), ROLE_REMOVAL);
}

#[test]
fn enters_tapped_text() {
    assert!(enters_tapped_of("CARDNAME enters tapped."));
    assert!(!enters_tapped_of(
        "CARDNAME enters tapped unless you control two or more other lands."
    ));
    assert!(!enters_tapped_of("{oT}: Add {oG}."));
}

#[test]
fn mana_text() {
    assert_eq!(cmc_of("o2oUoB"), 4);
    assert_eq!(cmc_of("oXoR"), 1);
    assert_eq!(cmc_of("o1o(R/G)"), 2);
    assert_eq!(cmc_of(""), 0);
    assert_eq!(pretty_mana("o2oUoB"), "{2}{U}{B}");
    assert_eq!(pretty_mana("o1o(R/G)"), "{1}{R/G}");
    assert_eq!(parse_list::<u32>("1:2, 3, x,4:9"), vec![1, 3, 4]);
    assert_eq!(strip_tags("<i>a</i>b<x"), "ab");
}

#[test]
fn pips_and_types() {
    let c = card(1, "X")
        .cost("{2}{U}{U}{W/U}{X}")
        .creature(1, 1)
        .build();
    assert_eq!(c.pips(), (3, [0, 2, 0, 0, 0]));
    assert!(c.is_creature() && !c.is_instant() && !c.is_land());
    let db = testutil::std_db();
    let s = &db.cards[&testutil::SHOCK];
    assert!(s.is_instant() && s.has_role(ROLE_REMOVAL) && !s.has_role(ROLE_DRAW));
    assert_eq!(Rarity::from_arena(0).label(), "Land");
    assert_eq!(Rarity::from_arena(1).label(), "Land");
    assert_eq!(Rarity::from_arena(2).label(), "Common");
    assert_eq!(Rarity::from_arena(3).label(), "Uncommon");
    assert_eq!(Rarity::from_arena(4).label(), "Rare");
    assert_eq!(Rarity::from_arena(5).label(), "Mythic");
    assert_eq!(
        Rarity::from_arena(9).label(),
        "Land",
        "unknown like a token"
    );
}

#[test]
fn names_are_normalized() {
    assert_eq!(name_key("  Lightning   Bolt "), "lightning bolt");
    assert_eq!(name_key("Urza\u{2019}s Saga"), "urza's saga");
    // Without spaces the slash (e.g. in card names) is kept.
    assert_eq!(name_key("Fire/Ice"), "fire/ice");
    assert_eq!(name_key("Fire//Ice"), "fire // ice");
    assert_eq!(name_key("Fire / Ice"), "fire // ice");
    assert_eq!(name_key("Fire // Ice"), "fire // ice");
}

#[test]
fn lookup_and_printings() {
    let mut special = card(13, "Opt").cost("{U}").rarity(3).build();
    special.regular = false;
    let mut back = card(14, "Back").build();
    back.back_face = true;
    let db = CardDb::from_cards(
        vec![
            card(10, "Fire // Ice").cost("{1}{R}").rarity(3).build(),
            card(11, "Opt").cost("{U}").set("OLD", "1").build(),
            card(12, "Opt").cost("{U}").set("NEW", "2").build(),
            special,
            back,
        ],
        "1.0",
    );
    assert_eq!(db.version, "1.0");
    assert_eq!(db.resolve("Fire // Ice").as_deref(), Some("fire // ice"));
    assert_eq!(db.resolve("Fire").as_deref(), Some("fire // ice"));
    assert_eq!(db.resolve("OPT").as_deref(), Some("opt"));
    assert_eq!(db.resolve("Opt // Something").as_deref(), Some("opt"));
    assert_eq!(db.resolve("Unbekannt"), None);
    assert_eq!(db.printings("opt"), &[GrpId(11), GrpId(12), GrpId(13)]);
    assert!(db.printings("nix").is_empty());
    // The newest regular printing is preferred.
    assert_eq!(db.representative("opt").unwrap().set, "NEW");
    assert_eq!(db.representative("back").unwrap().grp_id, GrpId(14));
    assert!(db.representative("nix").is_none());
    // Cheapest rarity over all printings.
    assert_eq!(db.craft_rarity("opt"), Rarity::Common);
    assert_eq!(db.craft_rarity("nix"), Rarity::Token);
    assert_eq!(db.names().count(), 3);
}

#[test]
fn abilities_set_roles_burn_tapped_and_mana() {
    let mut info = AbilityInfo::default();
    info.add(100, "CARDNAME deals 3 damage to any target.");
    info.add(101, "Draw a card.");
    info.add(200, "CARDNAME enters tapped.");
    info.add(300, "{oT}: Add {oG}.");
    info.add(301, "{oT}: Add {oC}.");
    let mut spell = card(1, "Bolt").instant().abilities(&[100, 101]).build();
    apply_abilities(&mut spell, &info);
    assert_eq!(spell.roles, ROLE_REMOVAL | ROLE_DRAW);
    assert_eq!(spell.burn, 3);
    assert_eq!(spell.mana, None);
    // Creatures get no burn damage, but their mana ability.
    let mut guy = card(2, "Guy")
        .creature(1, 1)
        .abilities(&[100, 300, 301])
        .build();
    apply_abilities(&mut guy, &info);
    assert_eq!(guy.burn, 0);
    assert_eq!(guy.mana, Some(1 << 4));
    let mut land = card(3, "Gate").land("UG").abilities(&[200, 300]).build();
    apply_abilities(&mut land, &info);
    assert!(land.enters_tapped);
    assert_eq!(land.mana, None, "lands provide colors via colors");
}

#[test]
fn mana_abilities_from_text() {
    assert_eq!(mana_of("{oT}: Add {oG}."), Some(1 << 4));
    assert_eq!(
        mana_of("{oT}: Add {oB} or {oG}."),
        Some((1 << 2) | (1 << 4))
    );
    assert_eq!(mana_of("{oT}: Add one mana of any color."), Some(0x1F));
    assert_eq!(mana_of("{oT}: Add {oC}."), Some(0));
    assert_eq!(mana_of("{o2}, {oT}: Draw a card."), None);
    assert_eq!(mana_of("Flying"), None);
}

/// Creates a small database in Arena's format.
fn arena_db(dir: &TempDir, with_de: bool) -> std::path::PathBuf {
    let path = dir.path("Raw_CardDatabase_test.mtga");
    let conn = Connection::open(&path).unwrap();
    let mut sql = String::from(
        "CREATE TABLE Versions (Type TEXT, Version TEXT);
         INSERT INTO Versions VALUES ('Data', '2026.11');
         CREATE TABLE Localizations_enUS (LocId INTEGER, Formatted INTEGER, Loc TEXT);
         CREATE TABLE Abilities (Id INTEGER, TextId INTEGER);
         CREATE TABLE Cards (GrpId INTEGER, TitleId INTEGER, Rarity INTEGER, ExpansionCode TEXT,
            CollectorNumber TEXT, IsPrimaryCard INTEGER, OldSchoolManaText TEXT, Types TEXT,
            Subtypes TEXT, Supertypes TEXT, ColorIdentity TEXT, LinkedFaceType INTEGER,
            AbilityIds TEXT, Power TEXT, Toughness TEXT, DigitalReleaseSet TEXT, IsToken INTEGER);
         INSERT INTO Localizations_enUS VALUES
            (1, 1, 'Shock'), (2, 1, 'Hedge Maze'), (3, 1, '<i>Forest</i>'), (4, 1, 'Token'),
            (5, 1, ''), (6, 1, 'Grizzly Bears'),
            (50, 1, 'CARDNAME deals 2 damage to any target.'),
            (51, 1, 'CARDNAME enters tapped.'),
            (52, 1, '{oT}: Add {oG}.'),
            (53, 1, '{oT}, Sacrifice CARDNAME: Add one mana of any color.');
         INSERT INTO Abilities VALUES (500, 50), (501, 51), (502, 52), (503, 53);
         INSERT INTO Cards VALUES
            (100, 1, 2, 'M21', '159', 1, 'oR', '4', '', '', '4', 0, '500:50', '', '', NULL, 0),
            (101, 2, 3, 'MKM', '262', 1, '', '5', '10,11', '', '2,5', 0, '501:51', '', '', NULL, 0),
            (102, 3, 1, 'FDN', '280', 1, '', '5', '11', '1', '5', 0, '', '', '', NULL, 0),
            (103, 4, 0, 'TKN', '1', 1, '', '1', '', '', '', 0, '503:53', '', '', NULL, 1),
            (104, 5, 2, 'X', '1', 1, 'o1', '2', '', '', '', 0, '', '1', '1', NULL, 0),
            (105, 6, 2, 'Y22', '7', 1, 'o1oG', '2', '', '2', '5', 1, '502:52', '2', '2', 'Y22', 0);",
    );
    if with_de {
        sql.push_str(
            "CREATE TABLE Localizations_deDE (LocId INTEGER, Formatted INTEGER, Loc TEXT);
             INSERT INTO Localizations_deDE VALUES (1, 1, 'Schock'), (2, 1, '#NV');",
        );
    }
    conn.execute_batch(&sql).unwrap();
    path
}

#[test]
fn loads_arena_database() {
    let dir = TempDir::new("carddb");
    let db = CardDb::load(&arena_db(&dir, true)).unwrap();
    assert_eq!(db.version, "2026.11");
    // Cards without names are missing; tokens are loaded, but don't count as
    // cards and can't be found by name (many share a name with a real card).
    assert_eq!((db.cards.len(), db.card_count()), (5, 4));
    let token = &db.cards[&GrpId(103)];
    assert!(token.token);
    assert_eq!(db.resolve("Token"), None);
    // Treasure: "{oT}, Sacrifice CARDNAME: Add one mana of any color."
    assert_eq!(token.mana, Some(0x1F));
    let shock = &db.cards[&GrpId(100)];
    assert_eq!(shock.name, "Shock");
    assert_eq!(shock.name_de.as_deref(), Some("Schock"));
    assert_eq!((shock.cmc, shock.mana_cost.as_str()), (1, "{R}"));
    assert_eq!(shock.colors, 1 << 3);
    assert!(shock.has_role(ROLE_REMOVAL));
    assert_eq!(shock.burn, 2);
    let maze = &db.cards[&GrpId(101)];
    assert_eq!(maze.name_de, None, "placeholders are ignored");
    assert!(maze.is_land() && maze.enters_tapped);
    assert_eq!(maze.colors, 0b10010);
    assert_eq!(maze.subtypes, vec![10, 11]);
    let forest = &db.cards[&GrpId(102)];
    assert_eq!(forest.name, "Forest");
    assert!(forest.basic);
    let bears = &db.cards[&GrpId(105)];
    assert!(bears.back_face);
    assert!(!bears.regular);
    assert_eq!((bears.power.as_str(), bears.toughness.as_str()), ("2", "2"));
    // Supertype 2 = legendary; "{oT}: …" is a tap ability.
    assert!(bears.legendary && bears.tap_ability);
    assert!(!shock.legendary && !shock.tap_ability && !forest.legendary);
}

#[test]
fn mana_abilities_with_tap_or_sacrifice() {
    assert_eq!(text::mana_of("{oT}: Add {oG}."), Some(1 << 4));
    // Treasure and Gold: sacrifice (with or without tapping), any color.
    assert_eq!(
        text::mana_of("{oT}, Sacrifice CARDNAME: Add one mana of any color."),
        Some(0x1F)
    );
    assert_eq!(
        text::mana_of("Sacrifice CARDNAME: Add one mana of any color."),
        Some(0x1F)
    );
    // Eldrazi Spawn: colorless.
    assert_eq!(text::mana_of("Sacrifice CARDNAME: Add {oC}."), Some(0));
    // Costs with mana (filter lands) or other permanents don't count.
    assert_eq!(text::mana_of("{o1}, {oT}: Add {oW}{oU}."), None);
    assert_eq!(text::mana_of("Sacrifice a creature: Add {oB}."), None);
    assert_eq!(text::mana_of("Flying"), None);
}

#[test]
fn tap_costs_come_from_the_cost_part() {
    assert!(text::tap_cost_of("{oT}: Add {oG}."));
    assert!(text::tap_cost_of("{oT}, Sacrifice CARDNAME: Draw a card."));
    assert!(!text::tap_cost_of("Flying"));
    assert!(!text::tap_cost_of(
        "Whenever you attack, tap target creature. {oT} untaps"
    ));
    assert!(!text::tap_cost_of(
        "{o2}: CARDNAME gets +1/+1 until end of turn. Then {oT} it"
    ));
}

#[test]
fn loads_without_german_and_reports_errors() {
    let dir = TempDir::new("carddb");
    let db = CardDb::load(&arena_db(&dir, false)).unwrap();
    assert_eq!(db.cards[&GrpId(100)].name_de, None);
    assert!(CardDb::load(&dir.path("fehlt.mtga")).is_err());
}

#[test]
fn color_names() {
    use super::color::{label, letters};
    assert_eq!(letters(0b01010), "UR");
    assert_eq!(letters(0), "");
    assert_eq!(label(""), "Unbekannt");
    assert_eq!(label("R"), "Mono-Rot");
    assert_eq!(label("WUBG"), "Weiß-Blau-Schwarz-Grün");
    assert_eq!(label("X"), "Mono-?");
}

#[test]
fn display_names_follow_language() {
    let mut cards = crate::testutil::std_cards();
    cards[0].name_de = Some("Ebene".into());
    let db = CardDb::from_cards(cards, "t");
    let plains = db.representative("plains").unwrap();
    // Tests run in German.
    assert_eq!(plains.display_name(), "Ebene");
    assert_eq!(db.display_name("Plains"), "Ebene");
    assert_eq!(
        db.display_name("Island"),
        "Island",
        "without a German name English"
    );
    assert_eq!(db.display_name("Unbekannte Karte"), "Unbekannte Karte");
}

#[test]
fn rarity_order_and_wildcards() {
    assert!(Rarity::Mythic > Rarity::Rare && Rarity::Common > Rarity::BasicLand);
    assert_eq!(Rarity::from_arena(9), Rarity::Token);
    assert!(!Rarity::BasicLand.needs_wildcard() && Rarity::Common.needs_wildcard());
    let slots: Vec<usize> = Rarity::WILDCARDS
        .iter()
        .map(|r| r.wildcard_slot())
        .collect();
    assert_eq!(slots, [0, 1, 2, 3]);
    assert_eq!(Rarity::Token.wildcard_slot(), 0);
}
