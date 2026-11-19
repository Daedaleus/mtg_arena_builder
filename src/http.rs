//! Netzabruf hinter einem Trait: Turnierlisten ([`crate::meta`]) und
//! Kartenbilder ([`crate::images`]) holen ihre Daten darüber. In Tests ersetzt
//! ein Fake den echten Abruf – so sind Cache und Fehlerfälle ohne Netz prüfbar.
//!
//! [`Polite`] hält die Mindestabstände je Seite ein (MTGTop8 5 s laut
//! robots.txt, Scryfall 100 ms, magic.gg 2 s). Alle Threads teilen sich eine
//! Instanz ([`shared`]), damit die Abstände auch gemeinsam gelten. Die Uhr ist
//! einschleusbar ([`Clock`]), Tests warten also nicht wirklich.

use std::collections::HashMap;
use std::io::Read;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Höchstens so viele Bytes je Antwort.
const MAX_BYTES: u64 = 5_000_000;
pub const USER_AGENT: &str = "mtg_arena_builder/0.1 (privates Deckbau-Werkzeug)";

/// Mindestabstand zwischen zwei Anfragen an denselben Host.
pub const GAPS: [(&str, Duration); 3] = [
    ("www.mtgtop8.com", Duration::from_secs(5)),
    ("magic.gg", Duration::from_secs(2)),
    ("api.scryfall.com", Duration::from_millis(100)),
];

/// Warum ein Abruf scheiterte. „Gibt es nicht“ ist eine Antwort (merken),
/// alles andere ein vorübergehendes Problem (später erneut versuchen).
#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("not found (404)")]
    NotFound,
    #[error("HTTP status {0}")]
    Status(u16),
    #[error("network error: {0}")]
    Network(String),
}

pub type Fetched = Result<Vec<u8>, FetchError>;

/// Holt Adressen aus dem Netz.
pub trait Http: Send + Sync {
    /// Inhalt einer Adresse (höchstens 5 MB).
    fn get(&self, url: &str, accept: &str) -> Fetched;
}

impl<T: Http + ?Sized> Http for Arc<T> {
    fn get(&self, url: &str, accept: &str) -> Fetched {
        (**self).get(url, accept)
    }
}

/// Echter Netzabruf über `ureq`.
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
            Err(ureq::Error::Status(code, _)) => Err(FetchError::Status(code)),
            Err(e) => Err(FetchError::Network(e.to_string())),
        }
    }
}

/// Uhr für Wartezeiten (in Tests ersetzbar).
pub trait Clock: Send + Sync {
    fn now(&self) -> Instant;
    fn sleep(&self, d: Duration);
}

/// Die echte Uhr.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn sleep(&self, d: Duration) {
        std::thread::sleep(d);
    }
}

/// Host einer Adresse, z. B. „api.scryfall.com“.
pub fn host_of(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    rest.split(['/', '?', '#']).next().unwrap_or("")
}

/// Hält Mindestabstände je Host ein und reicht die Anfrage dann weiter.
pub struct Polite<H> {
    inner: H,
    clock: Arc<dyn Clock>,
    gaps: Vec<(String, Duration)>,
    /// Frühester Zeitpunkt für die nächste Anfrage je Host.
    next: Mutex<HashMap<String, Instant>>,
}

impl<H: Http> Polite<H> {
    pub fn new(inner: H, clock: Arc<dyn Clock>, gaps: &[(&str, Duration)]) -> Polite<H> {
        Polite {
            inner,
            clock,
            gaps: gaps.iter().map(|(h, d)| (h.to_string(), *d)).collect(),
            next: Mutex::new(HashMap::new()),
        }
    }

    /// Reserviert den nächsten freien Zeitpunkt für `host` und liefert die
    /// Wartezeit bis dahin. Gewartet wird ohne Sperre, damit andere Hosts
    /// nicht mitwarten.
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
    fn get(&self, url: &str, accept: &str) -> Fetched {
        let wait = self.reserve(host_of(url));
        if !wait.is_zero() {
            self.clock.sleep(wait);
        }
        self.inner.get(url, accept)
    }
}

/// Der gemeinsame Netzabruf der App: echt, mit Mindestabständen.
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
