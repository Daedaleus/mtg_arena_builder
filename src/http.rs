//! Network access behind a trait: tournament lists ([`crate::meta`]) and
//! card images ([`crate::images`]) fetch their data through it. In tests a
//! fake replaces the real access – so cache and error cases are testable offline.
//!
//! [`Polite`] keeps the minimum gaps per site (MTGTop8 5 s according to
//! robots.txt, Scryfall 500 ms, magic.gg 2 s). All threads share one
//! instance ([`shared`]), so the gaps apply jointly. The clock can be
//! injected ([`Clock`]), so tests don't really wait.

use std::collections::HashMap;
use std::io::Read;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Waiting time after HTTP 429 without `Retry-After` (seconds).
pub const DEFAULT_RETRY_AFTER: u64 = 60;

/// At most this many bytes per answer.
const MAX_BYTES: u64 = 5_000_000;
/// Identification in requests: program, version and where to find it.
pub const USER_AGENT: &str = concat!(
    "mtg_arena_builder/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/Daedaleus/mtg_arena_builder)"
);

/// Minimum gap between two requests to the same host.
pub const GAPS: [(&str, Duration); 3] = [
    ("www.mtgtop8.com", Duration::from_secs(5)),
    ("magic.gg", Duration::from_secs(2)),
    // Scryfall allows up to 10 requests/s, but throttles the search more strictly.
    ("api.scryfall.com", Duration::from_millis(500)),
];

/// Why a fetch failed. "Doesn't exist" is an answer (remember it),
/// everything else a temporary problem (try again later).
#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("not found (404)")]
    NotFound,
    #[error("HTTP status {0}")]
    Status(u16),
    /// The site throttles (HTTP 429): send no more requests for this long.
    #[error("rate limited, retry after {0} s")]
    RateLimited(u64),
    #[error("network error: {0}")]
    Network(String),
}

impl FetchError {
    /// Message for the interface, in the selected language.
    pub fn user_text(&self) -> String {
        match self {
            FetchError::NotFound => tr!("http.found"),
            FetchError::Status(code) => tr!("http.server_answered_error", code = code),
            FetchError::RateLimited(secs) => tr!("http.too_many_requests_site", secs = secs),
            FetchError::Network(e) => tr!("http.network_error", e = e),
        }
    }
}

pub type Fetched = Result<Vec<u8>, FetchError>;

/// Fetches addresses from the network.
pub trait Http: Send + Sync {
    /// Content of an address (at most 5 MB).
    fn get(&self, url: &str, accept: &str) -> Fetched;
}

impl<T: Http + ?Sized> Http for Arc<T> {
    fn get(&self, url: &str, accept: &str) -> Fetched {
        (**self).get(url, accept)
    }
}

/// Real network access via `ureq`.
pub struct Ureq(ureq::Agent);

impl Ureq {
    pub fn new(timeout: Duration) -> Ureq {
        Ureq(
            ureq::AgentBuilder::new()
                .timeout(timeout)
                .user_agent(USER_AGENT)
                .build(),
        )
    }
}

impl Http for Ureq {
    fn get(&self, url: &str, accept: &str) -> Fetched {
        match self.0.get(url).set("Accept", accept).call() {
            Ok(resp) => {
                let mut bytes = Vec::new();
                resp.into_reader()
                    .take(MAX_BYTES)
                    .read_to_end(&mut bytes)
                    .map_err(|e| FetchError::Network(e.to_string()))?;
                Ok(bytes)
            }
            Err(ureq::Error::Status(404, _)) => Err(FetchError::NotFound),
            Err(ureq::Error::Status(429, resp)) => Err(FetchError::RateLimited(
                resp.header("Retry-After")
                    .and_then(|v| v.trim().parse().ok())
                    .unwrap_or(DEFAULT_RETRY_AFTER),
            )),
            Err(ureq::Error::Status(code, _)) => Err(FetchError::Status(code)),
            Err(e) => Err(FetchError::Network(e.to_string())),
        }
    }
}

/// Clock for waiting times (replaceable in tests).
pub trait Clock: Send + Sync {
    fn now(&self) -> Instant;
    fn sleep(&self, d: Duration);
}

/// The real clock.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn sleep(&self, d: Duration) {
        std::thread::sleep(d);
    }
}

/// Host of an address, e.g. "api.scryfall.com".
pub fn host_of(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    rest.split(['/', '?', '#']).next().unwrap_or("")
}

/// Keeps minimum gaps per host and then passes the request on.
pub struct Polite<H> {
    inner: H,
    clock: Arc<dyn Clock>,
    gaps: Vec<(String, Duration)>,
    /// Earliest time for the next request per host.
    next: Mutex<HashMap<String, Instant>>,
    /// Hosts that throttled (HTTP 429): no request until then.
    blocked: Mutex<HashMap<String, Instant>>,
}

impl<H: Http> Polite<H> {
    pub fn new(inner: H, clock: Arc<dyn Clock>, gaps: &[(&str, Duration)]) -> Polite<H> {
        Polite {
            inner,
            clock,
            gaps: gaps.iter().map(|(h, d)| (h.to_string(), *d)).collect(),
            next: Mutex::new(HashMap::new()),
            blocked: Mutex::new(HashMap::new()),
        }
    }

    /// Remaining block for `host` after throttling (in whole seconds, rounded
    /// up) or `None`.
    fn blocked_for(&self, host: &str) -> Option<u64> {
        let now = self.clock.now();
        let blocked = self.blocked.lock().unwrap_or_else(|e| e.into_inner());
        let until = blocked.get(host).copied().filter(|t| *t > now)?;
        let left = until - now;
        Some(left.as_secs() + u64::from(left.subsec_nanos() > 0))
    }

    fn block(&self, host: &str, secs: u64) {
        let until = self.clock.now() + Duration::from_secs(secs);
        let mut blocked = self.blocked.lock().unwrap_or_else(|e| e.into_inner());
        blocked.insert(host.to_string(), until);
    }

    /// Reserves the next free time for `host` and returns the waiting time
    /// until then. Waiting happens without the lock, so other hosts don't
    /// wait as well.
    fn reserve(&self, host: &str) -> Duration {
        let Some(gap) = self.gaps.iter().find(|(h, _)| h == host).map(|(_, d)| *d) else {
            return Duration::ZERO;
        };
        let now = self.clock.now();
        let mut next = self.next.lock().unwrap_or_else(|e| e.into_inner());
        let slot = next.get(host).copied().filter(|t| *t > now).unwrap_or(now);
        next.insert(host.to_string(), slot + gap);
        slot - now
    }
}

impl<H: Http> Http for Polite<H> {
    /// During throttling no request is made at all – not from other threads
    /// either –, `RateLimited` is returned right away.
    fn get(&self, url: &str, accept: &str) -> Fetched {
        let host = host_of(url);
        if let Some(left) = self.blocked_for(host) {
            return Err(FetchError::RateLimited(left));
        }
        let wait = self.reserve(host);
        if !wait.is_zero() {
            self.clock.sleep(wait);
        }
        let result = self.inner.get(url, accept);
        if let Err(FetchError::RateLimited(secs)) = &result {
            self.block(host, *secs);
        }
        result
    }
}

/// The app's shared network access: real, with minimum gaps.
pub fn shared() -> Arc<dyn Http> {
    Arc::new(Polite::new(
        Ureq::new(Duration::from_secs(30)),
        Arc::new(SystemClock),
        &GAPS,
    ))
}

#[cfg(test)]
#[path = "tests/http.rs"]
mod tests;
