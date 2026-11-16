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

fn download(agent: &ureq::Agent, name: &str) -> Option<Vec<u8>> {
    let encoded: String = name
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect();
    // Erst exakter Name, sonst unscharfe Suche (z. B. bei doppelseitigen Karten).
    for mode in ["exact", "fuzzy"] {
        let url = format!(
            "https://api.scryfall.com/cards/named?{mode}={encoded}&format=image&version=normal"
        );
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

fn load(agent: &ureq::Agent, cache: &Path, name: &str) -> Option<ColorImage> {
    let stem = file_stem(name);
    let file = cache.join(format!("{stem}.jpg"));
    let missing = cache.join(format!("{stem}.none"));
    if let Ok(bytes) = fs::read(&file) {
        if let Some(img) = decode(&bytes) {
            return Some(img);
        }
        let _ = fs::remove_file(&file);
    }
    if missing.exists() {
        return None;
    }
    match download(agent, name) {
        Some(bytes) => {
            let img = decode(&bytes);
            if img.is_some() {
                let _ = fs::write(&file, &bytes);
            }
            img
        }
        None => {
            // Merken, damit nicht bei jedem Überfahren neu gefragt wird.
            let _ = fs::write(&missing, b"");
            None
        }
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
