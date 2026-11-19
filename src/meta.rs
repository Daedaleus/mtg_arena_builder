//! Turnierdecks von MTGTop8 und magic.gg, Format-Legalität von Scryfall.
//!
//! MTGTop8 verlangt in der robots.txt 5 Sekunden Abstand zwischen Anfragen –
//! daran halten wir uns und legen alles im lokalen Cache ab.

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result};
use regex::Regex;

use crate::carddb::name_key;
pub use crate::http::Http;
use crate::http::Ureq;
use crate::magicgg::{self, GgDeck, GgEvent};

/// (MTGTop8-Kürzel, Anzeigename, Scryfall-Formatname)
pub const FORMATS: [(&str, &str, &str); 5] = [
    ("ST", "Standard", "standard"),
    ("PI", "Pioneer", "pioneer"),
    ("EXP", "Explorer", "explorer"),
    ("HI", "Historic", "historic"),
    ("ALCH", "Alchemy", "alchemy"),
];

const TOP8: &str = "https://www.mtgtop8.com";
const TOP8_DELAY: Duration = Duration::from_secs(5);
const GG: &str = "https://magic.gg";
/// magic.gg nennt keine Wartezeit; trotzdem höflich bleiben.
const GG_DELAY: Duration = Duration::from_secs(2);
const HOUR: u64 = 3600;

#[derive(Clone, Debug)]
pub struct Archetype {
    pub id: u32,
    pub name: String,
    /// Anteil am Metagame in Prozent.
    pub share: u32,
    /// Zeitraum-Kennung der Übersichtsseite (wird für den Archetyp-Link benötigt).
    pub meta: u32,
}

#[derive(Clone, Debug)]
pub struct DeckRef {
    pub id: u32,
    pub player: String,
    pub event: String,
    pub rank: String,
    pub date: String,
}

/// Wartezeiten zwischen Anfragen an dieselbe Seite.
#[derive(Clone, Copy, Debug)]
pub struct Delays {
    pub top8: Duration,
    pub gg: Duration,
}

impl Default for Delays {
    fn default() -> Delays {
        Delays {
            top8: TOP8_DELAY,
            gg: GG_DELAY,
        }
    }
}

pub struct Fetcher {
    http: Box<dyn Http>,
    cache: PathBuf,
    delays: Delays,
    last_top8: Option<Instant>,
    last_gg: Option<Instant>,
}

/// Wartet, bis seit der letzten Anfrage an dieselbe Seite `delay` vergangen ist.
fn throttle(last: &mut Option<Instant>, delay: Duration) {
    if let Some(t) = *last {
        let since = t.elapsed();
        if since < delay {
            sleep(delay - since);
        }
    }
    *last = Some(Instant::now());
}

fn decode_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&#039;", "'")
        .replace("&quot;", "\"")
        .replace("&nbsp;", " ")
        .trim()
        .to_string()
}

impl Fetcher {
    pub fn new(cache: PathBuf) -> Fetcher {
        Fetcher::with_http(
            cache,
            Box::new(Ureq::new(Duration::from_secs(30))),
            Delays::default(),
        )
    }

    /// Wie [`Fetcher::new`], aber mit eigenem Netzabruf und eigenen Wartezeiten.
    pub fn with_http(cache: PathBuf, http: Box<dyn Http>, delays: Delays) -> Fetcher {
        let _ = fs::create_dir_all(&cache);
        Fetcher {
            http,
            cache,
            delays,
            last_top8: None,
            last_gg: None,
        }
    }

    fn cached(&self, file: &str, max_age_secs: Option<u64>) -> Option<String> {
        let path = self.cache.join(file);
        if let Some(max) = max_age_secs {
            let age = SystemTime::now()
                .duration_since(fs::metadata(&path).ok()?.modified().ok()?)
                .ok()?;
            if age.as_secs() > max {
                return None;
            }
        }
        fs::read_to_string(path).ok()
    }

    /// Lädt eine Seite herunter (höchstens 5 MB) und legt sie im Cache ab.
    /// `latin1`: MTGTop8 liefert ISO-8859-1, magic.gg UTF-8.
    fn download(&self, url: &str, file: &str, latin1: bool) -> Result<String> {
        let bytes = self
            .http
            .get(url, "*/*")
            .and_then(|b| b.ok_or_else(|| anyhow::anyhow!("404")))
            .with_context(|| tr!("meta.abruf_fehlgeschlagen", url = url))?;
        let text: String = if latin1 {
            bytes.iter().map(|&b| b as char).collect()
        } else {
            String::from_utf8_lossy(&bytes).into_owned()
        };
        // Ein Cache-Schreibfehler ist unkritisch: dann wird beim nächsten Mal neu geladen.
        let _ = fs::write(self.cache.join(file), &text);
        Ok(text)
    }

