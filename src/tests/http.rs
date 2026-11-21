use super::*;
use crate::testutil::{FakeClock, FakeHttp, Reply};

#[test]
fn hosts_of_urls() {
    assert_eq!(
        host_of("https://api.scryfall.com/cards/search?q=x"),
        "api.scryfall.com"
    );
    assert_eq!(
        host_of("https://www.mtgtop8.com/format?f=ST"),
        "www.mtgtop8.com"
    );
    assert_eq!(host_of("magic.gg/decklists"), "magic.gg");
    assert_eq!(host_of("https://magic.gg?x#y"), "magic.gg");
}

fn polite(clock: &Arc<FakeClock>) -> (Polite<FakeHttp>, FakeHttp) {
    let fake = FakeHttp::new(vec![("", Reply::text("ok"))]);
    let p = Polite::new(fake.clone(), clock.clone(), &GAPS);
    (p, fake)
}

#[test]
fn keeps_gaps_per_host_without_real_waiting() {
    let clock = Arc::new(FakeClock::default());
    let (p, fake) = polite(&clock);
    let top8 = "https://www.mtgtop8.com/a";
    let scry = "https://api.scryfall.com/b";
    p.get(top8, "*/*").unwrap();
    p.get(scry, "*/*").unwrap();
    // The second MTGTop8 request waits 5 s; Scryfall (0.5 s) is free long before.
    p.get(top8, "*/*").unwrap();
    p.get(scry, "*/*").unwrap();
    assert_eq!(clock.sleeps(), vec![Duration::from_secs(5)]);
    // If the time has already passed, there is no waiting.
    clock.advance(Duration::from_secs(10));
    p.get(top8, "*/*").unwrap();
    assert_eq!(clock.sleeps().len(), 1);
    // Unknown hosts (e.g. the image CDN) without a gap.
    p.get("https://cards.scryfall.io/x.jpg", "image/*").unwrap();
    p.get("https://cards.scryfall.io/y.jpg", "image/*").unwrap();
    assert_eq!(clock.sleeps().len(), 1);
    assert_eq!(fake.calls(), 7);
}

#[test]
fn gaps_hold_across_threads() {
    let clock = Arc::new(FakeClock::default());
    let (p, _) = polite(&clock);
    let p: Arc<dyn Http> = Arc::new(p);
    // Two threads (e.g. images and legality) share the Scryfall gap.
    let handles: Vec<_> = (0..2)
        .map(|_| {
            let p = p.clone();
            std::thread::spawn(move || {
                for _ in 0..3 {
                    p.get("https://api.scryfall.com/c", "application/json")
                        .unwrap();
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    // Six requests: all except the first get a slot in the 500 ms grid.
    let waited: Duration = clock.sleeps().iter().sum();
    assert!(waited >= Duration::from_millis(2500), "{waited:?}");
}

#[test]
fn rate_limit_blocks_host_for_all_threads() {
    let clock = Arc::new(FakeClock::default());
    let fake = FakeHttp::new(vec![
        ("api.scryfall.com", Reply::RateLimited(30)),
        ("magic.gg", Reply::text("ok")),
    ]);
    let p = Polite::new(fake.clone(), clock.clone(), &GAPS);
    let scry = "https://api.scryfall.com/cards/search?q=x";
    assert!(matches!(
        p.get(scry, "*/*"),
        Err(FetchError::RateLimited(30))
    ));
    // During the block no request goes out.
    clock.advance(Duration::from_secs(10));
    assert!(matches!(
        p.get(scry, "*/*"),
        Err(FetchError::RateLimited(20))
    ));
    assert_eq!(fake.calls(), 1);
    // Other sites are not affected.
    assert!(p.get("https://magic.gg/decklists", "*/*").is_ok());
    // After it expires, requests are made again.
    clock.advance(Duration::from_secs(21));
    let _ = p.get(scry, "*/*");
    assert_eq!(fake.calls(), 3);
}

#[test]
fn real_clock_moves() {
    let c = SystemClock;
    let before = c.now();
    c.sleep(Duration::from_millis(1));
    assert!(c.now() > before);
}

/// Contract test against the real Scryfall API: only by hand with
/// `cargo test -- --ignored`, so the normal test suite runs without network.
/// Gentle: one search page and two images (about 5 requests, 0.5 s apart)
/// instead of the whole format legality. Reports when the answers change so
/// that the app no longer understands them.
#[test]
#[ignore = "braucht Netz (Scryfall)"]
fn contract_with_real_scryfall() {
    let net = shared();
    let url = "https://api.scryfall.com/cards/search?q=%21%22Shock%22+game%3Aarena&unique=cards";
    let bytes = net
        .get(url, "application/json")
        .unwrap_or_else(|e| panic!("Suche: {e}"));
    let page: serde_json::Value = serde_json::from_slice(&bytes).expect("JSON");
    let (names, next) = crate::meta::parse_scryfall_page(&page);
    assert!(names.iter().any(|n| n == "shock"), "{names:?}");
    assert_eq!(next, None, "eine Seite");
    for key in ["Shock", "Shock@de"] {
        let bytes = crate::images::download(net.as_ref(), key)
            .unwrap_or_else(|e| panic!("{key}: {e}"))
            .expect("Bild vorhanden");
        assert!(
            crate::images::decode(&bytes).is_some(),
            "{key}: kein gültiges Bild"
        );
    }
}

#[test]
fn fetch_errors_read_well() {
    let texts = [
        FetchError::NotFound.user_text(),
        FetchError::Status(503).user_text(),
        FetchError::RateLimited(42).user_text(),
        FetchError::Network("x".into()).user_text(),
    ];
    assert!(
        texts[1].contains("503") && texts[2].contains("42"),
        "{texts:?}"
    );
    assert!(texts.iter().all(|t| !t.contains('%')), "{texts:?}");
    // Technical text stays English.
    assert_eq!(
        FetchError::RateLimited(42).to_string(),
        "rate limited, retry after 42 s"
    );
}
