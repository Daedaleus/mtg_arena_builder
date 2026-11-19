//! Netzabruf hinter einem Trait: Turnierlisten ([`crate::meta`]) und
//! Kartenbilder ([`crate::images`]) holen ihre Daten darüber. In Tests ersetzt
//! ein Fake den echten Abruf – so sind Cache und Fehlerfälle ohne Netz prüfbar.

use std::io::Read;
use std::time::Duration;

use anyhow::Result;

/// Höchstens so viele Bytes je Antwort.
const MAX_BYTES: u64 = 5_000_000;
pub const USER_AGENT: &str = "mtg_arena_builder/0.1 (privates Deckbau-Werkzeug)";

/// Holt Adressen aus dem Netz.
pub trait Http: Send {
    /// Inhalt einer Adresse; `Ok(None)` = gibt es nicht (404), `Err` =
    /// Netz- oder Serverfehler (später erneut versuchen).
    fn get(&self, url: &str, accept: &str) -> Result<Option<Vec<u8>>>;
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
    fn get(&self, url: &str, accept: &str) -> Result<Option<Vec<u8>>> {
        match self.0.get(url).set("Accept", accept).call() {
            Ok(resp) => {
                let mut bytes = Vec::new();
                resp.into_reader().take(MAX_BYTES).read_to_end(&mut bytes)?;
                Ok(Some(bytes))
            }
            Err(ureq::Error::Status(404, _)) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}
