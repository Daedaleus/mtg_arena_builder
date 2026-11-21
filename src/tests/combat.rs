use super::*;
use crate::testutil::*;

fn cr(grp: u32, power: i32, toughness: i32) -> LiveCreature {
    LiveCreature {
        grp_id: grp,
        power,
        toughness,
        ..Default::default()
    }
}

#[test]
fn blocks() {
    // 3/3 blocks 2/2 and wins; 1/1 stays back.
    let p = plan_blocks(&[cr(1, 2, 2)], &[cr(10, 1, 1), cr(11, 3, 3)], 20);
    assert_eq!(p.blocks.len(), 1);
    assert_eq!(p.blocks[0].blockers, vec![11]);
    assert_eq!(p.blocks[0].kind, BlockKind::Win);
    assert_eq!(p.damage, 0);

    // 2/2 against 4/4: no worthwhile block with a lot of life …
    let p = plan_blocks(&[cr(1, 4, 4)], &[cr(10, 2, 2)], 20);
    assert!(p.blocks.is_empty());
    assert_eq!(p.damage, 4);
    // … but mandatory if the damage would be lethal.
    let p = plan_blocks(&[cr(1, 4, 4)], &[cr(10, 2, 2)], 4);
    assert_eq!(p.blocks[0].kind, BlockKind::Chump);
    assert!(!p.lethal);

    // Equally strong creatures trade.
    let p = plan_blocks(&[cr(1, 3, 3)], &[cr(10, 3, 3)], 20);
    assert_eq!(p.blocks[0].kind, BlockKind::Trade);

    // First strike: 2/2 with first strike wins against 3/2.
    let mut fs = cr(10, 2, 2);
    fs.first_strike = true;
    let p = plan_blocks(&[cr(1, 3, 2)], &[fs], 20);
    assert_eq!(p.blocks[0].kind, BlockKind::Win);

    // A flyer can't be blocked from the ground.
    let mut flyer = cr(1, 3, 3);
    flyer.flying = true;
    let p = plan_blocks(&[flyer], &[cr(10, 5, 5)], 3);
    assert!(p.blocks.is_empty());
    assert!(p.lethal);
}

#[test]
fn block_outcomes_with_keywords() {
    // An attacker with first strike kills the blocker first.
    let mut fs = cr(1, 3, 2);
    fs.first_strike = true;
    assert_eq!(outcome(&cr(10, 3, 3), &fs), (true, false));
    // Deathtouch also kills big blockers.
    let mut dt = cr(1, 1, 1);
    dt.deathtouch = true;
    assert_eq!(outcome(&cr(10, 6, 6), &dt), (true, true));
    // Indestructible survives everything.
    let mut ind = cr(10, 1, 5);
    ind.indestructible = true;
    assert_eq!(outcome(&ind, &cr(1, 9, 9)), (false, false));
    assert!(
        !kills(&cr(1, 0, 1), &cr(10, 1, 1)),
        "ohne Kraft kein Schaden"
    );
}

#[test]
fn safe_blocks_and_trample() {
    // A wall stops the attacker without dying.
    let p = plan_blocks(&[cr(1, 2, 2)], &[cr(10, 0, 4)], 20);
    assert_eq!(p.blocks[0].kind, BlockKind::Safe);
    assert_eq!(p.damage, 0);
    // Trample: blocking only pays off if damage is absorbed.
    let mut tr = cr(1, 5, 5);
    tr.trample = true;
    assert_eq!(through(&tr, &[&cr(10, 0, 3)]), 2);
    assert_eq!(through(&tr, &[&cr(10, 0, 2), &cr(11, 0, 2)]), 1);
    assert_eq!(through(&tr, &[]), 5);
    let p = plan_blocks(&[tr.clone()], &[cr(10, 0, 6)], 20);
    assert_eq!(p.blocks[0].kind, BlockKind::Safe);
    assert_eq!(p.damage, 0);
    // An expensive blocker doesn't trade with a small attacker.
    let mut big = cr(10, 2, 2);
    big.deathtouch = true;
    let p = plan_blocks(&[cr(1, 2, 2)], &[big], 20);
    assert!(p.blocks.is_empty());
}

