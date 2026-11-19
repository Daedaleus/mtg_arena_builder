use super::*;
use crate::testutil::TempDir;

/// Kleines, gültiges PNG (2×1 Pixel).
fn png() -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(2, 1, image::Rgba([10, 20, 30, 255]));
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

#[test]
fn file_names_and_urls() {
    assert_eq!(file_stem("Urza's Saga"), "urza_s_saga");
    assert_eq!(file_stem("Fire // Ice"), "fire____ice");
    assert_eq!(
        image_url("exact", "Fire // Ice"),
        "https://api.scryfall.com/cards/named?exact=Fire%20%2F%2F%20Ice&format=image&version=normal"
    );
    assert!(image_url("fuzzy", "Bolt-1_a.b").contains("fuzzy=Bolt-1_a.b&"));
}

#[test]
fn keys_and_localized_images() {
    assert_eq!(key("Shock", Locale::English), "Shock");
    assert_eq!(key("Shock", Locale::German), "Shock@de");
    assert_eq!(split_key("Shock@de"), ("Shock", Some("de")));
    assert_eq!(split_key("Fire // Ice"), ("Fire // Ice", None));
    // Eigene Cache-Datei je Sprache.
    assert_ne!(file_stem(&key("Shock", Locale::German)), file_stem("Shock"));
    assert_eq!(
        search_url("Shock", "de"),
        "https://api.scryfall.com/cards/search?q=%21%22Shock%22%20lang%3Ade&unique=prints"
    );
    let single = r#"{"data":[{"image_uris":{"normal":"https://img/de.jpg"}}]}"#;
    assert_eq!(
        image_from_search(single).as_deref(),
        Some("https://img/de.jpg")
    );
    let faces =
        r#"{"data":[{"card_faces":[{"image_uris":{"normal":"https://img/front.jpg"}},{}]}]}"#;
    assert_eq!(
        image_from_search(faces).as_deref(),
        Some("https://img/front.jpg")
    );
    assert_eq!(image_from_search(r#"{"object":"error"}"#), None);
    assert_eq!(image_from_search("kein json"), None);
}

#[test]
fn decodes_images() {
    let img = decode(&png()).unwrap();
    assert_eq!(img.size, [2, 1]);
    assert!(decode(b"kein Bild").is_none());
}

#[test]
fn cache_hits_misses_and_repairs() {
    let dir = TempDir::new("img");
    assert!(matches!(from_cache(&dir.0, "Shock"), Cached::Unknown));
    // Heruntergeladenes Bild wird abgelegt und wiedergefunden.
    assert!(store(&dir.0, "Shock", Some(&png())).is_some());
    assert!(matches!(from_cache(&dir.0, "Shock"), Cached::Image(_)));
    // Kaputte Antwort wird nicht gespeichert.
    assert!(store(&dir.0, "Opt", Some(b"<html>")).is_none());
    assert!(!dir.path("opt.jpg").exists());
    // Kein Bild bei Scryfall: gemerkt.
    assert!(store(&dir.0, "Unbekannt", None).is_none());
    assert!(matches!(from_cache(&dir.0, "Unbekannt"), Cached::Missing));
    // Beschädigte Cache-Datei wird verworfen.
    std::fs::write(dir.path("bad.jpg"), b"defekt").unwrap();
    assert!(matches!(from_cache(&dir.0, "Bad"), Cached::Unknown));
    assert!(!dir.path("bad.jpg").exists());
}

#[test]
fn worker_answers_from_cache() {
    let dir = TempDir::new("img");
    store(&dir.0, "Shock", Some(&png()));
    store(&dir.0, "Nichts", None);
    let (done_tx, done_rx) = channel();
    let tx = spawn(dir.0.clone(), move |name, img| {
        let _ = done_tx.send((name, img.is_some()));
    });
    tx.send("Shock".into()).unwrap();
    tx.send("Nichts".into()).unwrap();
    let wait = Duration::from_secs(5);
    assert_eq!(done_rx.recv_timeout(wait).unwrap(), ("Shock".into(), true));
    assert_eq!(
        done_rx.recv_timeout(wait).unwrap(),
        ("Nichts".into(), false)
    );
}

#[test]
fn downloads_localized_with_fallback() {
    use crate::testutil::{FakeHttp, Reply};
    let search = br#"{"data":[{"image_uris":{"normal":"https://img/schock.jpg"}}]}"#.to_vec();
    let http = FakeHttp::new(vec![
        ("lang%3Ade", Reply::Body(search)),
        ("img/schock", Reply::Body(b"DE".to_vec())),
        ("exact=Shock", Reply::Body(b"EN".to_vec())),
        ("Opt", Reply::Fail),
    ]);
    assert_eq!(download(&http, "Shock@de").unwrap().unwrap(), b"DE");
    assert_eq!(download(&http, "Shock").unwrap().unwrap(), b"EN");
    // Kein deutscher Druck: englisches Bild; gar keins: None.
    let http = FakeHttp::new(vec![("exact=Shock", Reply::Body(b"EN".to_vec()))]);
    assert_eq!(download(&http, "Shock@de").unwrap().unwrap(), b"EN");
    assert!(download(&http, "Unbekannt").unwrap().is_none());
    // Netzfehler: nicht als „fehlt“ merken.
    let dir = TempDir::new("img_fail");
    let failing = FakeHttp::new(vec![("Opt", Reply::Fail)]);
    assert!(load(&failing, &dir.0, "Opt").is_none());
    assert!(matches!(from_cache(&dir.0, "Opt"), Cached::Unknown));
    // Gibt es nicht: gemerkt.
    let empty = FakeHttp::new(vec![]);
    assert!(load(&empty, &dir.0, "Opt").is_none());
    assert!(matches!(from_cache(&dir.0, "Opt"), Cached::Missing));
}
