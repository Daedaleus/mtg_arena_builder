//! Sammlung und Wildcards des Arena-Accounts.
//!
//! Arena schreibt die Sammlung seit 2021 nicht mehr ins Log. Sie wird deshalb
//! (wie bei anderen Trackern) aus dem Speicher des laufenden Spiels gelesen
//! und anschließend lokal zwischengespeichert.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::carddb::CardDb;

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

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Collection {
    /// grpId -> Anzahl
    pub cards: HashMap<u32, u32>,
    pub wildcards: Wildcards,
    pub gold: u32,
    pub gems: u32,
    pub updated_unix: u64,
}

impl Collection {
    pub fn load(path: &Path) -> Option<Collection> {
        crate::paths::read_json(path)
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let json = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        crate::paths::write_atomic(path, &json).map_err(|e| format!("{}: {e}", path.display()))
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
    let err = serde_json::json!({ "error": tr!("collection.plattform_nicht_unterstuetzt") });
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
pub fn read_from_game() -> Result<Collection, String> {
    // Der Speicherleser kann bei unerwarteten Strukturen paniken – abfangen.
    let (cards, inv) = std::thread::spawn(raw_read)
        .join()
        .map_err(|_| tr!("collection.speicher_mtg_arena_konnte").to_string())?;

    from_reader(&cards, &inv, now_unix())
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Baut die Sammlung aus den Antworten des Speicherlesers (Karten und Inventar).
pub fn from_reader(cards: &Value, inv: &Value, now: u64) -> Result<Collection, String> {
    if let Some(e) = err_of(cards) {
        return Err(e);
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
            out.cards.insert(grp, qty);
        }
    }
    if out.cards.is_empty() {
        return Err(tr!("collection.arena_hat_keine_karten"));
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
    match read_from_game().and_then(|c| c.save(out)) {
        Ok(()) => 0,
        Err(e) => {
            let _ = fs::write(&err_path, e);
            1
        }
    }
}

/// Startet diese App erneut mit Administratorrechten (UAC-Abfrage), um die
/// Sammlung zu lesen. Nur unter Windows.
#[cfg(windows)]
pub fn read_elevated(out: &Path) -> Result<Collection, String> {
    use std::os::windows::process::CommandExt;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
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
        .map_err(|e| tr!("collection.powershell_konnte_nicht_gestartet", e = e))?;
    if let Ok(e) = fs::read_to_string(&err_path) {
        return Err(e);
    }
    let after = fs::metadata(out).and_then(|m| m.modified()).ok();
    if !status.success() || after.is_none() || after == before {
        return Err(tr!(
            "collection.lesen_administratorrechten_wurde_abgebrochen"
        ));
    }
    Collection::load(out)
        .ok_or_else(|| tr!("collection.gespeicherte_sammlung_unlesbar").to_string())
}

#[cfg(not(windows))]
pub fn read_elevated(_out: &Path) -> Result<Collection, String> {
    Err(tr!("collection.bitte_app_root_rechten").into())
}

#[cfg(test)]
#[path = "tests/collection.rs"]
mod tests;
