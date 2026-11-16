//! Offizielle Decklisten von magic.gg (Championships, Pro Tours, Arena-Events,
//! wöchentliche Ranglisten-Decks). Nur das Auswerten der Seiten – der Abruf
//! mit Cache steckt in [`crate::meta::Fetcher`].
//!
//! Aufbau einer Event-Seite: je Deck ein `<deck-list deck-title=… event-date=…
//! event-name=… format=…>` mit `<main-deck>` und `<side-board>` als Textliste.

use std::sync::LazyLock;

use regex::Regex;

use crate::deck::{self, Deck};

/// Ein Event auf der Übersichtsseite.
#[derive(Clone, Debug, PartialEq)]
pub struct GgEvent {
    /// Pfadteil nach `/decklists/`.
    pub slug: String,
    pub title: String,
    pub date: String,
}

/// Ein Deck einer Event-Seite.
#[derive(Clone, Debug)]
pub struct GgDeck {
    /// Spielername oder z. B. „Platinum-Mythic Rank Player“.
    pub title: String,
    /// Platzierung o. Ä., oft leer.
    pub subtitle: String,
    pub event: String,
    pub date: String,
    pub format: String,
    pub deck: Deck,
}

static INDEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)<a href="/decklists/([a-z0-9-]+)"[^>]*>(.*?)</a>"#)
        .expect("gültiger Ausdruck")
});
static TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]*>").expect("gültiger Ausdruck"));
static DECK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)<deck-list([^>]*)>(.*?)</deck-list>").expect("gültiger Ausdruck")
});
static ATTR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"([a-z-]+)="([^"]*)""#).expect("gültiger Ausdruck"));

fn decode(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&#39;", "'")
        .replace("&#039;", "'")
        .replace("&quot;", "\"")
        .replace("&nbsp;", " ")
        .trim()
        .to_string()
}

/// Text zwischen zwei Tags (`<main-deck>` … `</main-deck>`).
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

/// Events der Übersichtsseite, deren Kennung `format` enthält (z. B.
/// „standard“); leer = alle. Jede Kennung nur einmal, Reihenfolge der Seite.
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

/// Alle Decks einer Event-Seite.
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

/// Feste Kennung für Deck `index` eines Events: im oberen Zahlenbereich, damit
/// sie nicht mit MTGTop8-Kennungen, eigenen Listen oder Vorschlägen kollidiert.
pub fn deck_id(slug: &str, index: usize) -> u32 {
    // FNV-1a
    let mut h: u32 = 0x811c_9dc5;
    for b in slug.bytes().chain(index.to_le_bytes()) {
        h ^= u32::from(b);
        h = h.wrapping_mul(0x0100_0193);
    }
    0x8000_0000 | (h & 0x7fff_ffff)
}

/// Dateiname im Cache für eine Event-Seite.
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
