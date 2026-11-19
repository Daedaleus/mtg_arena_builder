use super::*;
use crate::testutil::*;

fn card(cost: &str) -> Card {
    Card {
        mana_cost: cost.into(),
        ..Default::default()
    }
}

#[test]
fn pay_with_colors() {
    const W: u8 = 1;
    const U: u8 = 2;
    const G: u8 = 16;
    let bear = card("{1}{G}");
    let counter = card("{U}{U}");
    assert!(can_pay(&[&bear], &[G, U]));
    assert!(!can_pay(&[&bear], &[U, U]));
    assert!(!can_pay(&[&counter], &[U, G]));
    // Duales Land muss für die richtige Farbe eingesetzt werden.
    assert!(can_pay(&[&bear, &counter], &[U | G, U, G, W]));
    assert!(!can_pay(&[&bear, &counter], &[U | G, G, G, W]));
    // Zu wenig Länder insgesamt
    assert!(!can_pay(&[&card("{3}")], &[W, W]));
    assert!(can_pay(&[], &[]));
}

fn plan_for(hand: &[GrpId], lands: &[GrpId], played: bool, opp: &[(GrpId, i32)]) -> Plan {
    plan(&std_db(), hand, lands, &[], played, opp, 0)
}

#[test]
fn uses_mana_fully() {
    // Mit Berg aus der Hand: Oger (3) statt Bär (2) – die Farben passen.
    let p = plan_for(&[MOUNTAIN, OGRE, BEAR], &[FOREST, MOUNTAIN], false, &[]);
    assert_eq!(p.land, Some(MOUNTAIN));
    assert_eq!(p.spells, vec![OGRE]);
    assert_eq!((p.mana_used, p.mana_total), (3, 3));
    // Zwei Zauber passen zusammen.
    let p = plan_for(&[ELF, BEAR], &[FOREST, FOREST, FOREST], true, &[]);
    assert_eq!(p.land, None);
    assert_eq!(p.spells, vec![BEAR, ELF], "teurere Kreatur zuerst");
    assert_eq!(p.mana_used, 3);
}

#[test]
fn checks_colors() {
    // Grüne Karten mit roten Ländern: nichts bezahlbar.
    let p = plan_for(&[BEAR, ELF], &[MOUNTAIN, MOUNTAIN], true, &[]);
    assert!(p.spells.is_empty());
    // Unbekannte Länder gelten als beliebige Farbe.
    let p = plan_for(&[ELF], &[GrpId(9999)], true, &[]);
    assert_eq!(p.spells, vec![ELF]);
}

#[test]
fn chooses_the_right_land() {
    // Das Land, das die fehlende Farbe bringt, gewinnt.
    let p = plan_for(&[ISLAND, FOREST, BEAR], &[MOUNTAIN], false, &[]);
    assert_eq!(p.land, Some(FOREST));
    assert_eq!(p.spells, vec![BEAR]);
    // Ohne bezahlbaren Zauber lieber das getappte Land legen.
    let p = plan_for(&[TAPLAND_UR, MOUNTAIN, GIANT], &[MOUNTAIN], false, &[]);
    assert_eq!(p.land, Some(TAPLAND_UR));
    assert!(p.land_tapped);
    // Getapptes Land liefert diesen Zug kein Mana.
    let p = plan_for(&[TAPLAND_UR, ELF], &[], false, &[]);
    assert!(p.spells.is_empty());
    // Bereits gelegt: kein Landvorschlag.
    let p = plan_for(&[FOREST, ELF], &[FOREST], true, &[]);
    assert_eq!(p.land, None);
}

