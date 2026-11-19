//! Kartenbilder von Scryfall (frei abrufbar, ohne API-Schlüssel).
//!
//! Ein Hintergrund-Thread holt Bilder nacheinander – Scryfall bittet um
//! höchstens 10 Anfragen pro Sekunde – und legt sie im Cache ab, sodass jede
//! Karte nur einmal heruntergeladen wird.
//!
//! Bilder gibt es je Sprache: Der Schlüssel ist der englische Kartenname,
//! für andere Sprachen mit angehängtem Kürzel (siehe [`key`]). Gibt es keinen
//! Druck in der Sprache, kommt das englische Bild.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Sender, channel};
use std::thread::sleep;
use std::time::Duration;

use eframe::egui::ColorImage;
use serde_json::Value;

use crate::i18n::Locale;

const USER_AGENT: &str = "mtg_arena_builder/0.1 (privates Deckbau-Werkzeug)";

fn file_stem(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

fn decode(bytes: &[u8]) -> Option<ColorImage> {
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    Some(ColorImage::from_rgba_unmultiplied(size, img.as_raw()))
}

/// Trenner zwischen Kartenname und Sprachkürzel im Schlüssel.
const LANG_SEP: char = '@';

/// Schlüssel für Bild und Cache: der englische Kartenname, für andere
/// Sprachen mit Kürzel, z. B. „Shock@de“.
pub fn key(name: &str, locale: Locale) -> String {
    match locale {
        Locale::English => name.to_string(),
        other => format!("{name}{LANG_SEP}{}", other.code()),
    }
}

/// Kartenname und Sprachkürzel (`None` = Englisch) aus einem Schlüssel.
fn split_key(key: &str) -> (&str, Option<&str>) {
    match key.rsplit_once(LANG_SEP) {
        Some((name, lang)) => (name, Some(lang)),
        None => (key, None),
    }
}

fn encode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Scryfall-Adresse für das Kartenbild (`mode` = `exact` oder `fuzzy`).
fn image_url(mode: &str, name: &str) -> String {
    format!(
        "https://api.scryfall.com/cards/named?{mode}={}&format=image&version=normal",
        encode(name)
    )
}

/// Scryfall-Suche nach Drucken einer Karte in einer Sprache.
fn search_url(name: &str, lang: &str) -> String {
    let query = format!("!\"{name}\" lang:{lang}");
    format!(
        "https://api.scryfall.com/cards/search?q={}&unique=prints",
        encode(&query)
    )
}

/// Bildadresse des ersten Treffers einer Scryfall-Suche (bei doppelseitigen
/// Karten die Vorderseite).
fn image_from_search(json: &str) -> Option<String> {
    let v: Value = serde_json::from_str(json).ok()?;
    let card = &v["data"][0];
    card["image_uris"]["normal"]
        .as_str()
        .or_else(|| card["card_faces"][0]["image_uris"]["normal"].as_str())
        .map(str::to_string)
}

/// Lädt eine Adresse; `Ok(None)` heißt „gibt es nicht“ (404).
fn fetch(agent: &ureq::Agent, url: &str, accept: &str) -> Result<Option<Vec<u8>>, ()> {
    sleep(Duration::from_millis(120));
    match agent.get(url).set("Accept", accept).call() {
        Ok(resp) => {
            let mut bytes = Vec::new();
            resp.into_reader()
                .take(5_000_000)
                .read_to_end(&mut bytes)
                .map_err(|_| ())?;
            Ok(Some(bytes))
        }
        Err(ureq::Error::Status(404, _)) => Ok(None),
        Err(_) => Err(()),
    }
}

fn download_english(agent: &ureq::Agent, name: &str) -> Option<Vec<u8>> {
    // Erst exakter Name, sonst unscharfe Suche (z. B. bei doppelseitigen Karten).
    for mode in ["exact", "fuzzy"] {
        match fetch(agent, &image_url(mode, name), "image/*,*/*;q=0.8") {
            Ok(Some(bytes)) => return Some(bytes),
            Ok(None) => continue,
            Err(()) => return None,
        }
    }
    None
}

fn download_localized(agent: &ureq::Agent, name: &str, lang: &str) -> Option<Vec<u8>> {
    let json = fetch(agent, &search_url(name, lang), "application/json").ok()??;
    let url = image_from_search(&String::from_utf8_lossy(&json))?;
    fetch(agent, &url, "image/*,*/*;q=0.8").ok()?
}

fn download(agent: &ureq::Agent, key: &str) -> Option<Vec<u8>> {
    match split_key(key) {
        (name, Some(lang)) => {
            download_localized(agent, name, lang).or_else(|| download_english(agent, name))
        }
        (name, None) => download_english(agent, name),
    }
}

/// Was der Bild-Cache zu einer Karte weiß.
enum Cached {
    Image(ColorImage),
    /// Scryfall kennt kein Bild (gemerkt, damit nicht erneut gefragt wird).
    Missing,
    Unknown,
}

fn cache_files(cache: &Path, name: &str) -> (PathBuf, PathBuf) {
    let stem = file_stem(name);
    (
        cache.join(format!("{stem}.jpg")),
        cache.join(format!("{stem}.none")),
    )
}

fn from_cache(cache: &Path, name: &str) -> Cached {
    let (file, missing) = cache_files(cache, name);
    if let Ok(bytes) = fs::read(&file) {
        if let Some(img) = decode(&bytes) {
            return Cached::Image(img);
        }
        // Beschädigte Datei verwerfen und neu laden.
        let _ = fs::remove_file(&file);
    }
    if missing.exists() {
        Cached::Missing
    } else {
        Cached::Unknown
    }
}

/// Merkt ein heruntergeladenes Bild bzw. dass es keines gibt.
fn store(cache: &Path, name: &str, bytes: Option<&[u8]>) -> Option<ColorImage> {
    let (file, missing) = cache_files(cache, name);
    match bytes {
        Some(bytes) => {
            let img = decode(bytes);
            if img.is_some() {
                let _ = fs::write(&file, bytes);
            }
            img
        }
        None => {
            let _ = fs::write(&missing, b"");
            None
        }
    }
}

fn load(agent: &ureq::Agent, cache: &Path, name: &str) -> Option<ColorImage> {
    match from_cache(cache, name) {
        Cached::Image(img) => Some(img),
        Cached::Missing => None,
        Cached::Unknown => store(cache, name, download(agent, name).as_deref()),
    }
}

/// Startet den Bild-Thread. Kartennamen in den zurückgegebenen Kanal schicken;
/// `done` wird mit dem Bild (oder `None`) aufgerufen.
pub fn spawn(
    cache: PathBuf,
    done: impl Fn(String, Option<ColorImage>) + Send + 'static,
) -> Sender<String> {
    let (tx, rx) = channel::<String>();
    let _ = fs::create_dir_all(&cache);
    std::thread::spawn(move || {
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(20))
            .user_agent(USER_AGENT)
            .build();
        while let Ok(name) = rx.recv() {
            let img = load(&agent, &cache, &name);
            done(name, img);
        }
    });
    tx
}

#[cfg(test)]
#[path = "tests/images.rs"]
mod tests;