    /// Holt eine MTGTop8-Seite, mit Cache und Wartezeit.
    fn top8(&mut self, path: &str, file: &str, max_age_secs: Option<u64>) -> Result<String> {
        if let Some(hit) = self.cached(file, max_age_secs) {
            return Ok(hit);
        }
        throttle(&mut self.last_top8, self.delays.top8);
        self.download(&format!("{TOP8}/{path}"), file, true)
    }

    /// Holt eine magic.gg-Seite, mit Cache und Wartezeit.
    fn gg(&mut self, path: &str, file: &str, max_age_secs: Option<u64>) -> Result<String> {
        if let Some(hit) = self.cached(file, max_age_secs) {
            return Ok(hit);
        }
        throttle(&mut self.last_gg, self.delays.gg);
        self.download(&format!("{GG}/{path}"), file, false)
    }

    /// Events mit Decklisten auf magic.gg, deren Kennung `format` enthält.
    pub fn gg_events(&mut self, format: &str, force: bool) -> Result<Vec<GgEvent>> {
        let max_age = if force { Some(0) } else { Some(12 * HOUR) };
        let html = self.gg("decklists", "gg_index.html", max_age)?;
        let list = magicgg::parse_index(&html, format);
        if list.is_empty() {
            anyhow::bail!(tr!("meta.magic_gg_wurden_dieses"));
        }
        Ok(list)
    }

    /// Bereits heruntergeladene magic.gg-Events eines Formats mit ihren Decks –
    /// ohne Netzwerkzugriff und unabhängig vom Alter der Übersicht.
    pub fn cached_gg(&self, format: &str) -> Vec<(GgEvent, Vec<GgDeck>)> {
        let Some(index) = self.cached("gg_index.html", None) else {
            return Vec::new();
        };
        magicgg::parse_index(&index, format)
            .into_iter()
            .filter_map(|e| {
                let html = self.cached(&magicgg::cache_file(&e.slug), None)?;
                let decks = magicgg::parse_event(&html);
                (!decks.is_empty()).then_some((e, decks))
            })
            .collect()
    }

    /// Decks eines magic.gg-Events. Veröffentlichte Listen ändern sich nicht
    /// mehr – dauerhaft cachen.
    pub fn gg_decks(&mut self, slug: &str) -> Result<Vec<GgDeck>> {
        let file = magicgg::cache_file(slug);
        let html = self.gg(&format!("decklists/{slug}"), &file, None)?;
        let decks = magicgg::parse_event(&html);
        if decks.is_empty() {
            let _ = fs::remove_file(self.cache.join(&file));
            anyhow::bail!(tr!("meta.magic_gg_hat_dieses"));
        }
        Ok(decks)
    }

