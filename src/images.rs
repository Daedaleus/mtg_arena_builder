//! Kartenbilder von Scryfall (frei abrufbar, ohne API-Schlüssel).
//!
//! Ein Hintergrund-Thread holt Bilder nacheinander – Scryfall bittet um
//! höchstens 10 Anfragen pro Sekunde – und legt sie im Cache ab, sodass jede
//! Karte nur einmal heruntergeladen wird.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Sender, channel};
use std::thread::sleep;
use std::time::Duration;

use eframe::egui::ColorImage;

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

/// Scryfall-Adresse für das Kartenbild (`mode` = `exact` oder `fuzzy`).
fn image_url(mode: &str, name: &str) -> String {
    let encoded: String = name
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect();
    format!("https://api.scryfall.com/cards/named?{mode}={encoded}&format=image&version=normal")
}

fn download(agent: &ureq::Agent, name: &str) -> Option<Vec<u8>> {
    // Erst exakter Name, sonst unscharfe Suche (z. B. bei doppelseitigen Karten).
    for mode in ["exact", "fuzzy"] {
        let url = image_url(mode, name);
        sleep(Duration::from_millis(120));
        match agent.get(&url).set("Accept", "image/*,*/*;q=0.8").call() {
            Ok(resp) => {
                let mut bytes = Vec::new();
                resp.into_reader()
                    .take(5_000_000)
                    .read_to_end(&mut bytes)
                    .ok()?;
                return Some(bytes);
            }
            Err(ureq::Error::Status(404, _)) => continue,
            Err(_) => return None,
        }
    }
    None
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
