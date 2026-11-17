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

/// Gespeicherte Partien laden, neue aus den Logs dazunehmen und speichern.
pub fn sync(store: &Path, logs: &[PathBuf]) -> Vec<GameRecord> {
    // Ältere Dateien enthielten nur die Liste der Partien.
    let saved: Store = crate::paths::read_json::<Store>(store)
        .or_else(|| {
            crate::paths::read_json::<Vec<GameRecord>>(store)
                .map(|games| Store { version: 1, games })
        })
        .unwrap_or_default();
    let outdated = saved.version < PARSER_VERSION;
    let before = serde_json::to_vec(&saved.games).unwrap_or_default();
    let mut games = saved.games;
    for log in logs {
        for rec in parse_log(log) {
            match games.iter_mut().find(|g| g.id == rec.id) {
                // Vollständigere oder mit neuerer Auswertung erzeugte Aufzeichnung gewinnt.
                // Bessere Aufzeichnung gewinnt; nach einem Update der Auswertung
                // auch eine gleich gute (dann mit den neuen Daten), nie eine schlechtere.
                Some(old)
                    if quality(&rec) > quality(old)
                        || (outdated && quality(&rec) >= quality(old)) =>
                {
                    *old = rec
                }
                Some(_) => {}
                None => games.push(rec),
            }
        }
    }
    games.sort_by_key(|g| std::cmp::Reverse(g.started_ms));
    let changed = outdated || serde_json::to_vec(&games).unwrap_or_default() != before;
    if changed {
        let out = Store {
            version: PARSER_VERSION,
            games,
        };
        if let Ok(json) = serde_json::to_vec(&out) {
            let _ = crate::paths::write_atomic(store, &json);
        }
        return out.games;
    }
    games
}
