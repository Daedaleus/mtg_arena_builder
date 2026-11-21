//! Settings and session state that survive a restart (`settings.json` in the
//! data folder).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::games::GameFormat;
use crate::i18n::Language;
use crate::meta::Format;
use crate::paths;

/// Current version of `settings.json`. When it is increased,
/// [`Settings::migrate`] adapts older files.
pub const SETTINGS_VERSION: u32 = 2;

/// Everything that should survive a restart.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Version of the file (0 = before the version number was introduced; if
    /// the field is missing, 0 applies – not the default of the new settings).
    #[serde(default)]
    pub version: u32,
    pub mtga_dir: String,
    /// Companion window open?
    pub companion: bool,
    /// Text size (zoom factor of the interface).
    pub zoom: f32,
    /// "Getting started" hidden?
    pub onboarding_hidden: bool,
    /// In the file as an MTGTop8 code ("ST"); older files with a number (index
    /// of the former format list) are still read.
    #[serde(with = "format_code")]
    pub format: Format,
    /// Game mode of the whole app (in the file as a code, e.g. "brawl").
    #[serde(with = "mode_code")]
    pub mode: GameFormat,
    pub include_side: bool,
    pub with_sets: bool,
    pub sort_by_fit: bool,
    /// Active tab (see `app::Tab`).
    pub tab: u8,
    pub paste_text: String,
    pub sel_arch: Option<u32>,
    pub sel_deck: Option<u32>,
    /// Chosen replacement cards: (deck, card key, replacement).
    pub replacements: Vec<(u32, String, String)>,
    /// Interface language (default: like the operating system).
    pub language: Language,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            version: SETTINGS_VERSION,
            mtga_dir: String::new(),
            companion: false,
            zoom: 1.0,
            onboarding_hidden: false,
            format: Format::Standard,
            mode: GameFormat::Standard,
            include_side: true,
            with_sets: true,
            sort_by_fit: false,
            tab: 0,
            paste_text: String::new(),
            sel_arch: None,
            sel_deck: None,
            replacements: Vec::new(),
            language: Language::System,
        }
    }
}

/// Smallest and largest allowed zoom factor.
pub const ZOOM_RANGE: (f32, f32) = (0.8, 1.6);

impl Settings {
    /// Brings settings from an older file up to date.
    /// Version 0 (without number) has the same fields – only the number is
    /// missing. Version 2 adds the game mode: taken from the format (Pioneer
    /// becomes Explorer, Arena has no Pioneer).
    pub fn migrate(mut self) -> Settings {
        if self.version < 2 {
            self.mode = GameFormat::from_meta(self.format);
            self.format = self.mode.meta().unwrap_or_default();
        }
        self.version = SETTINGS_VERSION;
        self
    }

    /// Zoom factor within the allowed range.
    pub fn zoom(&self) -> f32 {
        self.zoom.clamp(ZOOM_RANGE.0, ZOOM_RANGE.1)
    }

    /// Replacement cards as a lookup table (deck, card key) -> replacement.
    pub fn replacement_map(&self) -> HashMap<(u32, String), String> {
        self.replacements
            .iter()
            .cloned()
            .map(|(deck, key, rep)| ((deck, key), rep))
            .collect()
    }

    /// Takes over the replacement cards (sorted, so the file stays stable).
    pub fn set_replacements(&mut self, map: &HashMap<(u32, String), String>) {
        let mut list: Vec<(u32, String, String)> = map
            .iter()
            .map(|((deck, key), rep)| (*deck, key.clone(), rep.clone()))
            .collect();
        list.sort();
        self.replacements = list;
    }
}

/// The file `settings.json`; only writes when something changed.
pub struct SettingsFile {
    path: PathBuf,
    saved: Vec<u8>,
    /// May it be written? No if the existing file was unreadable – otherwise
    /// the settings would be replaced by defaults.
    writable: bool,
    /// Problem while loading (for the status line).
    pub problem: Option<String>,
}

impl SettingsFile {
    /// Reads the settings from `dir`; if the file is missing or damaged, the
    /// defaults apply.
    pub fn load(dir: &Path) -> (SettingsFile, Settings) {
        let path = dir.join("settings.json");
        let (raw, problem, writable) = match paths::load_json::<Settings>(&path) {
            Ok(s) => (Some(s), None, true),
            Err(paths::LoadError::Missing) => (None, None, true),
            Err(paths::LoadError::Corrupt(e)) => match paths::backup_corrupt(&path) {
                Ok(backup) => {
                    let text = tr!(
                        "settings.settings_json_corrupt_saved",
                        e = e,
                        backup = backup.display()
                    );
                    (None, Some(text), true)
                }
                Err(io) => (
                    None,
                    Some(tr!("settings.settings_json_cannot_read", e = io)),
                    false,
                ),
            },
            Err(paths::LoadError::Unreadable(e)) => (
                None,
                Some(tr!("settings.settings_json_cannot_read", e = e)),
                false,
            ),
        };
        // Remember the stored state before the migration – then an adapted
        // file is written again on the next save.
        let saved = raw
            .as_ref()
            .and_then(|s| serde_json::to_vec_pretty(s).ok())
            .unwrap_or_default();
        let settings = raw.map(Settings::migrate).unwrap_or_default();
        let saved = if saved.is_empty() {
            serde_json::to_vec_pretty(&settings).unwrap_or_default()
        } else {
            saved
        };
        (
            SettingsFile {
                path,
                saved,
                writable,
                problem,
            },
            settings,
        )
    }

    /// Writes the settings if they changed since the last save.
    /// `Ok(true)` = written, `Ok(false)` = unchanged.
    pub fn save(&mut self, settings: &Settings) -> std::io::Result<bool> {
        let json = serde_json::to_vec_pretty(settings)?;
        if json == self.saved || !self.writable {
            return Ok(false);
        }
        paths::write_atomic(&self.path, &json)?;
        self.saved = json;
        Ok(true)
    }
}

/// Store `format` as an MTGTop8 code, read a number or a code.
mod format_code {
    use serde::{Deserialize, Deserializer, Serializer};

    use crate::meta::Format;

    pub fn serialize<S: Serializer>(format: &Format, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(format.code())
    }

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Stored {
        Index(usize),
        Code(String),
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Format, D::Error> {
        Ok(match Stored::deserialize(d)? {
            Stored::Index(i) => Format::ALL[i.min(Format::ALL.len() - 1)],
            Stored::Code(code) => Format::from_code(&code).unwrap_or_default(),
        })
    }
}

/// Store the game mode as a code; unknown codes give Standard.
mod mode_code {
    use serde::{Deserialize, Deserializer, Serializer};

    use crate::games::GameFormat;

    pub fn serialize<S: Serializer>(mode: &GameFormat, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(mode.code())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<GameFormat, D::Error> {
        let code = String::deserialize(d)?;
        Ok(GameFormat::from_code(&code).unwrap_or(GameFormat::Standard))
    }
}

#[cfg(test)]
#[path = "tests/settings.rs"]
mod tests;
