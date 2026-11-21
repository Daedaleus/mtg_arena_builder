//! Tournament decks from MTGTop8 and magic.gg, format legality from Scryfall.
//!
//! MTGTop8 asks for 5 seconds between requests in its robots.txt – we keep
//! to that and store everything in the local cache.

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

use anyhow::{Context, Result};
use regex::Regex;

use crate::carddb::name_key;
pub use crate::http::Http;
use crate::magicgg::{self, GgDeck, GgEvent};

/// Game format in Arena (tournament decks, legality, opponent recognition).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Format {
    #[default]
    Standard,
    Pioneer,
    Explorer,
    Historic,
    Alchemy,
}

impl Format {
    pub const ALL: [Format; 5] = [
        Format::Standard,
        Format::Pioneer,
        Format::Explorer,
        Format::Historic,
        Format::Alchemy,
    ];

    /// Code at MTGTop8 (also in `settings.json`).
    pub fn code(self) -> &'static str {
        match self {
            Format::Standard => "ST",
            Format::Pioneer => "PI",
            Format::Explorer => "EXP",
            Format::Historic => "HI",
            Format::Alchemy => "ALCH",
        }
    }

    /// Display name (the same in all Arena languages).
    pub fn name(self) -> &'static str {
        match self {
            Format::Standard => "Standard",
            Format::Pioneer => "Pioneer",
            Format::Explorer => "Explorer",
            Format::Historic => "Historic",
            Format::Alchemy => "Alchemy",
        }
    }

    /// Name at Scryfall and magic.gg.
    pub fn scryfall(self) -> &'static str {
        match self {
            Format::Standard => "standard",
            Format::Pioneer => "pioneer",
            Format::Explorer => "explorer",
            Format::Historic => "historic",
            Format::Alchemy => "alchemy",
        }
    }

    /// Format for an MTGTop8 code (case doesn't matter).
    pub fn from_code(code: &str) -> Option<Format> {
        Format::ALL
            .into_iter()
            .find(|f| f.code().eq_ignore_ascii_case(code))
    }
}

const TOP8: &str = "https://www.mtgtop8.com";
const GG: &str = "https://magic.gg";
const HOUR: u64 = 3600;

#[derive(Clone, Debug)]
pub struct Archetype {
    pub id: u32,
    pub name: String,
    /// Share of the metagame in percent.
    pub share: u32,
    /// Period id of the overview page (needed for the archetype link).
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
    /// Network access; keeps the minimum gaps per site itself (see [`crate::http`]).
    http: Arc<dyn Http>,
    cache: PathBuf,
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
    /// With its own real network access (for the command line).
    pub fn new(cache: PathBuf) -> Fetcher {
        Fetcher::with_http(cache, crate::http::shared())
    }

