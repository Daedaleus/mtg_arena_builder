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
    // 3/3 blockt 2/2 und gewinnt; 1/1 bleibt zurück.
    let p = plan_blocks(&[cr(1, 2, 2)], &[cr(10, 1, 1), cr(11, 3, 3)], 20);
    assert_eq!(p.blocks.len(), 1);
    assert_eq!(p.blocks[0].blockers, vec![11]);
    assert_eq!(p.blocks[0].kind, BlockKind::Win);
    assert_eq!(p.damage, 0);

    // 2/2 gegen 4/4: kein lohnender Block bei viel Leben …
    let p = plan_blocks(&[cr(1, 4, 4)], &[cr(10, 2, 2)], 20);
    assert!(p.blocks.is_empty());
    assert_eq!(p.damage, 4);
    // … aber Pflicht, wenn der Schaden tödlich wäre.
    let p = plan_blocks(&[cr(1, 4, 4)], &[cr(10, 2, 2)], 4);
    assert_eq!(p.blocks[0].kind, BlockKind::Chump);
    assert!(!p.lethal);

    // Gleich starke Kreaturen tauschen.
    let p = plan_blocks(&[cr(1, 3, 3)], &[cr(10, 3, 3)], 20);
    assert_eq!(p.blocks[0].kind, BlockKind::Trade);

    // Erstschlag: 2/2 mit Erstschlag gewinnt gegen 3/2.
    let mut fs = cr(10, 2, 2);
    fs.first_strike = true;
    let p = plan_blocks(&[cr(1, 3, 2)], &[fs], 20);
    assert_eq!(p.blocks[0].kind, BlockKind::Win);

    // Flieger kann nicht vom Boden geblockt werden.
    let mut flyer = cr(1, 3, 3);
    flyer.flying = true;
    let p = plan_blocks(&[flyer], &[cr(10, 5, 5)], 3);
    assert!(p.blocks.is_empty());
    assert!(p.lethal);
}

#[test]
fn block_outcomes_with_keywords() {
    // Angreifer mit Erstschlag tötet den Blocker zuerst.
    let mut fs = cr(1, 3, 2);
    fs.first_strike = true;
    assert_eq!(outcome(&cr(10, 3, 3), &fs), (true, false));
    // Todesberührung tötet auch große Blocker.
    let mut dt = cr(1, 1, 1);
    dt.deathtouch = true;
    assert_eq!(outcome(&cr(10, 6, 6), &dt), (true, true));
    // Unzerstörbar überlebt alles.
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
    // Mauer hält den Angreifer auf, ohne zu sterben.
    let p = plan_blocks(&[cr(1, 2, 2)], &[cr(10, 0, 4)], 20);
    assert_eq!(p.blocks[0].kind, BlockKind::Safe);
    assert_eq!(p.damage, 0);
    // Trampel: Blocken lohnt nur, wenn Schaden abgefangen wird.
    let mut tr = cr(1, 5, 5);
    tr.trample = true;
    assert_eq!(through(&tr, &[&cr(10, 0, 3)]), 2);
    assert_eq!(through(&tr, &[&cr(10, 0, 2), &cr(11, 0, 2)]), 1);
    assert_eq!(through(&tr, &[]), 5);
    let p = plan_blocks(&[tr.clone()], &[cr(10, 0, 6)], 20);
    assert_eq!(p.blocks[0].kind, BlockKind::Safe);
    assert_eq!(p.damage, 0);
    // Teurer Blocker tauscht nicht gegen kleinen Angreifer.
    let mut big = cr(10, 2, 2);
    big.deathtouch = true;
    let p = plan_blocks(&[cr(1, 2, 2)], &[big], 20);
    assert!(p.blocks.is_empty());
}

