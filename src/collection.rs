//! Sammlung und Wildcards des Arena-Accounts.
//!
//! Arena schreibt die Sammlung seit 2021 nicht mehr ins Log. Sie wird deshalb
//! (wie bei anderen Trackern) aus dem Speicher des laufenden Spiels gelesen
//! und anschließend lokal zwischengespeichert.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::carddb::{CardDb, GrpId};

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct Wildcards {
    pub common: u32,
    pub uncommon: u32,
    pub rare: u32,
    pub mythic: u32,
}

impl Wildcards {
    /// Index 0..4 = Common, Uncommon, Rare, Mythic.
    pub fn as_array(&self) -> [u32; 4] {
        [self.common, self.uncommon, self.rare, self.mythic]
    }
}

/// Aktuelle Version von `collection.json`. Wird sie erhöht, passt
/// [`Collection::migrate`] ältere Dateien an.
pub const COLLECTION_VERSION: u32 = 1;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Collection {
    /// Version der Datei (0 = vor Einführung der Versionsnummer).
    #[serde(default)]
    pub version: u32,
    /// grpId -> Anzahl
    pub cards: HashMap<GrpId, u32>,
    pub wildcards: Wildcards,
    pub gold: u32,
    pub gems: u32,
    pub updated_unix: u64,
}

impl Collection {
    pub fn load(path: &Path) -> Option<Collection> {
        crate::paths::read_json::<Collection>(path).map(Collection::migrate)
    }

    /// Bringt eine Sammlung aus einer älteren Datei auf den aktuellen Stand.
    /// Version 0 (ohne Nummer) hat dieselben Felder – nur die Nummer fehlt.
    pub fn migrate(mut self) -> Collection {
        self.version = COLLECTION_VERSION;
        self
    }

    /// Schreibt die Sammlung immer mit der aktuellen Versionsnummer.
    pub fn save(&self, path: &Path) -> Result<()> {
        let current = Collection {
            version: COLLECTION_VERSION,
            ..self.clone()
        };
        let json = serde_json::to_vec(&current)?;
        crate::paths::write_atomic(path, &json).with_context(|| path.display().to_string())
    }

    /// Besitz je Kartenname (alle Drucke zusammengezählt).
    pub fn owned_by_name(&self, db: &CardDb) -> HashMap<String, u32> {
        let mut out = HashMap::new();
        for key in db.names() {
            let n: u32 = db
                .printings(key)
                .iter()
                .filter_map(|g| self.cards.get(g))
                .sum();
            if n > 0 {
                out.insert(key.clone(), n);
            }
        }
        out
    }

    pub fn total_cards(&self) -> u32 {
        self.cards.values().sum()
    }

    pub fn age_text(&self) -> String {
        crate::present::ago(self.updated_unix * 1000)
    }
}

/// Warum die Sammlung nicht gelesen werden konnte. Der `Display`-Text ist
/// technisch (englisch, für Logs und den Admin-Hilfsprozess); für die
/// Oberfläche gibt es [`CollectionError::user_text`] in der eingestellten Sprache.
#[derive(Debug, thiserror::Error)]
pub enum CollectionError {
    #[error("MTG Arena is not running")]
    NotRunning,
    #[error("no access to the Arena process")]
    AccessDenied,
    #[error("Arena is not in the main menu")]
    NotInMenu,
    #[error("Arena returned no cards")]
    NoCards,
    #[error("the memory reader crashed")]
    ReaderPanicked,
    #[error("platform not supported")]
    Unsupported,
    /// Sonstige Meldung des Speicherlesers.
    #[error("memory reader: {0}")]
    Reader(String),
    #[error("reading with administrator rights was cancelled")]
    ElevationCancelled,
    /// Der Hilfsprozess mit Administratorrechten meldete einen Fehler.
    #[error("elevated helper: {0}")]
    Helper(String),
    #[error("could not start the elevated helper: {0}")]
    HelperStart(std::io::Error),
    #[error("collection file unreadable")]
    SavedUnreadable,
    #[error("could not save the collection: {0}")]
    Save(String),
}