    /// Archetypen eines Formats mit Meta-Anteil (Standardzeitraum der Seite).
    pub fn archetypes(&mut self, format: &str, force: bool) -> Result<Vec<Archetype>> {
        let max_age = if force { Some(0) } else { Some(12 * HOUR) };
        if force {
            // Auch die Deck-Übersichten der Archetypen neu holen.
            let prefix = format!("arch_{format}_");
            for entry in fs::read_dir(&self.cache).into_iter().flatten().flatten() {
                if entry.file_name().to_string_lossy().starts_with(&prefix) {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }
        let html = self.top8(
            &format!("format?f={format}"),
            &format!("format_{format}.html"),
            max_age,
        )?;
        let list = parse_archetypes(&html);
        if list.is_empty() {
            anyhow::bail!(tr!("meta.mtgtop8_wurden_dieses_format"));
        }
        Ok(list)
    }

    /// Aktuelle Turnierdecks eines Archetyps (neueste zuerst).
    pub fn decks(&mut self, format: &str, arch: &Archetype) -> Result<Vec<DeckRef>> {
        let html = self.top8(
            &format!("archetype?a={}&meta={}&f={format}", arch.id, arch.meta),
            &format!("arch_{format}_{}.html", arch.id),
            Some(12 * HOUR),
        )?;
        Ok(parse_decks(&html))
    }

    /// Wie [`Fetcher::decks`], aber nur aus dem Cache – ohne Netzwerkzugriff.
    /// Auch ältere Seiten zählen: Beim Start ist eine etwas ältere Deckliste
    /// besser als keine; neu geladen wird über „Aktualisieren“/„Alle bewerten“.
    pub fn cached_decks(&self, format: &str, arch: &Archetype) -> Option<Vec<DeckRef>> {
        let html = self.cached(&format!("arch_{format}_{}.html", arch.id), None)?;
        Some(parse_decks(&html))
    }

    /// Bereits heruntergeladene Deckliste, falls vorhanden.
    pub fn cached_decklist(&self, deck_id: u32) -> Option<String> {
        self.cached(&format!("deck_{deck_id}.txt"), None)
            .filter(|t| !t.contains("<html") && !t.trim().is_empty())
    }

    /// Deckliste als Text (MTGO-Format). Listen ändern sich nie -> dauerhaft cachen.
    pub fn decklist(&mut self, deck_id: u32) -> Result<String> {
        let text = self.top8(
            &format!("mtgo?d={deck_id}"),
            &format!("deck_{deck_id}.txt"),
            None,
        )?;
        if text.contains("<html") || text.trim().is_empty() {
            let _ = fs::remove_file(self.cache.join(format!("deck_{deck_id}.txt")));
            anyhow::bail!(tr!("meta.mtgtop8_hat_keine_deckliste"));
        }
        Ok(text)
    }

    /// Namen aller in Arena verfügbaren, im Format legalen Karten (Scryfall).
    pub fn legal_names(&self, scryfall_format: &str) -> Result<HashSet<String>> {
        let file = format!("legal_{scryfall_format}.json");
        if let Some(hit) = self.cached(&file, Some(7 * 24 * HOUR))
            && let Ok(names) = serde_json::from_str::<Vec<String>>(&hit)
        {
            return Ok(names.into_iter().collect());
        }
        let mut names: Vec<String> = Vec::new();
        let mut url = format!(
            "https://api.scryfall.com/cards/search?q=f%3A{scryfall_format}+game%3Aarena&unique=cards"
        );
        loop {
            let bytes = self
                .http
                .get(&url, "application/json")
                .and_then(|b| b.ok_or_else(|| anyhow::anyhow!("404")))
                .map_err(|e| anyhow::anyhow!(tr!("meta.scryfall", e = format!("{e:#}"))))?;
            let page: serde_json::Value = serde_json::from_slice(&bytes)?;
            let (found, next) = parse_scryfall_page(&page);
            names.extend(found);
            match next {
                Some(next) => url = next,
                None => break,
            }
            sleep(Duration::from_millis(120)); // Scryfall bittet um 50–100 ms Abstand
        }
        if names.is_empty() {
            anyhow::bail!(tr!("meta.scryfall_kennt_dieses_format"));
        }
        // Cache-Schreibfehler unkritisch (siehe `download`).
        let _ = fs::write(self.cache.join(&file), serde_json::to_string(&names)?);
        Ok(names.into_iter().collect())
    }
}

/// Archetypen aus der MTGTop8-Formatseite (jede Kennung nur einmal).
pub fn parse_archetypes(html: &str) -> Vec<Archetype> {
    let re = Regex::new(
        r"archetype\?a=(\d+)&meta=(\d+)&f=\w+>([^<]+)</a></div>\s*<div>\s*<div[^>]*>\s*(\d+)\s*%",
    )
    .expect("gültiger Ausdruck");
    let mut seen = HashSet::new();
    re.captures_iter(html)
        .filter_map(|c| {
            let id: u32 = c[1].parse().ok()?;
            seen.insert(id).then(|| Archetype {
                id,
                meta: c[2].parse().unwrap_or(0),
                name: decode_entities(&c[3]),
                share: c[4].parse().unwrap_or(0),
            })
        })
        .collect()
}

/// Turnierdecks aus einer MTGTop8-Archetypseite.
pub fn parse_decks(html: &str) -> Vec<DeckRef> {
    let re = Regex::new(
        r"(?s)event\?e=\d+&d=(\d+)&f=\w+>[^<]*</a></td>\s*<td><a class=player[^>]*>([^<]*)</a></td>\s*<td><a[^>]*>([^<]*)</a></td>\s*<td[^>]*>.*?</td>\s*<td>([^<]*)</td>\s*<td>([^<]*)</td>",
    )
    .expect("gültiger Ausdruck");
    re.captures_iter(html)
        .filter_map(|c| {
            Some(DeckRef {
                id: c[1].parse().ok()?,
                player: decode_entities(&c[2]),
                event: decode_entities(&c[3]),
                rank: decode_entities(&c[4]),
                date: decode_entities(&c[5]),
            })
        })
        .collect()
}

/// Kartennamen (normalisiert, bei Doppelkarten auch die Vorderseite) und
/// Adresse der nächsten Seite einer Scryfall-Suchantwort.
pub fn parse_scryfall_page(page: &serde_json::Value) -> (Vec<String>, Option<String>) {
    let mut names = Vec::new();
    for card in page["data"].as_array().into_iter().flatten() {
        if let Some(n) = card["name"].as_str() {
            let key = name_key(n);
            if let Some((front, _)) = key.split_once(" // ") {
                names.push(front.to_string());
            }
            names.push(key);
        }
    }
    let next = match page["next_page"].as_str() {
        Some(next) if page["has_more"].as_bool() == Some(true) => Some(next.to_string()),
        _ => None,
    };
    (names, next)
}

#[cfg(test)]
#[path = "tests/meta.rs"]
mod tests;
