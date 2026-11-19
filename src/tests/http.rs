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
    // Zweite MTGTop8-Anfrage wartet 5 s; Scryfall danach ist längst frei.
    p.get(top8, "*/*").unwrap();
    p.get(scry, "*/*").unwrap();
    assert_eq!(clock.sleeps(), vec![Duration::from_secs(5)]);
    // Ist die Zeit schon vergangen, wird nicht gewartet.
    clock.advance(Duration::from_secs(10));
    p.get(top8, "*/*").unwrap();
    assert_eq!(clock.sleeps().len(), 1);
    // Unbekannte Hosts (z. B. das Bild-CDN) ohne Abstand.
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
    // Zwei Threads (z. B. Bilder und Legalität) teilen sich den Scryfall-Abstand.
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
    // Sechs Anfragen: alle außer der ersten haben einen Platz im 100-ms-Raster.
    let waited: Duration = clock.sleeps().iter().sum();
    assert!(waited >= Duration::from_millis(500), "{waited:?}");
}

#[test]
fn real_clock_moves() {
    let c = SystemClock;
    let before = c.now();
    c.sleep(Duration::from_millis(1));
    assert!(c.now() > before);
}

/// Vertragstest gegen die echte Scryfall-API: nur von Hand mit
/// `cargo test -- --ignored`, damit die normale Testsuite ohne Netz läuft.
/// Meldet, wenn sich die Antworten so ändern, dass die App sie nicht mehr versteht.
#[test]
#[ignore = "braucht Netz (Scryfall)"]
fn contract_with_real_scryfall() {
    let dir = crate::testutil::TempDir::new("contract");
    let net = shared();
    let fetcher = crate::meta::Fetcher::with_http(dir.0.clone(), net.clone());
    let legal = fetcher.legal_names("standard").expect("Legalität");
    assert!(legal.len() > 500, "{} Karten", legal.len());
    for key in ["Shock", "Shock@de"] {
        let bytes = crate::images::download(net.as_ref(), key)
            .expect("Abruf")
            .expect("Bild vorhanden");
        assert!(
            crate::images::decode(&bytes).is_some(),
            "{key}: kein gültiges Bild"
        );
    }
}
