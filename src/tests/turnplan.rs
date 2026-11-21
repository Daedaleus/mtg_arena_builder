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
    // A dual land must be used for the right color.
    assert!(can_pay(&[&bear, &counter], &[U | G, U, G, W]));
    assert!(!can_pay(&[&bear, &counter], &[U | G, G, G, W]));
    // Too few lands in total
    assert!(!can_pay(&[&card("{3}")], &[W, W]));
    assert!(can_pay(&[], &[]));
}

fn plan_for(hand: &[GrpId], lands: &[GrpId], played: bool, opp: &[(GrpId, i32)]) -> Plan {
    plan(&std_db(), hand, lands, &[], played, opp, 0)
}

#[test]
fn uses_mana_fully() {
    // With a mountain from the hand: ogre (3) instead of bear (2) – the colors fit.
    let p = plan_for(&[MOUNTAIN, OGRE, BEAR], &[FOREST, MOUNTAIN], false, &[]);
    assert_eq!(p.land, Some(MOUNTAIN));
    assert_eq!(p.spells, vec![OGRE]);
    assert_eq!((p.mana_used, p.mana_total), (3, 3));
    // Two spells fit together.
    let p = plan_for(&[ELF, BEAR], &[FOREST, FOREST, FOREST], true, &[]);
    assert_eq!(p.land, None);
    assert_eq!(p.spells, vec![BEAR, ELF], "more expensive creature first");
    assert_eq!(p.mana_used, 3);
}

#[test]
fn checks_colors() {
    // Green cards with red lands: nothing affordable.
    let p = plan_for(&[BEAR, ELF], &[MOUNTAIN, MOUNTAIN], true, &[]);
    assert!(p.spells.is_empty());
    // Unknown lands count as any color.
    let p = plan_for(&[ELF], &[GrpId(9999)], true, &[]);
    assert_eq!(p.spells, vec![ELF]);
}

#[test]
fn chooses_the_right_land() {
    // The land that brings the missing color wins.
    let p = plan_for(&[ISLAND, FOREST, BEAR], &[MOUNTAIN], false, &[]);
    assert_eq!(p.land, Some(FOREST));
    assert_eq!(p.spells, vec![BEAR]);
    // Without an affordable spell rather play the tapped land.
    let p = plan_for(&[TAPLAND_UR, MOUNTAIN, GIANT], &[MOUNTAIN], false, &[]);
    assert_eq!(p.land, Some(TAPLAND_UR));
    assert!(p.land_tapped);
    // A tapped land provides no mana this turn.
    let p = plan_for(&[TAPLAND_UR, ELF], &[], false, &[]);
    assert!(p.spells.is_empty());
    // Already played: no land suggestion.
    let p = plan_for(&[FOREST, ELF], &[FOREST], true, &[]);
    assert_eq!(p.land, None);
}

#[test]
fn removal_targets_and_holding() {
    // Removal first, on the strongest creature.
    let p = plan_for(
        &[MURDER, BEAR],
        &[SWAMP, SWAMP, SWAMP, FOREST, FOREST],
        true,
        &[(ELF, 1), (DRAGON, 5)],
    );
    assert_eq!(p.spells, vec![MURDER, BEAR]);
    assert_eq!(p.target, Some(DRAGON));
    // Without opposing creatures: keep instant removal.
    let p = plan_for(
        &[MURDER, DIVINATION],
        &[SWAMP, SWAMP, SWAMP, ISLAND],
        true,
        &[],
    );
    assert_eq!(p.spells, vec![DIVINATION]);
    assert_eq!(p.target, None);
    assert!(p.hold.is_empty(), "after Divination only 1 mana is left");
    let p = plan_for(&[MURDER], &[SWAMP, SWAMP, SWAMP], true, &[]);
    assert_eq!(p.hold, vec![MURDER]);
    // Sorcery removal without a target isn't suggested at all.
    let p = plan_for(&[SPIKE], &[MOUNTAIN], true, &[]);
    assert!(p.spells.is_empty() && p.hold.is_empty());
    // Always hold counters back.
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
    // Giant Growth (instant, no removal) is only cast if nothing else works.
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
    // Haste: cast right away.
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
    // Elf (G) and a forest: bear affordable; ogre not (no red).
    assert_eq!(sources(&db, &[FOREST], &[ELF, BEAR]), vec![1 << 4, 1 << 4]);
    let p = plan(&db, &[BEAR, OGRE], &[FOREST], &[ELF], true, &[], 0);
    assert_eq!(p.spells, vec![BEAR]);
    assert_eq!(p.mana_total, 2);
    assert!(
        instant_removal(&db, &[SHOCK], &[], &[ELF]).is_empty(),
        "elf provides no red"
    );
}

#[test]
fn treasures_count_as_mana_sources() {
    let db = std_db();
    // A forest and a Treasure: the ogre (2 + red) becomes affordable via the
    // Treasure's any-color mana.
    let p = plan(&db, &[OGRE], &[FOREST, FOREST], &[TREASURE], true, &[], 0);
    assert_eq!(p.spells, vec![OGRE]);
    assert_eq!(p.mana_total, 3);
    assert_eq!(db.resolve("Treasure"), None, "tokens aren't found by name");
}

#[test]
fn legend_rule_allows_one_copy() {
    let db = std_db();
    assert_eq!(
        without_legend_duplicates(&db, &[LEGEND, BEAR, LEGEND], &[]),
        vec![LEGEND, BEAR]
    );
    assert_eq!(
        without_legend_duplicates(&db, &[LEGEND, BEAR], &[LEGEND, OGRE]),
        vec![BEAR]
    );
    // Non-legendary cards may appear several times.
    assert_eq!(
        without_legend_duplicates(&db, &[BEAR, BEAR], &[BEAR]),
        vec![BEAR, BEAR]
    );
}
