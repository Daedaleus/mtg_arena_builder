//! Turnierdecks von MTGTop8 und Format-Legalität von Scryfall.
//!
//! MTGTop8 verlangt in der robots.txt 5 Sekunden Abstand zwischen Anfragen –
//! daran halten wir uns und legen alles im lokalen Cache ab.

use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result, anyhow, bail};
use regex::Regex;

use crate::carddb::name_key;

/// (MTGTop8-Kürzel, Anzeigename, Scryfall-Formatname)
pub const FORMATS: [(&str, &str, &str); 5] = [
    ("ST", "Standard", "standard"),
    ("PI", "Pioneer", "pioneer"),
    ("EXP", "Explorer", "explorer"),
    ("HI", "Historic", "historic"),
    ("ALCH", "Alchemy", "alchemy"),
];

const USER_AGENT: &str = "mtg_arena_builder/0.1 (privates Deckbau-Werkzeug)";
const TOP8: &str = "https://www.mtgtop8.com";
const TOP8_DELAY: Duration = Duration::from_secs(5);
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

pub struct Fetcher {
    agent: ureq::Agent,
    cache: PathBuf,
    last_top8: Option<Instant>,
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
        let _ = fs::create_dir_all(&cache);
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(30))
            .user_agent(USER_AGENT)
            .build();
        Fetcher {
            agent,
            cache,
            last_top8: None,
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

    /// Holt eine MTGTop8-Seite (ISO-8859-1), mit Cache und Wartezeit.
    fn top8(&mut self, path: &str, file: &str, max_age_secs: Option<u64>) -> Result<String> {
        if let Some(hit) = self.cached(file, max_age_secs) {
            return Ok(hit);
        }
        if let Some(last) = self.last_top8 {
            let since = last.elapsed();
            if since < TOP8_DELAY {
                sleep(TOP8_DELAY - since);
            }
        }
        self.last_top8 = Some(Instant::now());
        let url = format!("{TOP8}/{path}");
        let resp = self
            .agent
            .get(&url)
            .call()
            .with_context(|| format!("Abruf von {url} fehlgeschlagen"))?;
        let mut bytes = Vec::new();
        resp.into_reader().take(5_000_000).read_to_end(&mut bytes)?;
        let text: String = bytes.iter().map(|&b| b as char).collect();
        let _ = fs::write(self.cache.join(file), &text);
        Ok(text)
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
            bail!("Auf MTGTop8 wurden für dieses Format keine Archetypen gefunden.");
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
    pub fn cached_decks(&self, format: &str, arch: &Archetype) -> Option<Vec<DeckRef>> {
        let html = self.cached(&format!("arch_{format}_{}.html", arch.id), Some(12 * HOUR))?;
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
            bail!("MTGTop8 hat keine Deckliste geliefert.");
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
            let resp = self
                .agent
                .get(&url)
                .set("Accept", "application/json")
                .call()
                .map_err(|e| anyhow!("Scryfall: {e}"))?;
            let page: serde_json::Value = serde_json::from_reader(resp.into_reader())?;
            let (found, next) = parse_scryfall_page(&page);
            names.extend(found);
            match next {
                Some(next) => url = next,
                None => break,
            }
            sleep(Duration::from_millis(120)); // Scryfall bittet um 50–100 ms Abstand
        }
        if names.is_empty() {
            bail!("Scryfall kennt für dieses Format keine Arena-Karten.");
        }
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
