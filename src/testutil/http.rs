//! Network access for tests: fixed answers per address part, a clock that
//! doesn't really wait, and real (shortened) answers from Scryfall.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::http::{Clock, FetchError, Fetched, Http};

/// Real Scryfall answers, shortened to a few cards (`fixtures/scryfall`).
pub mod scryfall {
    /// Legality search, page 1 of 2 (`has_more`, `next_page` with `page=2`).
    pub const LEGAL_PAGE1: &str = include_str!("fixtures/scryfall/legal_page1.json");
    /// Legality search, last page.
    pub const LEGAL_PAGE2: &str = include_str!("fixtures/scryfall/legal_page2.json");
    /// Error object of a search without hits (HTTP 404).
    pub const NOT_FOUND: &str = include_str!("fixtures/scryfall/not_found.json");
    /// German printing of Shock ("Schock") with `image_uris`.
    pub const SHOCK_DE: &str = include_str!("fixtures/scryfall/shock_de.json");
    /// Double-faced card: images only in `card_faces`.
    pub const FABLE_DE: &str = include_str!("fixtures/scryfall/fable_de.json");
}

/// Answer to a request.
#[derive(Clone)]
pub enum Reply {
    Body(Vec<u8>),
    /// 404 – doesn't exist.
    NotFound,
    /// Network or server error.
    Fail,
    /// HTTP 429 with waiting time in seconds.
    RateLimited(u64),
}

impl Reply {
    pub fn text(s: &str) -> Reply {
        Reply::Body(s.as_bytes().to_vec())
    }
}

/// Answers requests with the first matching rule (address contains the
/// text); without a matching rule: 404. Records all requests.
#[derive(Clone, Default)]
pub struct FakeHttp {
    pub rules: Vec<(&'static str, Reply)>,
    pub calls: Arc<AtomicUsize>,
    /// Requests made: (address, Accept).
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

    /// Requests made (addresses only).
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
    fn get(&self, url: &str, accept: &str) -> Fetched {
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.requests
            .lock()
            .unwrap()
            .push((url.to_string(), accept.to_string()));
        match self.rules.iter().find(|(part, _)| url.contains(part)) {
            Some((_, Reply::Body(b))) => Ok(b.clone()),
            Some((_, Reply::Fail)) => Err(FetchError::Network("kein Netz".into())),
            Some((_, Reply::RateLimited(s))) => Err(FetchError::RateLimited(*s)),
            Some((_, Reply::NotFound)) | None => Err(FetchError::NotFound),
        }
    }
}

/// Clock that only advances the time when waiting and records the waiting times.
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
    /// Let time pass without anyone having waited.
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
