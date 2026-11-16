use super::*;
use crate::deck::{evaluate, parse};
use crate::testutil::*;

fn owned(pairs: &[(&str, u32)]) -> HashMap<String, u32> {
    pairs.iter().map(|(k, n)| (k.to_string(), *n)).collect()
}

#[test]
fn crafts_within_budget_and_replaces_the_rest() {
    let db = std_db();
    let deck = parse("4 Serra Angel\n4 Hill Ogre\n4 Hedge Maze\n12 Plains\n");
    let mine = owned(&[("hill ogre", 4), ("shivan dragon", 4)]);
    let eval = evaluate(&deck, &db, &mine);

    // Genug Rare-Wildcards: Engel herstellen, Hedge Maze (Uncommon) fehlt.
    let p = plan(&eval, &db, &mine, None, [0, 0, 4, 0], true);
    assert_eq!(p.crafted, [0, 0, 4, 0]);
    // Kein passendes eigenes Land: Standardland der Hauptfarbe (Weiß).
    assert_eq!(
        p.replacements,
        vec![("hedge maze".to_string(), "plains".to_string())]
    );
    assert_eq!(p.replaced, 4);
    assert!(p.unresolved.is_empty());

    // Keine Wildcards: Engel durch den eigenen Drachen ersetzen.
    let p = plan(&eval, &db, &mine, None, [0; 4], true);
    assert_eq!(p.crafted, [0; 4]);
    assert!(
        p.replacements
            .contains(&("serra angel".to_string(), "shivan dragon".to_string()))
    );
    assert_eq!(p.replaced, 8);
    let text = plan_text(&p);
    assert!(text.starts_with("Budget-Version: 8 Karten durch eigene ersetzt"));
    assert!(!text.contains("Ohne passenden Ersatz"));
}

#[test]
fn reports_missing_replacements() {
    let db = std_db();
    let deck = parse("4 Cancel\n4 Storm Spirit\nSideboard\n2 Murder\n");
    let mine = owned(&[("divination", 2)]);
    let eval = evaluate(&deck, &db, &mine);
    let p = plan(&eval, &db, &mine, None, [0; 4], false);
    // Nebendeck wird ignoriert; ohne passenden Ersatz bleiben die Karten offen.
    assert_eq!(p.unresolved.len(), 2);
    assert!(!p.unresolved.contains(&"Murder".to_string()));
    assert!(plan_text(&p).contains("Ohne passenden Ersatz: "));
}

#[test]
fn partial_replacement_is_unresolved() {
    let db = crate::carddb::CardDb::from_cards(
        vec![
            card(1, "Ogre").cost("{2}{R}").creature(3, 3).build(),
            card(2, "Brute").cost("{2}{R}").creature(3, 3).build(),
        ],
        "t",
    );
    let deck = parse("4 Ogre\n");
    let mine = owned(&[("brute", 1)]);
    let eval = evaluate(&deck, &db, &mine);
    let p = plan(&eval, &db, &mine, None, [0; 4], true);
    assert_eq!(
        p.replacements,
        vec![("ogre".to_string(), "brute".to_string())]
    );
    assert_eq!(p.replaced, 1);
    assert_eq!(p.unresolved, vec!["Ogre".to_string()]);
}

#[test]
fn priorities() {
    let need = |qty, land| Need {
        key: String::new(),
        name: String::new(),
        qty,
        missing: qty,
        rarity: 2,
        land,
        cmc: 1,
    };
    assert_eq!(need(4, false).priority(), 3);
    assert_eq!(need(1, true).priority(), 2);
    assert_eq!(need(2, false).priority(), 2);
    assert_eq!(need(1, false).priority(), 1);
}

#[test]
fn replacement_quantities() {
    let db = std_db();
    let mine = owned(&[("hill ogre", 2)]);
    assert_eq!(
        replaced_qty(&db, &mine, "mountain", 7),
        7,
        "Standardländer unbegrenzt"
    );
    assert_eq!(replaced_qty(&db, &mine, "hill ogre", 3), 2);
    assert_eq!(replaced_qty(&db, &mine, "hill ogre", 1), 1);
    assert_eq!(replaced_qty(&db, &mine, "shock", 4), 0);
}
