//! Einstellungen und Sitzungszustand, die einen Neustart überleben
//! (`settings.json` im Datenordner).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::i18n::Language;
use crate::paths;

/// Aktuelle Version von `settings.json`. Wird sie erhöht, passt
/// [`Settings::migrate`] ältere Dateien an.
pub const SETTINGS_VERSION: u32 = 1;

/// Alles, was einen Neustart überleben soll.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Version der Datei (0 = vor Einführung der Versionsnummer; fehlt das
    /// Feld, gilt 0 – nicht der Standardwert der neuen Einstellungen).
    #[serde(default)]
    pub version: u32,
    pub mtga_dir: String,
    /// Begleitfenster geöffnet?
    pub companion: bool,
    /// Textgröße (Zoomfaktor der Oberfläche).
    pub zoom: f32,
    /// „Erste Schritte“ ausgeblendet?
    pub onboarding_hidden: bool,
    /// Index in [`crate::meta::FORMATS`]; in der Datei als Kürzel („ST“), damit
    /// eine geänderte Formatliste die Wahl nicht verschiebt. Ältere Dateien
    /// mit Zahl werden weiter gelesen.
    #[serde(with = "format_code")]
    pub format: usize,
    pub include_side: bool,
    pub with_sets: bool,
    pub sort_by_fit: bool,
    /// Aktiver Reiter (siehe `app::Tab`).
    pub tab: u8,
    pub paste_text: String,
    pub sel_arch: Option<u32>,
    pub sel_deck: Option<u32>,
    /// Gewählte Ersatzkarten: (Deck, Kartenschlüssel, Ersatz).
    pub replacements: Vec<(u32, String, String)>,
    /// Sprache der Oberfläche (Standard: wie das Betriebssystem).
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
            format: 0,
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

/// Kleinster und größter erlaubter Zoomfaktor.
pub const ZOOM_RANGE: (f32, f32) = (0.8, 1.6);

impl Settings {
    /// Bringt Einstellungen aus einer älteren Datei auf den aktuellen Stand.
    /// Version 0 (ohne Nummer) hat dieselben Felder – nur die Nummer fehlt.
    pub fn migrate(mut self) -> Settings {
        self.version = SETTINGS_VERSION;
        self
    }

    /// Zoomfaktor im erlaubten Bereich.
    pub fn zoom(&self) -> f32 {
        self.zoom.clamp(ZOOM_RANGE.0, ZOOM_RANGE.1)
    }

    /// Ersatzkarten als Nachschlagetabelle (Deck, Kartenschlüssel) -> Ersatz.
    pub fn replacement_map(&self) -> HashMap<(u32, String), String> {
        self.replacements
            .iter()
            .cloned()
            .map(|(deck, key, rep)| ((deck, key), rep))
            .collect()
    }

    /// Übernimmt die Ersatzkarten (sortiert, damit die Datei stabil bleibt).
    pub fn set_replacements(&mut self, map: &HashMap<(u32, String), String>) {
        let mut list: Vec<(u32, String, String)> = map
            .iter()
            .map(|((deck, key), rep)| (*deck, key.clone(), rep.clone()))
            .collect();
        list.sort();
        self.replacements = list;
    }
}

/// Die Datei `settings.json`; schreibt nur, wenn sich etwas geändert hat.
pub struct SettingsFile {
    path: PathBuf,
    saved: Vec<u8>,
    /// Darf geschrieben werden? Nein, wenn die vorhandene Datei nicht lesbar
    /// war – sonst würden die Einstellungen durch Standardwerte ersetzt.
    writable: bool,
    /// Problem beim Laden (für die Statuszeile).
    pub problem: Option<String>,
}

impl SettingsFile {
    /// Liest die Einstellungen aus `dir`; fehlt die Datei oder ist sie
    /// beschädigt, gelten die Standardwerte.
    pub fn load(dir: &Path) -> (SettingsFile, Settings) {
        let path = dir.join("settings.json");
        let (raw, problem, writable) = match paths::load_json::<Settings>(&path) {
            Ok(s) => (Some(s), None, true),
            Err(paths::LoadError::Missing) => (None, None, true),
            Err(paths::LoadError::Corrupt(e)) => match paths::backup_corrupt(&path) {
                Ok(backup) => {
                    let text = tr!(
                        "settings.beschaedigt_gesichert",
                        e = e,
                        backup = backup.display()
                    );
                    (None, Some(text), true)
                }
                Err(io) => (None, Some(tr!("settings.nicht_lesbar", e = io)), false),
            },
            Err(paths::LoadError::Unreadable(e)) => {
                (None, Some(tr!("settings.nicht_lesbar", e = e)), false)
            }
        };
        // Gespeicherten Stand vor der Migration merken – dann wird eine
        // angepasste Datei beim nächsten Speichern neu geschrieben.
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

    /// Schreibt die Einstellungen, falls sie sich seit dem letzten Speichern
    /// geändert haben. `Ok(true)` = geschrieben, `Ok(false)` = unverändert.
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

/// `format` als MTGTop8-Kürzel speichern, Zahl oder Kürzel lesen.
mod format_code {
    use serde::{Deserialize, Deserializer, Serializer};

    use crate::meta::FORMATS;

    pub fn serialize<S: Serializer>(format: &usize, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(FORMATS.get(*format).map_or(FORMATS[0].0, |f| f.0))
    }

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Stored {
        Index(usize),
        Code(String),
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<usize, D::Error> {
        Ok(match Stored::deserialize(d)? {
            Stored::Index(i) => i.min(FORMATS.len() - 1),
            Stored::Code(code) => FORMATS.iter().position(|f| f.0 == code).unwrap_or(0),
        })
    }
}

#[cfg(test)]
#[path = "tests/settings.rs"]
mod tests;
