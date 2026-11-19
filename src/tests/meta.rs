use super::*;
use crate::testutil::TempDir;
use serde_json::json;

/// Ausschnitt der MTGTop8-Formatseite (Struktur wie im Original).
const FORMAT_HTML: &str = "<div><img src=/metas_thumbs/271.jpg></div>\n\
    <div align=center style=\"width:100%;\">\n\
      <div class=S14><a href=archetype?a=271&meta=50&f=ST>Jund Aggro</a></div>\n\
      <div>\n        \n\
        <div style=\"display:inline-block;width:48%;\" class=S14>11 %</div>\n\
      </div></div>\n\
    <div class=S14><a href=archetype?a=272&meta=50&f=ST>Azorius &amp; Friends</a></div>\n\
      <div>\n\
        <div class=S14>7 %</div>\n\
    <div class=S14><a href=archetype?a=271&meta=50&f=ST>Jund Aggro</a></div>\n\
      <div>\n\
        <div class=S14>11 %</div>\n";

/// Ausschnitt einer Archetypseite mit zwei Decks.
const ARCH_HTML: &str = "<tr><td><a href=/event?e=1&d=912128&f=ST>Jund Sacrifice</a></td>\n\
    \t  <td><a class=player href=/search?player=Erika+Muster>Erika Muster</a></td>\n\
    \t  <td><a href=/event?e=1&f=ST>World Championship 32</a></td>\n\
    \t  <td class=O16><img src=/graph/bigstar.png height=20></td>\n\
    \t  <td>5-8</td>\n\
    \t  <td>14/11/26</td></tr>\n\
    <tr><td><a href=/event?e=2&d=912200&f=ST>Jund</a></td>\n\
    \t  <td><a class=player href=/search?player=Max>Max O&#039;Neil</a></td>\n\
    \t  <td><a href=/event?e=2&f=ST>Arena Open &amp; More</a></td>\n\
    \t  <td class=O16></td>\n\
    \t  <td>1</td>\n\
    \t  <td>15/11/26</td></tr>";

#[test]
fn parses_archetypes() {
    let list = parse_archetypes(FORMAT_HTML);
    assert_eq!(list.len(), 2, "doppelte Kennungen nur einmal");
    assert_eq!((list[0].id, list[0].meta, list[0].share), (271, 50, 11));
    assert_eq!(list[0].name, "Jund Aggro");
    assert_eq!(list[1].name, "Azorius & Friends");
    assert!(parse_archetypes("<html></html>").is_empty());
}

#[test]
fn parses_deck_lists() {
    let decks = parse_decks(ARCH_HTML);
    assert_eq!(decks.len(), 2);
    assert_eq!(decks[0].id, 912128);
    assert_eq!(decks[0].player, "Erika Muster");
    assert_eq!(decks[0].event, "World Championship 32");
    assert_eq!(decks[0].rank, "5-8");
    assert_eq!(decks[0].date, "14/11/26");
    assert_eq!(decks[1].player, "Max O'Neil");
    assert_eq!(decks[1].event, "Arena Open & More");
    assert!(parse_decks("").is_empty());
}

#[test]
fn decodes_entities() {
    assert_eq!(
        decode_entities("  A &amp; B &quot;C&quot;&nbsp;&#039;D&#039; "),
        "A & B \"C\" 'D'"
    );
}

#[test]
fn parses_scryfall_pages() {
    let page = json!({
        "has_more": true,
        "next_page": "https://api.scryfall.com/cards/search?page=2",
        "data": [ { "name": "Shock" }, { "name": "Fire // Ice" }, { "noname": 1 } ]
    });
    let (names, next) = parse_scryfall_page(&page);
    assert_eq!(names, vec!["shock", "fire", "fire // ice"]);
    assert_eq!(
        next.as_deref(),
        Some("https://api.scryfall.com/cards/search?page=2")
    );
    let last = json!({ "has_more": false, "next_page": "x", "data": [] });
    assert_eq!(parse_scryfall_page(&last), (Vec::new(), None));
}