    /// With existing network access – in the app all threads share one, so
    /// the minimum gaps apply jointly.
    pub fn with_http(cache: PathBuf, http: Arc<dyn Http>) -> Fetcher {
        let _ = fs::create_dir_all(&cache);
        Fetcher { http, cache }
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

    /// Downloads a page (at most 5 MB) and stores it in the cache.
    /// `latin1`: MTGTop8 delivers ISO-8859-1, magic.gg UTF-8.
    fn download(&self, url: &str, file: &str, latin1: bool) -> Result<String> {
        let bytes = self
            .http
            .get(url, "*/*")
            .map_err(|e| anyhow::anyhow!(e.user_text()))
            .with_context(|| tr!("meta.fetching_failed", url = url))?;
        let text: String = if latin1 {
            bytes.iter().map(|&b| b as char).collect()
        } else {
            String::from_utf8_lossy(&bytes).into_owned()
        };
        // A cache write error isn't critical: it is simply loaded again next time.
        let _ = fs::write(self.cache.join(file), &text);
        Ok(text)
    }

    /// Fetches an MTGTop8 page, with cache and waiting time.
    fn top8(&mut self, path: &str, file: &str, max_age_secs: Option<u64>) -> Result<String> {
        if let Some(hit) = self.cached(file, max_age_secs) {
            return Ok(hit);
        }
        self.download(&format!("{TOP8}/{path}"), file, true)
    }

    /// Fetches a magic.gg page, with cache and waiting time.
    fn gg(&mut self, path: &str, file: &str, max_age_secs: Option<u64>) -> Result<String> {
        if let Some(hit) = self.cached(file, max_age_secs) {
            return Ok(hit);
        }
        self.download(&format!("{GG}/{path}"), file, false)
    }

    /// Events with decklists on magic.gg whose id contains `format`.
    pub fn gg_events(&mut self, format: &str, force: bool) -> Result<Vec<GgEvent>> {
        let max_age = if force { Some(0) } else { Some(12 * HOUR) };
        let html = self.gg("decklists", "gg_index.html", max_age)?;
        let list = magicgg::parse_index(&html, format);
        if list.is_empty() {
            anyhow::bail!(tr!("meta.no_decklists_found_magic"));
        }
        Ok(list)
    }

    /// Already downloaded magic.gg events of a format with their decks –
    /// without network access and regardless of the age of the overview.
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

    /// Decks of a magic.gg event. Published lists don't change any more –
    /// cache them permanently.
    pub fn gg_decks(&mut self, slug: &str) -> Result<Vec<GgDeck>> {
        let file = magicgg::cache_file(slug);
        let html = self.gg(&format!("decklists/{slug}"), &file, None)?;
        let decks = magicgg::parse_event(&html);
        if decks.is_empty() {
            let _ = fs::remove_file(self.cache.join(&file));
            anyhow::bail!(tr!("meta.magic_gg_returned_no"));
        }
        Ok(decks)
    }

    /// Archetypes of a format with meta share (default period of the page).
    pub fn archetypes(&mut self, format: &str, force: bool) -> Result<Vec<Archetype>> {
        let max_age = if force { Some(0) } else { Some(12 * HOUR) };
        if force {
            // Fetch the deck overviews of the archetypes again as well.
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
            anyhow::bail!(tr!("meta.no_archetypes_found_mtgtop8"));
        }
        Ok(list)
    }

    /// Current tournament decks of an archetype (newest first).
    pub fn decks(&mut self, format: &str, arch: &Archetype) -> Result<Vec<DeckRef>> {
        let html = self.top8(
            &format!("archetype?a={}&meta={}&f={format}", arch.id, arch.meta),
            &format!("arch_{format}_{}.html", arch.id),
            Some(12 * HOUR),
        )?;
        Ok(parse_decks(&html))
    }

    /// Like [`Fetcher::decks`], but only from the cache – without network access.
    /// Older pages count too: at startup a slightly older decklist is better
    /// than none; reloading happens via "Refresh"/"Rate all".
    pub fn cached_decks(&self, format: &str, arch: &Archetype) -> Option<Vec<DeckRef>> {
        let html = self.cached(&format!("arch_{format}_{}.html", arch.id), None)?;
        Some(parse_decks(&html))
    }

    /// Already downloaded decklist, if available.
    pub fn cached_decklist(&self, deck_id: u32) -> Option<String> {
        self.cached(&format!("deck_{deck_id}.txt"), None)
            .filter(|t| !t.contains("<html") && !t.trim().is_empty())
    }

    /// Decklist as text (MTGO format). Lists never change -> cache permanently.
    pub fn decklist(&mut self, deck_id: u32) -> Result<String> {
        let text = self.top8(
            &format!("mtgo?d={deck_id}"),
            &format!("deck_{deck_id}.txt"),
            None,
        )?;
        if text.contains("<html") || text.trim().is_empty() {
            let _ = fs::remove_file(self.cache.join(format!("deck_{deck_id}.txt")));
            anyhow::bail!(tr!("meta.mtgtop8_returned_no_decklist"));
        }
        Ok(text)
    }

    /// Names of all cards available in Arena and legal in the format (Scryfall).
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
            let bytes = match self.http.get(&url, "application/json") {
                Ok(bytes) => bytes,
                // Scryfall answers a search without hits with 404.
                Err(crate::http::FetchError::NotFound) => break,
                Err(e) => anyhow::bail!(tr!("meta.scryfall", e = e.user_text())),
            };
            let page: serde_json::Value = serde_json::from_slice(&bytes)?;
            let (found, next) = parse_scryfall_page(&page);
            names.extend(found);
            match next {
                Some(next) => url = next,
                None => break,
            }
        }
        if names.is_empty() {
            anyhow::bail!(tr!("meta.scryfall_knows_no_arena"));
        }
        // Cache write errors aren't critical (see `download`).
        let _ = fs::write(self.cache.join(&file), serde_json::to_string(&names)?);
        Ok(names.into_iter().collect())
    }
}

/// Archetypes from the MTGTop8 format page (each id only once).
pub fn parse_archetypes(html: &str) -> Vec<Archetype> {
    let re = Regex::new(
        r"archetype\?a=(\d+)&meta=(\d+)&f=\w+>([^<]+)</a></div>\s*<div>\s*<div[^>]*>\s*(\d+)\s*%",
    )
    .expect("valid regex");
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

/// Tournament decks from an MTGTop8 archetype page.
pub fn parse_decks(html: &str) -> Vec<DeckRef> {
    let re = Regex::new(
        r"(?s)event\?e=\d+&d=(\d+)&f=\w+>[^<]*</a></td>\s*<td><a class=player[^>]*>([^<]*)</a></td>\s*<td><a[^>]*>([^<]*)</a></td>\s*<td[^>]*>.*?</td>\s*<td>([^<]*)</td>\s*<td>([^<]*)</td>",
    )
    .expect("valid regex");
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

/// Card names (normalised, for double-faced cards also the front face) and
/// address of the next page of a Scryfall search answer.
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
