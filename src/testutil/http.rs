//! Netzabruf für Tests: feste Antworten je Adressteil, zählt die Abrufe.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::http::Http;

/// Antwort auf eine Anfrage.
#[derive(Clone)]
pub enum Reply {
    Body(Vec<u8>),
    /// 404 – gibt es nicht.
    NotFound,
    /// Netz- oder Serverfehler.
    Fail,
}

/// Beantwortet Anfragen nach der ersten passenden Regel (Adresse enthält den
/// Text); ohne passende Regel: 404.
#[derive(Clone, Default)]
pub struct FakeHttp {
    pub rules: Vec<(&'static str, Reply)>,
    pub calls: Arc<AtomicUsize>,
}

impl FakeHttp {
    pub fn new(rules: Vec<(&'static str, Reply)>) -> FakeHttp {
        FakeHttp {
            rules,
            calls: Arc::default(),
        }
    }

    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::Relaxed)
    }
}

impl Http for FakeHttp {
    fn get(&self, url: &str, _accept: &str) -> anyhow::Result<Option<Vec<u8>>> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        match self.rules.iter().find(|(part, _)| url.contains(part)) {
            Some((_, Reply::Body(b))) => Ok(Some(b.clone())),
            Some((_, Reply::Fail)) => anyhow::bail!("kein Netz"),
            Some((_, Reply::NotFound)) | None => Ok(None),
        }
    }
}