impl CollectionError {
    /// Ordnet eine Meldung des Speicherlesers (mtga-reader, englisch) einer
    /// Fehlerart zu. Nur hier werden dessen Texte ausgewertet.
    pub fn from_reader(text: &str) -> CollectionError {
        let t = text.to_lowercase();
        if t.contains("run elevated")
            || t.contains("could not open")
            || t.contains("process not found")
        {
            CollectionError::AccessDenied
        } else if t.contains("home screen") || t.contains("instance is null") {
            CollectionError::NotInMenu
        } else if t.contains("not supported") || t.contains("not yet implemented") {
            CollectionError::Unsupported
        } else {
            CollectionError::Reader(text.to_string())
        }
    }

    /// Lohnt ein zweiter Versuch mit Administratorrechten? Nur, wenn der
    /// Zugriff verweigert wurde oder der Grund unklar ist – nicht bei
    /// „nicht im Hauptmenü“ oder „keine Karten“.
    pub fn needs_elevation(&self) -> bool {
        matches!(
            self,
            CollectionError::AccessDenied | CollectionError::Reader(_)
        )
    }

    /// Meldung für die Oberfläche, in der eingestellten Sprache.
    pub fn user_text(&self) -> String {
        match self {
            CollectionError::NotRunning => tr!("state.mtg_arena_running_please"),
            CollectionError::AccessDenied => tr!("collection.no_access_arena_process"),
            CollectionError::NotInMenu => tr!("collection.arena_main_menu_please"),
            CollectionError::NoCards => tr!("collection.arena_returned_no_cards"),
            CollectionError::ReaderPanicked => tr!("collection.could_read_mtg_arenas"),
            CollectionError::Unsupported => tr!("collection.platform_supported"),
            CollectionError::Reader(e) => tr!("collection.memory_reader_reports", e = e),
            CollectionError::ElevationCancelled => {
                tr!("collection.reading_administrator_rights_cancelled")
            }
            CollectionError::Helper(e) => {
                tr!("collection.reading_administrator_rights_failed", e = e)
            }
            CollectionError::HelperStart(e) => {
                tr!("collection.could_start_powershell", e = e)
            }
            CollectionError::SavedUnreadable => tr!("collection.saved_collection_unreadable"),
            CollectionError::Save(e) => tr!("collection.collection_read_but_saved", e = e),
        }
    }
}

/// Ergebnis von [`sync`]: die gelesene Sammlung und ggf. ein Fehler beim
/// Speichern (die Sammlung ist trotzdem gelesen).
#[derive(Debug)]
pub struct Synced {
    pub collection: Collection,
    pub save_error: Option<CollectionError>,
}

const PROCESS: &str = "MTGA";

#[cfg(any(target_os = "windows", target_os = "linux"))]
fn raw_read() -> (Value, Value) {
    (
        mtga_reader::read_collection(PROCESS.to_string()),
        mtga_reader::read_inventory(PROCESS.to_string()),
    )
}

#[cfg(target_os = "macos")]
fn raw_read() -> (Value, Value) {
    (
        mtga_reader::read_collection(PROCESS),
        mtga_reader::read_inventory(PROCESS),
    )
}

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
fn raw_read() -> (Value, Value) {
    let err = serde_json::json!({ "error": "Platform not supported" });
    (err.clone(), err)
}

pub fn is_admin() -> bool {
    mtga_reader::api::is_admin()
}

fn err_of(v: &Value) -> Option<String> {
    v.get("error").map(|e| {
        e.as_str()
            .map(str::to_string)
            .unwrap_or_else(|| e.to_string())
    })
}

