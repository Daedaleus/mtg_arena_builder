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
