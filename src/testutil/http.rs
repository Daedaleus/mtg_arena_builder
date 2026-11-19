//! Netzabruf für Tests: feste Antworten je Adressteil, eine Uhr, die nicht
//! wirklich wartet, und echte (gekürzte) Antworten von Scryfall.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::http::{Clock, Http};

/// Echte Scryfall-Antworten, auf wenige Karten gekürzt (`fixtures/scryfall`).
pub mod scryfall {
    /// Legalitätssuche, Seite 1 von 2 (`has_more`, `next_page` mit `page=2`).
    pub const LEGAL_PAGE1: &str = include_str!("fixtures/scryfall/legal_page1.json");
    /// Legalitätssuche, letzte Seite.
    pub const LEGAL_PAGE2: &str = include_str!("fixtures/scryfall/legal_page2.json");
    /// Fehlerobjekt einer Suche ohne Treffer (HTTP 404).
    pub const NOT_FOUND: &str = include_str!("fixtures/scryfall/not_found.json");
    /// Deutscher Druck von Shock („Schock“) mit `image_uris`.
    pub const SHOCK_DE: &str = include_str!("fixtures/scryfall/shock_de.json");
    /// Doppelseitige Karte: Bilder nur in `card_faces`.
    pub const FABLE_DE: &str = include_str!("fixtures/scryfall/fable_de.json");
}

/// Antwort auf eine Anfrage.
#[derive(Clone)]
pub enum Reply {
    Body(Vec<u8>),
    /// 404 – gibt es nicht.
    NotFound,
    /// Netz- oder Serverfehler.
    Fail,
}

impl Reply {
    pub fn text(s: &str) -> Reply {
        Reply::Body(s.as_bytes().to_vec())
    }
}

/// Beantwortet Anfragen nach der ersten passenden Regel (Adresse enthält den
/// Text); ohne passende Regel: 404. Schreibt alle Anfragen mit.
#[derive(Clone, Default)]
pub struct FakeHttp {
    pub rules: Vec<(&'static str, Reply)>,
    pub calls: Arc<AtomicUsize>,
    /// Gestellte Anfragen: (Adresse, Accept).
    pub requests: Arc<Mutex<Vec<(String, String)>>>,
}

impl FakeHttp {
    pub fn new(rules: Vec<(&'static str, Reply)>) -> FakeHttp {
        FakeHttp {
            rules,
            ..Default::default()
        }
    }

    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::Relaxed)
    }

    /// Gestellte Anfragen (nur die Adressen).
    pub fn urls(&self) -> Vec<String> {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .map(|(u, _)| u.clone())
            .collect()
    }
}

impl Http for FakeHttp {
    fn get(&self, url: &str, accept: &str) -> anyhow::Result<Option<Vec<u8>>> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.requests
            .lock()
            .unwrap()
            .push((url.to_string(), accept.to_string()));
        match self.rules.iter().find(|(part, _)| url.contains(part)) {
            Some((_, Reply::Body(b))) => Ok(Some(b.clone())),
            Some((_, Reply::Fail)) => anyhow::bail!("kein Netz"),
            Some((_, Reply::NotFound)) | None => Ok(None),
        }
    }
}

/// Uhr, die beim Warten nur die Zeit weiterstellt und die Wartezeiten mitschreibt.
pub struct FakeClock {
    start: Instant,
    elapsed: Mutex<Duration>,
    pub sleeps: Mutex<Vec<Duration>>,
}

impl Default for FakeClock {
    fn default() -> FakeClock {
        FakeClock {
            start: Instant::now(),
            elapsed: Mutex::new(Duration::ZERO),
            sleeps: Mutex::new(Vec::new()),
        }
    }
}

impl FakeClock {
    /// Zeit vergehen lassen, ohne dass jemand gewartet hat.
    pub fn advance(&self, d: Duration) {
        *self.elapsed.lock().unwrap() += d;
    }

    pub fn sleeps(&self) -> Vec<Duration> {
        self.sleeps.lock().unwrap().clone()
    }
}

impl Clock for FakeClock {
    fn now(&self) -> Instant {
        self.start + *self.elapsed.lock().unwrap()
    }

    fn sleep(&self, d: Duration) {
        self.sleeps.lock().unwrap().push(d);
        self.advance(d);
    }
}
