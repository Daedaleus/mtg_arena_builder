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
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Sender, channel};

use serde_json::Value;

use crate::http::Http;
use crate::i18n::Locale;

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

/// Dekodiertes Kartenbild: RGBA-Pixel Zeile für Zeile (unabhängig von der
/// Oberfläche; die App macht daraus eine Textur).
#[derive(Clone, Debug, PartialEq)]
pub struct Picture {
    /// Breite und Höhe in Pixeln.
    pub size: [usize; 2],
    pub rgba: Vec<u8>,
}

pub(crate) fn decode(bytes: &[u8]) -> Option<Picture> {
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    Some(Picture {
        size,
        rgba: img.into_raw(),
    })
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

const IMAGE: &str = "image/*,*/*;q=0.8";

/// Englisches Bild: erst exakter Name, sonst unscharfe Suche (z. B. bei
/// doppelseitigen Karten). `Ok(None)` = Scryfall kennt keins.
fn download_english(http: &dyn Http, name: &str) -> anyhow::Result<Option<Vec<u8>>> {
    for mode in ["exact", "fuzzy"] {
        if let Some(bytes) = http.get(&image_url(mode, name), IMAGE)? {
            return Ok(Some(bytes));
        }
    }
    Ok(None)
}

/// Bild eines Drucks in der Sprache `lang` (`Ok(None)` = keiner vorhanden).
fn download_localized(http: &dyn Http, name: &str, lang: &str) -> anyhow::Result<Option<Vec<u8>>> {
    let Some(json) = http.get(&search_url(name, lang), "application/json")? else {
        return Ok(None);
    };
    match image_from_search(&String::from_utf8_lossy(&json)) {
        Some(url) => http.get(&url, IMAGE),
        None => Ok(None),
    }
}

/// Bild zu einem Schlüssel (siehe [`key`]); ohne Druck in der Sprache das englische.
pub(crate) fn download(http: &dyn Http, key: &str) -> anyhow::Result<Option<Vec<u8>>> {
    match split_key(key) {
        (name, Some(lang)) => match download_localized(http, name, lang)? {
            Some(bytes) => Ok(Some(bytes)),
            None => download_english(http, name),
        },
        (name, None) => download_english(http, name),
    }
}

/// Was der Bild-Cache zu einer Karte weiß.
enum Cached {
    Image(Picture),
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
fn store(cache: &Path, name: &str, bytes: Option<&[u8]>) -> Option<Picture> {
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

/// Ergebnis einer Bildanfrage.
#[derive(Clone, Debug, PartialEq)]
pub enum Fetched {
    Found(Picture),
    /// Scryfall kennt kein Bild (gemerkt).
    NotFound,
    /// Netzfehler: später erneut versuchen.
    Failed,
}

fn load(http: &dyn Http, cache: &Path, name: &str) -> Fetched {
    match from_cache(cache, name) {
        Cached::Image(img) => Fetched::Found(img),
        Cached::Missing => Fetched::NotFound,
        Cached::Unknown => match download(http, name) {
            Ok(bytes) => {
                store(cache, name, bytes.as_deref()).map_or(Fetched::NotFound, Fetched::Found)
            }
            Err(_) => Fetched::Failed,
        },
    }
}

/// Startet den Bild-Thread. Kartennamen in den zurückgegebenen Kanal schicken;
/// `done` wird mit dem Bild (oder `None`) aufgerufen.
pub fn spawn(
    cache: PathBuf,
    http: Arc<dyn Http>,
    done: impl Fn(String, Fetched) + Send + 'static,
) -> Sender<String> {
    let (tx, rx) = channel::<String>();
    let _ = fs::create_dir_all(&cache);
    std::thread::spawn(move || {
        while let Ok(name) = rx.recv() {
            let img = load(http.as_ref(), &cache, &name);
            done(name, img);
        }
    });
    tx
}

#[cfg(test)]
#[path = "tests/images.rs"]
mod tests;
