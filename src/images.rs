//! Card images from Scryfall (freely available, without an API key).
//!
//! A background thread fetches images one after another – Scryfall asks for
//! at most 10 requests per second – and stores them in the cache, so each
//! card is downloaded only once.
//!
//! Images exist per language: the key is the English card name, for other
//! languages with an appended code (see [`key`]). If there is no printing in
//! the language, the English image is used.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Sender, channel};

use serde_json::Value;

use crate::http::{FetchError, Http};
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

/// Decoded card image: RGBA pixels row by row (independent of the
/// interface; the app turns it into a texture).
#[derive(Clone, Debug, PartialEq)]
pub struct Picture {
    /// Width and height in pixels.
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

/// Separator between card name and language code in the key.
const LANG_SEP: char = '@';

/// Key for image and cache: the English card name, for other languages with
/// a code, e.g. "Shock@de".
pub fn key(name: &str, locale: Locale) -> String {
    match locale {
        Locale::English => name.to_string(),
        other => format!("{name}{LANG_SEP}{}", other.code()),
    }
}

/// Card name and language code (`None` = English) from a key.
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

/// Scryfall address for the card image (`mode` = `exact` or `fuzzy`).
fn image_url(mode: &str, name: &str) -> String {
    format!(
        "https://api.scryfall.com/cards/named?{mode}={}&format=image&version=normal",
        encode(name)
    )
}

/// Scryfall search for printings of a card in a language.
fn search_url(name: &str, lang: &str) -> String {
    let query = format!("!\"{name}\" lang:{lang}");
    format!(
        "https://api.scryfall.com/cards/search?q={}&unique=prints",
        encode(&query)
    )
}

/// Image address of the first hit of a Scryfall search (the front face for
/// double-faced cards).
fn image_from_search(json: &str) -> Option<String> {
    let v: Value = serde_json::from_str(json).ok()?;
    let card = &v["data"][0];
    card["image_uris"]["normal"]
        .as_str()
        .or_else(|| card["card_faces"][0]["image_uris"]["normal"].as_str())
        .map(str::to_string)
}

const IMAGE: &str = "image/*,*/*;q=0.8";

/// English image: exact name first, otherwise fuzzy search (e.g. for
/// double-faced cards). `Ok(None)` = Scryfall knows none.
fn download_english(http: &dyn Http, name: &str) -> Result<Option<Vec<u8>>, FetchError> {
    for mode in ["exact", "fuzzy"] {
        match http.get(&image_url(mode, name), IMAGE) {
            Ok(bytes) => return Ok(Some(bytes)),
            Err(FetchError::NotFound) => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(None)
}

/// `Ok(None)` instead of an error if the address doesn't exist.
fn found(result: Result<Vec<u8>, FetchError>) -> Result<Option<Vec<u8>>, FetchError> {
    match result {
        Ok(bytes) => Ok(Some(bytes)),
        Err(FetchError::NotFound) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Image of a printing in language `lang` (`Ok(None)` = none available).
fn download_localized(
    http: &dyn Http,
    name: &str,
    lang: &str,
) -> Result<Option<Vec<u8>>, FetchError> {
    let Some(json) = found(http.get(&search_url(name, lang), "application/json"))? else {
        return Ok(None);
    };
    match image_from_search(&String::from_utf8_lossy(&json)) {
        Some(url) => found(http.get(&url, IMAGE)),
        None => Ok(None),
    }
}

/// Image for a key (see [`key`]); without a printing in the language the English one.
pub(crate) fn download(http: &dyn Http, key: &str) -> Result<Option<Vec<u8>>, FetchError> {
    match split_key(key) {
        (name, Some(lang)) => match download_localized(http, name, lang)? {
            Some(bytes) => Ok(Some(bytes)),
            None => download_english(http, name),
        },
        (name, None) => download_english(http, name),
    }
}

/// What the image cache knows about a card.
enum Cached {
    Image(Picture),
    /// Scryfall knows no image (remembered so it isn't asked again).
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
        // Drop a damaged file and load it again.
        let _ = fs::remove_file(&file);
    }
    if missing.exists() {
        Cached::Missing
    } else {
        Cached::Unknown
    }
}

/// Remembers a downloaded image or that there is none.
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

/// Result of an image request.
#[derive(Clone, Debug, PartialEq)]
pub enum Fetched {
    Found(Picture),
    /// Scryfall knows no image (remembered).
    NotFound,
    /// Network error: try again later.
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

/// Starts the image thread. Send card names into the returned channel; `done`
/// is called with the image (or `None`).
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
