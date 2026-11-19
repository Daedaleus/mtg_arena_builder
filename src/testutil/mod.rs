//! Hilfen für Tests: kleine Kartendatenbanken, künstliche Arena-Lognachrichten,
//! Webseiten-Ausschnitte, ein Netzabruf ohne Netz und temporäre Ordner.

mod cards;
mod http;
mod log;
mod pages;

pub use crate::carddb::GrpId;
pub use cards::*;
pub use http::*;
pub use log::*;
pub use pages::*;

/// Temporäres Verzeichnis, das am Ende des Tests gelöscht wird.
pub struct TempDir(pub std::path::PathBuf);

impl TempDir {
    pub fn new(tag: &str) -> TempDir {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "mtgab_test_{tag}_{}_{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }

    pub fn path(&self, name: &str) -> std::path::PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