#[test]
fn works_from_cache_without_network() {
    let dir = TempDir::new("meta");
    std::fs::write(dir.path("format_ST.html"), FORMAT_HTML).unwrap();
    std::fs::write(dir.path("arch_ST_271.html"), ARCH_HTML).unwrap();
    std::fs::write(dir.path("deck_912128.txt"), "4 Shock\n").unwrap();
    std::fs::write(dir.path("deck_1.txt"), "<html>Fehler</html>").unwrap();
    std::fs::write(dir.path("legal_standard.json"), r#"["shock","opt"]"#).unwrap();

    let mut f = Fetcher::new(dir.0.clone());
    let archs = f.archetypes("ST", false).unwrap();
    assert_eq!(archs.len(), 2);
    let jund = archs[0].clone();
    assert_eq!(f.decks("ST", &jund).unwrap().len(), 2);
    assert_eq!(f.cached_decks("ST", &jund).unwrap().len(), 2);
    assert!(f.cached_decks("ST", &archs[1]).is_none());
    assert_eq!(f.decklist(912128).unwrap(), "4 Shock\n");
    assert_eq!(f.cached_decklist(912128).as_deref(), Some("4 Shock\n"));
    assert_eq!(f.cached_decklist(1), None, "HTML statt Liste");
    assert_eq!(f.cached_decklist(2), None);
    let legal = f.legal_names("standard").unwrap();
    assert!(legal.contains("shock") && legal.contains("opt"));
}

#[test]
fn cache_age_is_respected() {
    let dir = TempDir::new("meta");
    std::fs::write(dir.path("x.txt"), "hallo").unwrap();
    let f = Fetcher::new(dir.0.clone());
    assert_eq!(f.cached("x.txt", None).as_deref(), Some("hallo"));
    assert_eq!(f.cached("x.txt", Some(3600)).as_deref(), Some("hallo"));
    // Eine Stunde in der Zukunft gilt als frisch; max. Alter 0 nur, wenn sofort gelesen.
    assert_eq!(f.cached("fehlt.txt", None), None);
    assert_eq!(f.cached("fehlt.txt", Some(10)), None);
}

#[test]
fn format_table_is_consistent() {
    for (code, label, scryfall) in FORMATS {
        assert!(!code.is_empty() && !label.is_empty());
        assert_eq!(scryfall, scryfall.to_lowercase());
    }
}

#[test]
fn magic_gg_from_cache_without_network() {
    let dir = TempDir::new("gg");
    std::fs::write(dir.path("gg_index.html"), crate::testutil::GG_INDEX_HTML).unwrap();
    let slug = "arena-championship-13-standard-decklists-a-k";
    std::fs::write(
        dir.path(&crate::magicgg::cache_file(slug)),
        crate::testutil::GG_EVENT_HTML,
    )
    .unwrap();
    let mut f = Fetcher::new(dir.0.clone());
    let events = f.gg_events("standard", false).unwrap();
    assert_eq!(events.len(), 2);
    assert!(f.gg_events("pioneer", false).is_err());
    let decks = f.gg_decks(slug).unwrap();
    assert_eq!(decks.len(), 2);
    // Seite ohne Decks: Fehler, und die Datei wird verworfen.
    std::fs::write(
        dir.path(&crate::magicgg::cache_file("leer")),
        "<html></html>",
    )
    .unwrap();
    assert!(f.gg_decks("leer").is_err());
    assert!(!dir.path(&crate::magicgg::cache_file("leer")).exists());
}

#[test]
fn old_archetype_pages_are_restored() {
    let dir = TempDir::new("meta");
    let page = dir.path("arch_ST_271.html");
    std::fs::write(&page, ARCH_HTML).unwrap();
    // Datei zwei Tage zurückdatieren.
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(48 * 3600);
    std::fs::File::options()
        .write(true)
        .open(&page)
        .unwrap()
        .set_modified(old)
        .unwrap();
    let f = Fetcher::new(dir.0.clone());
    let arch = Archetype {
        id: 271,
        name: "Jund".into(),
        share: 10,
        meta: 50,
    };
    assert_eq!(f.cached_decks("ST", &arch).unwrap().len(), 2);
    assert!(
        f.cached("arch_ST_271.html", Some(12 * HOUR)).is_none(),
        "für Abrufe veraltet"
    );
}

#[test]
fn restores_opened_gg_events() {
    let dir = TempDir::new("gg");
    let f = Fetcher::new(dir.0.clone());
    assert!(f.cached_gg("standard").is_empty(), "ohne Übersicht nichts");
    std::fs::write(dir.path("gg_index.html"), crate::testutil::GG_INDEX_HTML).unwrap();
    let slug = "arena-championship-13-standard-decklists-a-k";
    std::fs::write(
        dir.path(&crate::magicgg::cache_file(slug)),
        crate::testutil::GG_EVENT_HTML,
    )
    .unwrap();
    // Nur das geöffnete Event (mit Datei im Cache) kommt zurück.
    let restored = f.cached_gg("standard");
    assert_eq!(restored.len(), 1);
    assert_eq!(restored[0].0.slug, slug);
    assert_eq!(restored[0].1.len(), 2);
    assert!(f.cached_gg("pioneer").is_empty());
}

/// Netzabruf für Tests: liefert festen Inhalt und zählt die Abrufe.
struct FakeHttp {
    calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    body: &'static [u8],
}

impl Http for FakeHttp {
    fn get(&self, url: &str, _accept: &str) -> Result<Vec<u8>> {
        self.calls
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if url.contains("fehler") {
            anyhow::bail!("kein Netz");
        }
        Ok(self.body.to_vec())
    }
}

#[test]
fn downloads_once_then_uses_cache() {
    let dir = TempDir::new("meta_http");
    let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let http = FakeHttp {
        calls: calls.clone(),
        body: b"Caf\xe9",
    };
    let delays = Delays {
        top8: Duration::ZERO,
        gg: Duration::ZERO,
    };
    let mut f = Fetcher::with_http(dir.0.clone(), Box::new(http), delays);
    // MTGTop8 liefert ISO-8859-1.
    assert_eq!(f.top8("seite", "seite.html", None).unwrap(), "Café");
    assert_eq!(f.top8("seite", "seite.html", None).unwrap(), "Café");
    assert_eq!(
        calls.load(std::sync::atomic::Ordering::Relaxed),
        1,
        "zweites Mal aus dem Cache"
    );
    // Fehler kommen mit Adresse zurück und landen nicht im Cache.
    let err = f.top8("fehler", "fehler.html", None).unwrap_err();
    assert!(format!("{err:#}").contains("fehler"), "{err:#}");
    assert!(!dir.path("fehler.html").exists());
}
