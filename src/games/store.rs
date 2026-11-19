//! Dauerhafte Ablage der Partien in `games.json`.

use super::*;

/// Version der Auswertung. Wird sie erhöht, werden gespeicherte Partien beim
/// nächsten Einlesen aus noch vorhandenen Logs neu berechnet.
pub(super) const PARSER_VERSION: u32 = 5;

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Store {
    pub(super) version: u32,
    pub(super) games: Vec<GameRecord>,
}

/// Ergebnis von [`sync`].
#[derive(Debug, Default)]
pub struct Synced {
    pub games: Vec<GameRecord>,
    /// `games.json` konnte nicht geschrieben werden (die Partien sind trotzdem da).
    pub save_error: Option<String>,
}

/// Gespeicherte Partien laden, neue aus den Logs dazunehmen und speichern.
///
/// Jede Partie kennt die Version der Auswertung, mit der sie erstellt wurde.
/// Eine bessere Aufzeichnung ersetzt eine schlechtere; eine gleich gute nur,
/// wenn die gespeicherte mit einer älteren Auswertung entstand. Partien,
/// deren Log nicht mehr existiert, behalten ihre alte Version.
pub fn sync(store: &Path, logs: &[PathBuf]) -> Synced {
    // Ältere Dateien enthielten nur die Liste der Partien.
    let saved: Store = crate::paths::read_json::<Store>(store)
        .or_else(|| {
            crate::paths::read_json::<Vec<GameRecord>>(store)
                .map(|games| Store { version: 1, games })
        })
        .unwrap_or_default();
    let file_outdated = saved.version < PARSER_VERSION;
    let mut games = saved.games;
    // Ältere Dateien kannten nur eine Version für alle Partien.
    for g in games.iter_mut().filter(|g| g.parser == 0) {
        g.parser = saved.version;
    }
    let before = serde_json::to_vec(&games).unwrap_or_default();
    for log in logs {
        for mut rec in parse_log(log) {
            rec.parser = PARSER_VERSION;
            match games.iter_mut().find(|g| g.id == rec.id) {
                Some(old)
                    if quality(&rec) > quality(old)
                        || (old.parser < PARSER_VERSION && quality(&rec) >= quality(old)) =>
                {
                    *old = rec
                }
                Some(_) => {}
                None => games.push(rec),
            }
        }
    }
    games.sort_by_key(|g| std::cmp::Reverse(g.started_ms));
    let changed = file_outdated || serde_json::to_vec(&games).unwrap_or_default() != before;
    if !changed {
        return Synced {
            games,
            save_error: None,
        };
    }
    let out = Store {
        version: PARSER_VERSION,
        games,
    };
    let save_error = serde_json::to_vec(&out)
        .map_err(|e| e.to_string())
        .and_then(|json| crate::paths::write_atomic(store, &json).map_err(|e| e.to_string()))
        .err()
        .map(|e| tr!("games_store.speichern_fehlgeschlagen", e = e));
    Synced {
        games: out.games,
        save_error,
    }
}
