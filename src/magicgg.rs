//! Official decklists from magic.gg (championships, Pro Tours, Arena events,
//! weekly ranked decks). Only parsing the pages – fetching with cache lives in
//! [`crate::meta::Fetcher`].
//!
//! Structure of an event page: per deck a `<deck-list deck-title=… event-date=…
//! event-name=… format=…>` with `<main-deck>` and `<side-board>` as a text list.

use std::sync::LazyLock;

use regex::Regex;

use crate::deck::{self, Deck};

/// An event on the overview page.
#[derive(Clone, Debug, PartialEq)]
pub struct GgEvent {
    /// Path part after `/decklists/`.
    pub slug: String,
    pub title: String,
    pub date: String,
}

/// A deck of an event page.
#[derive(Clone, Debug)]
pub struct GgDeck {
    /// Player name or e.g. "Platinum-Mythic Rank Player".
    pub title: String,
    /// Placement or similar, often empty.
    pub subtitle: String,
    pub event: String,
    pub date: String,
    pub format: String,
    pub deck: Deck,
}

static INDEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)<a href="/decklists/([a-z0-9-]+)"[^>]*>(.*?)</a>"#).expect("valid regex")
});
static TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]*>").expect("valid regex"));
static DECK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)<deck-list([^>]*)>(.*?)</deck-list>").expect("valid regex"));
static ATTR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"([a-z-]+)="([^"]*)""#).expect("valid regex"));

fn decode(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&#39;", "'")
        .replace("&#039;", "'")
        .replace("&quot;", "\"")
        .replace("&nbsp;", " ")
        .trim()
        .to_string()
}

/// Text between two tags (`<main-deck>` … `</main-deck>`).
fn section<'a>(body: &'a str, tag: &str) -> &'a str {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    body.find(&open)
        .and_then(|i| {
            let rest = &body[i + open.len()..];
            rest.find(&close).map(|j| &rest[..j])
        })
        .unwrap_or("")
}

/// Events of the overview page whose id contains `format` (e.g. "standard");
/// empty = all. Each id only once, in the order of the page.
pub fn parse_index(html: &str, format: &str) -> Vec<GgEvent> {
    let mut out: Vec<GgEvent> = Vec::new();
    for c in INDEX.captures_iter(html) {
        let slug = c[1].to_string();
        if (!format.is_empty() && !slug.contains(format)) || out.iter().any(|e| e.slug == slug) {
            continue;
        }
        let texts: Vec<String> = TAG
            .split(&c[2])
            .map(decode)
            .filter(|t| !t.is_empty())
            .collect();
        let Some(title) = texts.first() else { continue };
        out.push(GgEvent {
            slug,
            title: title.split_whitespace().collect::<Vec<_>>().join(" "),
            date: texts.get(1).cloned().unwrap_or_default(),
        });
    }
    out
}

/// All decks of an event page.
pub fn parse_event(html: &str) -> Vec<GgDeck> {
    DECK.captures_iter(html)
        .filter_map(|c| {
            let attr = |name: &str| {
                ATTR.captures_iter(&c[1])
                    .find(|a| &a[1] == name)
                    .map(|a| decode(&a[2]))
                    .unwrap_or_default()
            };
            let body = &c[2];
            let main = deck::parse(section(body, "main-deck")).main;
            let side = deck::parse(section(body, "side-board")).main;
            (!main.is_empty()).then(|| GgDeck {
                title: attr("deck-title"),
                subtitle: attr("subtitle"),
                event: attr("event-name"),
                date: attr("event-date"),
                format: attr("format"),
                deck: Deck { main, side },
            })
        })
        .collect()
}

/// Fixed id for deck `index` of an event: in the upper number range, so it
/// doesn't collide with MTGTop8 ids, own lists or suggestions.
/// Always from 2³¹ on – above the MTGTop8 deck numbers.
pub fn deck_id(slug: &str, index: usize) -> u32 {
    // FNV-1a
    let mut h: u32 = 0x811c_9dc5;
    for b in slug.bytes().chain(index.to_le_bytes()) {
        h ^= u32::from(b);
        h = h.wrapping_mul(0x0100_0193);
    }
    0x8000_0000 | (h & 0x7fff_ffff)
}

/// File name in the cache for an event page.
pub fn cache_file(slug: &str) -> String {
    let safe: String = slug
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("gg_{safe}.html")
}

#[cfg(test)]
#[path = "tests/magicgg.rs"]
mod tests;
