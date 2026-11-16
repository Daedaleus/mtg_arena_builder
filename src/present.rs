//! Texte und Sortierschlüssel für die Oberfläche – ohne GUI-Abhängigkeit,
//! damit sie sich testen lassen.

use crate::deck::Summary;

/// Sortierschlüssel „wie teuer ist das Deck für mich“: erst nicht verfügbare
/// Karten, dann ob die Wildcards reichen, dann Rares/Mythics (Mythic zählt doppelt).
pub fn cost_key(s: &Summary, wildcards: [u32; 4]) -> (u32, bool, u32, u32, u32) {
    (
        s.unavailable,
        !s.craftable(wildcards),
        s.need[2] + 2 * s.need[3],
        s.need[1],
        s.need[0],
    )
}

/// Ausführliche Kosten eines Decks für den Tooltip.
pub fn cost_tooltip(s: &Summary) -> String {
    let mut text = format!(
        "{:.0} % vorhanden ({} von {} Karten)\nFehlt: {} Common, {} Uncommon, {} Rare, {} Mythic",
        s.percent().floor(),
        s.total - s.missing,
        s.total,
        s.need[0],
        s.need[1],
        s.need[2],
        s.need[3]
    );
    if s.unavailable > 0 {
        text.push_str(&format!(
            "\n{} Karten gibt es in Arena nicht",
            s.unavailable
        ));
    }
    text
}

/// Wie gut passt ein Deck zur Sammlung?
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fit {
    /// Sammlung noch nicht gelesen.
    Unknown,
    Complete,
    /// Mit den vorhandenen Wildcards herstellbar.
    Craftable,
    TooExpensive,
}

/// Bewertung plus Kurztext: was das Deck an Wildcards kostet.
pub fn fit(s: &Summary, have_collection: bool, wildcards: [u32; 4]) -> (Fit, String) {
    if !have_collection {
        return (Fit::Unknown, "–".into());
    }
    if s.complete() {
        return (Fit::Complete, "komplett".into());
    }
    let mut text = if s.need[2] + s.need[3] > 0 {
        format!("{} R / {} M", s.need[2], s.need[3])
    } else {
        format!("nur {} C/U", s.need[0] + s.need[1])
    };
    if s.unavailable > 0 {
        text.push_str(&format!(" (+{} n. v.)", s.unavailable));
    }
    let level = if s.craftable(wildcards) {
        Fit::Craftable
    } else {
        Fit::TooExpensive
    };
    (level, text)
}

/// Lesbarer Name eines Arena-Events.
pub fn event_label(event: &str) -> String {
    match event {
        "Ladder" => "Rangliste".into(),
        "Traditional_Ladder" => "Rangliste Bo3".into(),
        "Play" => "Spielen".into(),
        "Traditional_Play" => "Spielen Bo3".into(),
        "DualColorPrecons" => "Starterdecks".into(),
        "" => "?".into(),
        e if e.starts_with("WelcomeDeckDuels") => "Willkommens-Duelle".into(),
        e if e.starts_with("ColorChallenge") => "Farb-Challenge".into(),
        e => e.replace('_', " "),
    }
}

/// „vor 5 Min.“ usw. relativ zu `now_ms` (beides Unix-Millisekunden).
pub fn ago_at(now_ms: u64, ms: u64) -> String {
    let secs = now_ms.saturating_sub(ms) / 1000;
    match secs {
        0..=89 => "gerade eben".into(),
        90..=5399 => format!("vor {} Min.", secs / 60),
        5400..=172_799 => format!("vor {} Std.", secs / 3600),
        _ => format!("vor {} Tagen", secs / 86_400),
    }
}

/// Wie [`ago_at`], relativ zur aktuellen Zeit.
pub fn ago(ms: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    ago_at(now, ms)
}

pub fn ratio(won: usize, total: usize) -> String {
    if total == 0 {
        "–".into()
    } else {
        format!("{won} von {total} gewonnen")
    }
}

#[cfg(test)]
#[path = "tests/present.rs"]
mod tests;