#[test]
fn removal_targets_and_holding() {
    // Entfernung zuerst, auf die stärkste Kreatur.
    let p = plan_for(
        &[MURDER, BEAR],
        &[SWAMP, SWAMP, SWAMP, FOREST, FOREST],
        true,
        &[(ELF, 1), (DRAGON, 5)],
    );
    assert_eq!(p.spells, vec![MURDER, BEAR]);
    assert_eq!(p.target, Some(DRAGON));
    // Ohne gegnerische Kreaturen: spontane Entfernung aufheben.
    let p = plan_for(
        &[MURDER, DIVINATION],
        &[SWAMP, SWAMP, SWAMP, ISLAND],
        true,
        &[],
    );
    assert_eq!(p.spells, vec![DIVINATION]);
    assert_eq!(p.target, None);
    assert!(p.hold.is_empty(), "nach Divination bleibt nur 1 Mana");
    let p = plan_for(&[MURDER], &[SWAMP, SWAMP, SWAMP], true, &[]);
    assert_eq!(p.hold, vec![MURDER]);
    // Hexerei-Entfernung ohne Ziel wird gar nicht vorgeschlagen.
    let p = plan_for(&[SPIKE], &[MOUNTAIN], true, &[]);
    assert!(p.spells.is_empty() && p.hold.is_empty());
    // Konter immer zurückhalten.
    let p = plan_for(&[COUNTER, SPIRIT], &[ISLAND; 6], true, &[]);
    assert_eq!(p.spells, vec![SPIRIT]);
    assert_eq!(p.hold, vec![COUNTER]);
}

#[test]
fn wipes_only_when_behind() {
    let lands = [PLAINS, PLAINS, PLAINS, PLAINS];
    let db = std_db();
    let behind = plan(&db, &[WRATH], &lands, &[], true, &[(BEAR, 2), (OGRE, 3)], 0);
    assert_eq!(behind.spells, vec![WRATH]);
    let ahead = plan(&db, &[WRATH], &lands, &[], true, &[(BEAR, 2)], 2);
    assert!(ahead.spells.is_empty());
}

#[test]
fn tricks_are_kept_open() {
    // Giant Growth (Spontan, kein Removal) wird nur gespielt, wenn sonst nichts geht.
    let p = plan_for(&[GROWTH, ELF], &[FOREST], true, &[]);
    assert_eq!(p.spells, vec![ELF]);
}

#[test]
fn many_cards_are_limited() {
    let hand = [ELF; 20];
    let p = plan_for(&hand, &[FOREST, FOREST], true, &[]);
    assert_eq!(p.spells.len(), 2);
}

#[test]
fn creatures_after_combat_rule() {
    let db = std_db();
    let only_creatures = Plan {
        spells: vec![BEAR, ELF],
        ..Default::default()
    };
    assert!(creatures_after_combat(&db, &only_creatures));
    let with_spell = Plan {
        spells: vec![BEAR, DIVINATION],
        ..Default::default()
    };
    assert!(!creatures_after_combat(&db, &with_spell));
    assert!(!creatures_after_combat(&db, &Plan::default()));
    // Eile: sofort spielen.
    let hasty = CardDb::from_cards(
        vec![
            crate::testutil::card(1, "Raging Goblin")
                .cost("{R}")
                .creature(1, 1)
                .abilities(&[crate::carddb::keyword::HASTE])
                .build(),
        ],
        "t",
    );
    assert!(!creatures_after_combat(
        &hasty,
        &Plan {
            spells: vec![GrpId(1)],
            ..Default::default()
        }
    ));
}

#[test]
fn instant_removal_in_opponents_turn() {
    let db = std_db();
    assert_eq!(
        instant_removal(&db, &[SHOCK, MURDER, BEAR], &[MOUNTAIN], &[]),
        vec![SHOCK]
    );
    assert!(
        instant_removal(&db, &[SPIKE], &[MOUNTAIN], &[]).is_empty(),
        "Hexerei"
    );
    assert!(instant_removal(&db, &[SHOCK], &[], &[]).is_empty());
}

#[test]
fn mana_creatures_count_as_sources() {
    let db = std_db();
    // Elf (G) und ein Wald: Bär bezahlbar; Oger nicht (kein Rot).
    assert_eq!(sources(&db, &[FOREST], &[ELF, BEAR]), vec![1 << 4, 1 << 4]);
    let p = plan(&db, &[BEAR, OGRE], &[FOREST], &[ELF], true, &[], 0);
    assert_eq!(p.spells, vec![BEAR]);
    assert_eq!(p.mana_total, 2);
    assert!(
        instant_removal(&db, &[SHOCK], &[], &[ELF]).is_empty(),
        "Elf liefert kein Rot"
    );
}
