//! Einstellungen und Sitzungszustand, die einen Neustart überleben
//! (`settings.json` im Datenordner).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::i18n::Language;
use crate::paths;

/// Alles, was einen Neustart überleben soll.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub mtga_dir: String,
    /// Begleitfenster geöffnet?
    pub companion: bool,
    /// Textgröße (Zoomfaktor der Oberfläche).
    pub zoom: f32,
    /// „Erste Schritte“ ausgeblendet?
    pub onboarding_hidden: bool,
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
}

impl SettingsFile {
    /// Liest die Einstellungen aus `dir`; fehlt die Datei oder ist sie
    /// beschädigt, gelten die Standardwerte.
    pub fn load(dir: &Path) -> (SettingsFile, Settings) {
        let path = dir.join("settings.json");
        let settings: Settings = paths::read_json(&path).unwrap_or_default();
        let saved = serde_json::to_vec_pretty(&settings).unwrap_or_default();
        (SettingsFile { path, saved }, settings)
    }

    /// Schreibt die Einstellungen, falls sie sich seit dem letzten Speichern
    /// geändert haben. Gibt zurück, ob geschrieben wurde.
    pub fn save(&mut self, settings: &Settings) -> bool {
        let Ok(json) = serde_json::to_vec_pretty(settings) else {
            return false;
        };
        if json == self.saved || paths::write_atomic(&self.path, &json).is_err() {
            return false;
        }
        self.saved = json;
        true
    }
}

#[cfg(test)]
#[path = "tests/settings.rs"]
mod tests;