#[test]
fn menace_and_chumps() {
    let mut m = cr(1, 3, 3);
    m.menace = true;
    // Zwei Blocker, die den Angreifer gemeinsam töten und überleben: gewinnen.
    let p = plan_blocks(
        &[m.clone()],
        &[cr(10, 2, 4), cr(11, 1, 4), cr(12, 0, 1)],
        20,
    );
    assert_eq!(p.blocks.len(), 1);
    assert_eq!(p.blocks[0].kind, BlockKind::Win);
    assert_eq!(p.blocks[0].blockers, vec![11, 10]);
    assert_eq!(p.damage, 0);
    // Ein einzelner Blocker reicht nicht, ohne Not wird nicht geopfert.
    let p = plan_blocks(&[m.clone()], &[cr(10, 5, 5)], 20);
    assert!(p.blocks.is_empty());
    // Würde ein Blocker sterben, kein „Gewinnen“ – bei tödlichem Schaden aber opfern.
    let p = plan_blocks(&[m.clone()], &[cr(10, 1, 1), cr(11, 1, 1)], 20);
    assert!(p.blocks.is_empty());
    let p = plan_blocks(&[m.clone()], &[cr(10, 1, 1), cr(11, 1, 1)], 3);
    assert_eq!(p.blocks[0].kind, BlockKind::Chump);
    assert_eq!(p.blocks[0].blockers.len(), 2);
    assert!(!p.lethal);
    // Unzerstörbar: gemeinsam blocken bringt nichts.
    let mut ind = m.clone();
    ind.indestructible = true;
    assert!(menace_pair(&ind, &[cr(10, 5, 5), cr(11, 5, 5)], &[0, 1]).is_none());
    // Todesberührung reicht auch mit wenig Kraft.
    let mut dt = cr(10, 1, 4);
    dt.deathtouch = true;
    assert!(menace_pair(&m, &[dt, cr(11, 0, 4)], &[0, 1]).is_some());

    // Mehrere Angreifer: nur so viele opfern wie nötig, größte zuerst.
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
    // Schon gesehen: Spontanzauber, die mit offenem Mana bezahlbar sind.
    let t = likely_tricks(&db, None, &[SHOCK, BEAR, MURDER, SHOCK], 1);
    assert_eq!(t, vec![("Shock".to_string(), None)]);
    assert!(likely_tricks(&db, None, &[SHOCK], 0).is_empty());
    // Typisch für den Archetyp (ab 25 % der Listen), häufigste zuerst.
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
    // Aufblitzen zählt wie ein Spontanzauber.
    let flash = CardDb::from_cards(
        vec![
            crate::testutil::card(1, "Ambusher")
                .cost("{G}")
                .creature(2, 1)
                .abilities(&[ABILITY_FLASH])
                .build(),
        ],
        "t",
    );
    assert_eq!(likely_tricks(&flash, None, &[1], 1).len(), 1);
}

#[test]
fn fights_resolve() {
    // Einzelblock wie outcome
    assert_eq!(fight(&cr(1, 3, 3), &[&cr(10, 2, 2)]), (false, vec![true]));
    // Zwei Blocker gegen Bedrohlich: 3 Schaden töten den ersten 2/2, der zweite überlebt.
    assert_eq!(
        fight(&cr(1, 3, 3), &[&cr(10, 2, 2), &cr(11, 2, 2)]),
        (true, vec![true, false])
    );
    // Erstschlag: der getötete Blocker schlägt nicht zurück.
    let mut fs = cr(1, 2, 3);
    fs.first_strike = true;
    assert_eq!(
        fight(&fs, &[&cr(10, 2, 2), &cr(11, 2, 2)]),
        (false, vec![true, false])
    );
    // Todesberührung verteilt je 1 Schaden.
    let mut dt = cr(1, 2, 5);
    dt.deathtouch = true;
    assert_eq!(
        fight(&dt, &[&cr(10, 1, 4), &cr(11, 1, 4), &cr(12, 1, 4)]),
        (false, vec![true, true, false])
    );
    // Unzerstörbare Blocker sterben nicht, ohne Kraft stirbt niemand.
    let mut ind = cr(10, 0, 1);
    ind.indestructible = true;
    assert_eq!(
        fight(&cr(1, 5, 5), &[&ind, &cr(11, 0, 1)]),
        (false, vec![false, true])
    );
}
