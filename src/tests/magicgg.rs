use super::*;
use crate::testutil::{GG_EVENT_HTML as EVENT_HTML, GG_INDEX_HTML as INDEX_HTML};

#[test]
fn parses_index_by_format() {
    let all = parse_index(INDEX_HTML, "");
    assert_eq!(all.len(), 3, "jede Kennung nur einmal, keine News");
    let standard = parse_index(INDEX_HTML, "standard");
    assert_eq!(standard.len(), 2);
    assert_eq!(
        standard[0],
        GgEvent {
            slug: "arena-championship-13-standard-decklists-a-k".into(),
            title: "Arena Championship 13 Standard Decklists A-K".into(),
            date: "October 24, 2026".into(),
        }
    );
    assert_eq!(
        standard[1].title,
        "Traditional Standard Ranked Decklists: November 9, 2026"
    );
    assert!(parse_index(INDEX_HTML, "pioneer").is_empty());
}

#[test]
fn parses_event_decks() {
    let decks = parse_event(EVENT_HTML);
    assert_eq!(decks.len(), 2, "leere Liste fällt weg");
    let d = &decks[0];
    assert_eq!(d.title, "Erika Muster");
    assert_eq!(d.subtitle, "1st Place");
    assert_eq!(d.event, "Arena Championship 13");
    assert_eq!(d.date, "October 24, 2026");
    assert_eq!(d.format, "Standard");
    assert_eq!(d.deck.main.len(), 3);
    assert_eq!(d.deck.main[2].name, "Hill Ogre");
    assert_eq!(d.deck.side[0].name, "Lava Spike");
    assert_eq!(decks[1].title, "Max O'Neil");
    assert!(decks[1].deck.side.is_empty());
    assert!(parse_event("<html></html>").is_empty());
}

#[test]
fn ids_and_files() {
    let a = deck_id("event-a", 0);
    assert!(a >= 0x8000_0000, "oberer Zahlenbereich");
    assert_eq!(a, deck_id("event-a", 0), "stabil");
    assert_ne!(a, deck_id("event-a", 1));
    assert_ne!(a, deck_id("event-b", 0));
    assert_eq!(cache_file("abc-12"), "gg_abc-12.html");
    assert_eq!(cache_file("../x"), "gg____x.html");
    assert_eq!(section("<a>x</a>", "b"), "");
    assert_eq!(decode(" A &amp; B &quot;C&quot;&nbsp;"), "A & B \"C\"");
}