/// Liest Sammlung und Inventar aus dem laufenden Arena-Prozess.
pub fn read_from_game() -> Result<Collection, CollectionError> {
    // Der Speicherleser kann bei unerwarteten Strukturen paniken – abfangen.
    let (cards, inv) = std::thread::spawn(raw_read)
        .join()
        .map_err(|_| CollectionError::ReaderPanicked)?;

    from_reader(&cards, &inv, now_unix())
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Baut die Sammlung aus den Antworten des Speicherlesers (Karten und Inventar).
pub fn from_reader(cards: &Value, inv: &Value, now: u64) -> Result<Collection, CollectionError> {
    if let Some(e) = err_of(cards) {
        return Err(CollectionError::from_reader(&e));
    }
    let mut out = Collection::default();
    for c in cards
        .get("cards")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let grp = c.get("grpId").and_then(Value::as_u64).unwrap_or(0) as u32;
        let qty = c
            .get("qty")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .clamp(0, 99) as u32;
        if grp > 0 && qty > 0 {
            out.cards.insert(GrpId(grp), qty);
        }
    }
    if out.cards.is_empty() {
        return Err(CollectionError::NoCards);
    }
    if err_of(inv).is_none() {
        let num = |v: &Value, k: &str| v.get(k).and_then(Value::as_i64).unwrap_or(0).max(0) as u32;
        let wc = inv.get("wildcards").cloned().unwrap_or(Value::Null);
        out.wildcards = Wildcards {
            common: num(&wc, "common"),
            uncommon: num(&wc, "uncommon"),
            rare: num(&wc, "rare"),
            mythic: num(&wc, "mythic"),
        };
        out.gold = num(inv, "gold");
        out.gems = num(inv, "gems");
    }
    out.updated_unix = now;
    Ok(out)
}

/// Hilfsmodus `--sync-collection <datei>`: liest die Sammlung und schreibt sie
/// als JSON. Wird von der Oberfläche mit Administratorrechten gestartet.
pub fn run_sync_helper(out: &Path) -> i32 {
    let err_path = out.with_extension("error.txt");
    let _ = fs::remove_file(&err_path);
    let result = read_from_game().and_then(|c| {
        c.save(out)
            .map_err(|e| CollectionError::Save(format!("{e:#}")))
    });
    match result {
        Ok(()) => 0,
        Err(e) => {
            // Technischer Text; der Aufrufer zeigt ihn als „Helfer meldet …“ an.
            let _ = fs::write(&err_path, e.to_string());
            1
        }
    }
}

/// Startet diese App erneut mit Administratorrechten (UAC-Abfrage), um die
/// Sammlung zu lesen. Nur unter Windows.
#[cfg(windows)]
pub fn read_elevated(out: &Path) -> Result<Collection, CollectionError> {
    use std::os::windows::process::CommandExt;
    let exe = std::env::current_exe().map_err(CollectionError::HelperStart)?;
    let err_path = out.with_extension("error.txt");
    let _ = fs::remove_file(&err_path);
    let before = fs::metadata(out).and_then(|m| m.modified()).ok();
    let quote = |p: &Path| p.display().to_string().replace('\'', "''");
    let script = format!(
        "Start-Process -FilePath '{}' -ArgumentList '--sync-collection','\"{}\"' -Verb RunAs -Wait",
        quote(&exe),
        quote(out)
    );
    let status = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .status()
        .map_err(CollectionError::HelperStart)?;
    if let Ok(e) = fs::read_to_string(&err_path) {
        return Err(CollectionError::Helper(e));
    }
    let after = fs::metadata(out).and_then(|m| m.modified()).ok();
    if !status.success() || after.is_none() || after == before {
        return Err(CollectionError::ElevationCancelled);
    }
    Collection::load(out).ok_or(CollectionError::SavedUnreadable)
}

#[cfg(not(windows))]
pub fn read_elevated(_out: &Path) -> Result<Collection, CollectionError> {
    Err(CollectionError::AccessDenied)
}

/// Liest die Sammlung aus dem laufenden Arena und speichert sie unter `path`.
/// Läuft Arena mit höheren Rechten, wird unter Windows einmalig per UAC
/// nachgefragt. Unter Linux gibt es keine Prozesserkennung – dort wird
/// direkt gelesen (Fehlermeldung, falls Arena nicht läuft).
pub fn sync(path: &Path) -> Result<Synced, CollectionError> {
    let running = cfg!(target_os = "linux") || mtga_reader::api::find_process(PROCESS);
    if !running {
        return Err(CollectionError::NotRunning);
    }
    let collection = match read_from_game() {
        Ok(c) => c,
        // Arena läuft mit höheren Rechten: einmalig per UAC nachfragen.
        Err(first) if cfg!(windows) && first.needs_elevation() && !is_admin() => {
            read_elevated(path)?
        }
        Err(first) => return Err(first),
    };
    let save_error = collection
        .save(path)
        .err()
        .map(|e| CollectionError::Save(format!("{e:#}")));
    Ok(Synced {
        collection,
        save_error,
    })
}

#[cfg(test)]
#[path = "tests/collection.rs"]
mod tests;