#[test]
fn menace_and_chumps() {
    let mut m = cr(1, 3, 3);
    m.menace = true;
    // Two blockers that kill the attacker together and survive: win.
    let p = plan_blocks(
        &[m.clone()],
        &[cr(10, 2, 4), cr(11, 1, 4), cr(12, 0, 1)],
        20,
    );
    assert_eq!(p.blocks.len(), 1);
    assert_eq!(p.blocks[0].kind, BlockKind::Win);
    assert_eq!(p.blocks[0].blockers, vec![11, 10]);
    assert_eq!(p.damage, 0);
    // A single blocker isn't enough; no chump block without need.
    let p = plan_blocks(&[m.clone()], &[cr(10, 5, 5)], 20);
    assert!(p.blocks.is_empty());
    // If a blocker would die, no "win" – but chump block against lethal damage.
    let p = plan_blocks(&[m.clone()], &[cr(10, 1, 1), cr(11, 1, 1)], 20);
    assert!(p.blocks.is_empty());
    let p = plan_blocks(&[m.clone()], &[cr(10, 1, 1), cr(11, 1, 1)], 3);
    assert_eq!(p.blocks[0].kind, BlockKind::Chump);
    assert_eq!(p.blocks[0].blockers.len(), 2);
    assert!(!p.lethal);
    // Indestructible: blocking together doesn't help.
    let mut ind = m.clone();
    ind.indestructible = true;
    assert!(menace_pair(&ind, &[cr(10, 5, 5), cr(11, 5, 5)], &[0, 1]).is_none());
    // Deathtouch is enough even with little power.
    let mut dt = cr(10, 1, 4);
    dt.deathtouch = true;
    assert!(menace_pair(&m, &[dt, cr(11, 0, 4)], &[0, 1]).is_some());

    // Several attackers: chump only as many as needed, biggest first.
    let p = plan_blocks(
        &[cr(1, 4, 4), cr(2, 3, 3), cr(3, 1, 1)],
        &[cr(10, 0, 1), cr(11, 0, 1)],
        6,
    );
    let chumped: Vec<u32> = p
        .blocks
        .iter()
        .filter(|b| b.kind == BlockKind::Chump)
        .map(|b| b.attacker)
        .collect();
    assert_eq!(chumped, vec![1]);
    assert_eq!(p.damage, 4);
    assert_eq!(p.damage_unblocked, 8);
    assert!(!p.lethal);
}

#[test]
fn tricks_seen_and_typical() {
    let db = std_db();
    // Already seen: instants affordable with open mana.
    let t = likely_tricks(&db, None, &[SHOCK, BEAR, MURDER, SHOCK], 1);
    assert_eq!(t, vec![("Shock".to_string(), None)]);
    assert!(likely_tricks(&db, None, &[SHOCK], 0).is_empty());
    // Typical for the archetype (from 25 % of the lists), most frequent first.
    let arch = crate::live::ArchShare {
        name: "X".into(),
        share: [
            ("giant growth".to_string(), 0.5),
            ("cancel".to_string(), 0.9),
            ("shock".to_string(), 1.0),
            ("murder".to_string(), 0.1),
            ("grizzly bears".to_string(), 1.0),
        ]
        .into(),
    };
    let t = likely_tricks(&db, Some(&arch), &[SHOCK], 3);
    let names: Vec<(&str, Option<f32>)> = t.iter().map(|(n, s)| (n.as_str(), *s)).collect();
    assert_eq!(
        names,
        vec![
            ("Shock", None),
            ("Cancel", Some(0.9)),
            ("Giant Growth", Some(0.5))
        ]
    );
    // Flash counts like an instant.
    let flash = CardDb::from_cards(
        vec![
            crate::testutil::card(1, "Ambusher")
                .cost("{G}")
                .creature(2, 1)
                .abilities(&[crate::carddb::keyword::FLASH])
                .build(),
        ],
        "t",
    );
    assert_eq!(likely_tricks(&flash, None, &[GrpId(1)], 1).len(), 1);
}

#[test]
fn fights_resolve() {
    // Single block like outcome
    assert_eq!(fight(&cr(1, 3, 3), &[&cr(10, 2, 2)]), (false, vec![true]));
    // Two blockers against menace: 3 damage kill the first 2/2, the second survives.
    assert_eq!(
        fight(&cr(1, 3, 3), &[&cr(10, 2, 2), &cr(11, 2, 2)]),
        (true, vec![true, false])
    );
    // First strike: the killed blocker doesn't strike back.
    let mut fs = cr(1, 2, 3);
    fs.first_strike = true;
    assert_eq!(
        fight(&fs, &[&cr(10, 2, 2), &cr(11, 2, 2)]),
        (false, vec![true, false])
    );
    // Deathtouch assigns 1 damage each.
    let mut dt = cr(1, 2, 5);
    dt.deathtouch = true;
    assert_eq!(
        fight(&dt, &[&cr(10, 1, 4), &cr(11, 1, 4), &cr(12, 1, 4)]),
        (false, vec![true, true, false])
    );
    // Indestructible blockers don't die, without power nobody dies.
    let mut ind = cr(10, 0, 1);
    ind.indestructible = true;
    assert_eq!(
        fight(&cr(1, 5, 5), &[&ind, &cr(11, 0, 1)]),
        (false, vec![false, true])
    );
}

#[test]
fn worth_attacking_keeps_blockers_and_tappers_home() {
    let db = std_db();
    // Elf: tap ability. Bear: free. Angel: vigilance. Unknown: stays.
    let safe = [ELF, BEAR, ANGEL, GrpId(9999)];
    assert_eq!(
        worth_attacking(&db, &safe, 20, 5),
        vec![BEAR, ANGEL, GrpId(9999)]
    );
    // The opponent has enough power for lethal damage: only vigilance attacks.
    assert_eq!(worth_attacking(&db, &safe, 5, 5), vec![ANGEL]);
}
