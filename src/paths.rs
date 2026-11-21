//! Locations of MTG Arena files (log, card database) and of this app's data.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use directories::{BaseDirs, ProjectDirs};

/// Directory for settings, collection and web cache of this app.
pub fn data_dir() -> PathBuf {
    let dir = ProjectDirs::from("", "", "mtg_arena_builder")
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let _ = fs::create_dir_all(&dir);
    dir
}

/// Writes a file so that it is never left half-written (first into a
/// temporary file, then rename).
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)
}

/// Why a JSON file could not be loaded. Important for files with user data:
/// a missing file is normal, a damaged one must be backed up, an unreadable
/// one must not be overwritten.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("file does not exist")]
    Missing,
    #[error("file is corrupt: {0}")]
    Corrupt(#[from] serde_json::Error),
    #[error("file cannot be read: {0}")]
    Unreadable(std::io::Error),
}

/// Loads a JSON file; a UTF-8 BOM at the start (e.g. from Windows editors) is ignored.
pub fn load_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, LoadError> {
    let bytes = fs::read(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => LoadError::Missing,
        _ => LoadError::Unreadable(e),
    })?;
    let body = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
    Ok(serde_json::from_slice(body)?)
}

/// Like [`load_json`], for files where the reason doesn't matter (caches).
pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    load_json(path).ok()
}

/// Renames a damaged file (`name.corrupt-<timestamp>`) so it isn't
/// overwritten. Returns the new path.
pub fn backup_corrupt(path: &Path) -> std::io::Result<PathBuf> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let backup = path.with_file_name(format!("{name}.corrupt-{stamp}"));
    fs::rename(path, &backup)?;
    Ok(backup)
}

/// Possible locations of `Player.log` per platform.
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

/// Existing Arena logs (current and previous session).
pub fn player_logs() -> Vec<PathBuf> {
    player_log_candidates()
        .into_iter()
        .flat_map(|p| [p.with_file_name("Player-prev.log"), p])
        .filter(|p| p.is_file())
        .collect()
}

/// Reads the install directory from the head of Player.log
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

/// `MTGA_Data` directory from the line `Mono path[0] = '…/MTGA_Data/Managed'`.
fn install_from_log_head(head: &str) -> Option<PathBuf> {
    head.lines().find_map(|line| {
        let rest = line.strip_prefix("Mono path[0] = '")?;
        let path = rest.trim_end_matches('\'').trim_end_matches("/Managed");
        Some(PathBuf::from(path))
    })
}

/// Candidates for `MTGA_Data` (or the directory that contains `Downloads/Raw`).
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

/// Searches for the local Arena card database (`Raw_CardDatabase_*.mtga`, SQLite).
pub fn find_card_db(user_override: Option<&str>) -> Result<PathBuf> {
    mtga_data_candidates(user_override)
        .iter()
        .find_map(|c| card_db_in(c))
        .ok_or_else(|| anyhow::anyhow!(tr!("paths.arena_card_database_found")))
}

#[cfg(test)]
#[path = "tests/paths.rs"]
mod tests;
