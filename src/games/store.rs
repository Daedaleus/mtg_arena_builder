//! Permanent storage of the games in `games.json`.

use super::*;

/// Version of the analysis. When it is increased, stored games are
/// recomputed from logs that still exist at the next read.
pub(super) const PARSER_VERSION: u32 = 7;

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Store {
    pub(super) version: u32,
    pub(super) games: Vec<GameRecord>,
}

/// Result of [`sync`].
#[derive(Debug, Default)]
pub struct Synced {
    pub games: Vec<GameRecord>,
    /// Problem with `games.json` for the status line: unreadable, damaged
    /// (then backed up) or not savable. The games are there anyway.
    pub problem: Option<String>,
}

/// Load stored games. A damaged file is backed up (not overwritten); if it
/// is unreadable, `sync` must not replace it.
fn load_store(store: &Path) -> (Store, Option<String>, bool) {
    use crate::paths::{LoadError, backup_corrupt, load_json};
    match load_json::<Store>(store) {
        Ok(s) => (s, None, true),
        Err(LoadError::Missing) => (Store::default(), None, true),
        // Older files contained only the list of games.
        Err(LoadError::Corrupt(_)) if load_json::<Vec<GameRecord>>(store).is_ok() => {
            let games = load_json::<Vec<GameRecord>>(store).unwrap_or_default();
            (Store { version: 1, games }, None, true)
        }
        Err(LoadError::Corrupt(e)) => match backup_corrupt(store) {
            Ok(backup) => {
                let text = tr!(
                    "games_store.games_json_corrupt_saved",
                    e = e,
                    backup = backup.display()
                );
                (Store::default(), Some(text), true)
            }
            Err(io) => {
                let text = tr!(
                    "games_store.games_json_cannot_read",
                    e = format!("{e}; {io}")
                );
                (Store::default(), Some(text), false)
            }
        },
        Err(LoadError::Unreadable(e)) => (
            Store::default(),
            Some(tr!("games_store.games_json_cannot_read", e = e)),
            false,
        ),
    }
}

/// Load stored games, add new ones from the logs and save.
///
/// Every game knows the version of the analysis that created it. A better
/// recording replaces a worse one; an equally good one only if the stored
/// one came from an older analysis. Games whose log no longer exists keep
/// their old version.
pub fn sync(store: &Path, logs: &[PathBuf]) -> Synced {
    let (saved, problem, writable) = load_store(store);
    let file_outdated = saved.version < PARSER_VERSION;
    let mut games = saved.games;
    // Older files knew only one version for all games.
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
    if !changed || !writable {
        return Synced { games, problem };
    }
    let out = Store {
        version: PARSER_VERSION,
        games,
    };
    let save_error = serde_json::to_vec(&out)
        .map_err(|e| e.to_string())
        .and_then(|json| crate::paths::write_atomic(store, &json).map_err(|e| e.to_string()))
        .err()
        .map(|e| tr!("games_store.could_save_games", e = e));
    Synced {
        games: out.games,
        problem: problem.or(save_error),
    }
}
