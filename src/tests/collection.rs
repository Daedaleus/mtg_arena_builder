use super::*;
use crate::testutil::{self, TempDir};
use serde_json::json;

#[test]
fn builds_from_reader_output() {
    let cards = json!({ "cards": [
        { "grpId": 20, "qty": 4 },
        { "grpId": 21, "qty": 150 },
        { "grpId": 0, "qty": 2 },
        { "grpId": 22, "qty": 0 },
        { "grpId": 23, "qty": -3 }
    ]});
    let inv = json!({
        "wildcards": { "common": 10, "uncommon": 5, "rare": 2, "mythic": -1 },
        "gold": 1500,
        "gems": 200
    });
    let c = from_reader(&cards, &inv, 42).unwrap();
    assert_eq!(c.cards.len(), 2);
    assert_eq!(c.cards[&GrpId(21)], 99, "auf 99 begrenzt");
    assert_eq!(c.wildcards.as_array(), [10, 5, 2, 0]);
    assert_eq!((c.gold, c.gems, c.updated_unix), (1500, 200, 42));
    assert_eq!(c.total_cards(), 103);
}

#[test]
fn reader_errors() {
    let err = |text: &str| from_reader(&json!({ "error": text }), &json!({}), 0).unwrap_err();
    // Meldungen des Speicherlesers werden Fehlerarten zugeordnet.
    assert!(matches!(
        err("could not open MTGA process (run elevated)"),
        CollectionError::AccessDenied
    ));
    assert!(matches!(
        err("Process not found"),
        CollectionError::AccessDenied
    ));
    assert!(matches!(
        err("WrapperController.Instance is null (must be on the home screen, not in a match)"),
        CollectionError::NotInMenu
    ));
    assert!(matches!(
        err("Platform not supported"),
        CollectionError::Unsupported
    ));
    assert!(matches!(err("etwas Neues"), CollectionError::Reader(_)));
    let err_obj = json!({ "error": { "code": 5 } });
    assert!(
        from_reader(&err_obj, &json!({}), 0)
            .unwrap_err()
            .to_string()
            .contains("5")
    );
    assert!(matches!(
        from_reader(&json!({ "cards": [] }), &json!({}), 0).unwrap_err(),
        CollectionError::NoCards
    ));
    // Fehler beim Inventar: Karten bleiben, Wildcards leer.
    let c = from_reader(
        &json!({ "cards": [ { "grpId": 1, "qty": 1 } ] }),
        &json!({ "error": "x" }),
        0,
    )
    .unwrap();
    assert_eq!(c.wildcards.as_array(), [0; 4]);
}

#[test]
fn owned_by_name_sums_printings() {
    let db = crate::carddb::CardDb::from_cards(
        vec![
            testutil::card(1, "Shock").build(),
            testutil::card(2, "Shock").build(),
            testutil::card(3, "Opt").build(),
        ],
        "t",
    );
    let mut c = Collection::default();
    c.cards.insert(GrpId(1), 2);
    c.cards.insert(GrpId(2), 3);
    c.cards.insert(GrpId(99), 1);
    let owned = c.owned_by_name(&db);
    assert_eq!(owned.get("shock"), Some(&5));
    assert_eq!(owned.get("opt"), None);
}

#[test]
fn save_load_roundtrip() {
    let dir = TempDir::new("collection");
    let mut c = Collection::default();
    c.cards.insert(GrpId(7), 4);
    c.wildcards.rare = 3;
    c.updated_unix = 1;
    let path = dir.path("collection.json");
    c.save(&path).unwrap();
    let back = Collection::load(&path).unwrap();
    assert_eq!(back.cards, c.cards);
    assert_eq!(back.wildcards.rare, 3);
    assert!(back.age_text().ends_with("Tagen"), "Stand von 1970");
    let fresh = Collection {
        updated_unix: now_unix(),
        ..Default::default()
    };
    assert_eq!(fresh.age_text(), "gerade eben");
    assert!(Collection::load(&dir.path("fehlt.json")).is_none());
    assert!(c.save(&dir.path("kein/ordner/c.json")).is_err());
}

#[test]
fn reader_error_text() {
    assert_eq!(err_of(&json!({ "ok": 1 })), None);
    assert_eq!(err_of(&json!({ "error": "x" })).as_deref(), Some("x"));
}

#[test]
fn errors_know_when_elevation_helps() {
    assert!(CollectionError::AccessDenied.needs_elevation());
    assert!(CollectionError::Reader("?".into()).needs_elevation());
    for e in [
        CollectionError::NotRunning,
        CollectionError::NotInMenu,
        CollectionError::NoCards,
        CollectionError::ReaderPanicked,
        CollectionError::Unsupported,
    ] {
        assert!(!e.needs_elevation(), "{e}");
    }
}

#[test]
fn error_texts_follow_the_language() {
    let all = [
        CollectionError::NotRunning,
        CollectionError::AccessDenied,
        CollectionError::NotInMenu,
        CollectionError::NoCards,
        CollectionError::ReaderPanicked,
        CollectionError::Unsupported,
        CollectionError::Reader("x".into()),
        CollectionError::ElevationCancelled,
        CollectionError::Helper("y".into()),
        CollectionError::HelperStart(std::io::Error::other("z")),
        CollectionError::SavedUnreadable,
        CollectionError::Save("w".into()),
    ];
    for e in &all {
        // Anzeige deutsch (Tests laufen auf Deutsch), technischer Text englisch.
        let shown = e.user_text();
        assert!(!shown.is_empty() && !shown.contains('%'), "{shown}");
        assert!(e.to_string().is_ascii(), "{e}");
    }
    assert!(CollectionError::NotInMenu.user_text().contains("Hauptmenü"));
}
