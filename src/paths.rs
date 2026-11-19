//! Orte von MTG-Arena-Dateien (Log, Kartendatenbank) und der eigenen App-Daten.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use directories::{BaseDirs, ProjectDirs};

/// Verzeichnis für Einstellungen, Sammlung und Web-Cache dieser App.
pub fn data_dir() -> PathBuf {
    let dir = ProjectDirs::from("", "", "mtg_arena_builder")
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let _ = fs::create_dir_all(&dir);
    dir
}

/// Schreibt eine Datei so, dass sie nie halb geschrieben zurückbleibt
/// (erst in eine temporäre Datei, dann umbenennen).
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)
}

/// Liest eine JSON-Datei; ein UTF-8-BOM am Anfang (z. B. von Windows-Editoren) wird ignoriert.
pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    let bytes = fs::read(path).ok()?;
    let body = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
    serde_json::from_slice(body).ok()
}

/// Mögliche Orte der `Player.log` je nach Plattform.
pub fn player_log_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Some(base) = BaseDirs::new() else {
        return out;
    };
    let home = base.home_dir();
    if cfg!(windows) {
        out.push(home.join("AppData/LocalLow/Wizards Of The Coast/MTGA/Player.log"));
    } else if cfg!(target_os = "macos") {
        out.push(home.join("Library/Logs/Wizards Of The Coast/MTGA/Player.log"));
    } else {
        for steam in [".steam/steam", ".local/share/Steam"] {
            out.push(home.join(steam).join(
                "steamapps/compatdata/2141910/pfx/drive_c/users/steamuser/AppData/LocalLow/Wizards Of The Coast/MTGA/Player.log",
            ));
        }
    }
    out
}

/// Vorhandene Arena-Logs (aktuelle und vorige Sitzung).
pub fn player_logs() -> Vec<PathBuf> {
    player_log_candidates()
        .into_iter()
        .flat_map(|p| [p.with_file_name("Player-prev.log"), p])
        .filter(|p| p.is_file())
        .collect()
}

/// Liest das Installationsverzeichnis aus dem Kopf der Player.log
/// (`Mono path[0] = 'D:/MTG/MTGA/MTGA_Data/Managed'`).
fn data_dir_from_log() -> Option<PathBuf> {
    for log in player_log_candidates() {
        let Ok(bytes) = fs::read(&log) else { continue };
        let head = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]).to_string();
        if let Some(p) = install_from_log_head(&head).filter(|p| p.is_dir()) {
            return Some(p);
        }
    }
    None
}

/// `MTGA_Data`-Verzeichnis aus der Zeile `Mono path[0] = '…/MTGA_Data/Managed'`.
fn install_from_log_head(head: &str) -> Option<PathBuf> {
    head.lines().find_map(|line| {
        let rest = line.strip_prefix("Mono path[0] = '")?;
        let path = rest.trim_end_matches('\'').trim_end_matches("/Managed");
        Some(PathBuf::from(path))
    })
}

/// Kandidaten für `MTGA_Data` (bzw. das Verzeichnis, das `Downloads/Raw` enthält).
fn mtga_data_candidates(user_override: Option<&str>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(o) = user_override.filter(|s| !s.trim().is_empty()) {
        let p = PathBuf::from(o.trim());
        out.push(p.join("MTGA_Data"));
        out.push(p);
    }
    if let Some(p) = data_dir_from_log() {
        out.push(p);
    }
    if cfg!(windows) {
        for drive in 'C'..='Z' {
            for sub in [
                "Program Files/Wizards of the Coast/MTGA",
                "Program Files (x86)/Wizards of the Coast/MTGA",
                "Program Files (x86)/Steam/steamapps/common/MTGA",
                "SteamLibrary/steamapps/common/MTGA",
                "Program Files/Epic Games/MagicTheGathering",
                "MTG/MTGA",
                "MTGA",
                "Games/MTGA",
            ] {
                out.push(PathBuf::from(format!("{drive}:/{sub}/MTGA_Data")));
            }
        }
    } else if let Some(base) = BaseDirs::new() {
        let home = base.home_dir();
        out.push(home.join("Library/Application Support/com.wizards.mtga"));
        out.push(PathBuf::from(
            "/Applications/MTGA.app/Contents/Resources/Data",
        ));
        out.push(home.join(".steam/steam/steamapps/common/MTGA/MTGA_Data"));
        out.push(home.join(".local/share/Steam/steamapps/common/MTGA/MTGA_Data"));
    }
    out
}

fn card_db_in(data: &Path) -> Option<PathBuf> {
    let raw = data.join("Downloads").join("Raw");
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    for entry in fs::read_dir(raw).ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("Raw_CardDatabase_") && name.ends_with(".mtga") {
            let modified = entry
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            if best.as_ref().is_none_or(|(t, _)| modified > *t) {
                best = Some((modified, entry.path()));
            }
        }
    }
    best.map(|(_, p)| p)
}

/// Sucht die lokale Arena-Kartendatenbank (`Raw_CardDatabase_*.mtga`, SQLite).
pub fn find_card_db(user_override: Option<&str>) -> Result<PathBuf> {
    mtga_data_candidates(user_override)
        .iter()
        .find_map(|c| card_db_in(c))
        .ok_or_else(|| anyhow::anyhow!(tr!("paths.arena_kartendatenbank_nicht_gefunden")))
}

#[cfg(test)]
#[path = "tests/paths.rs"]
mod tests;
