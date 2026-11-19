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
    let mut text = tr!("present.vorhanden_karten_fehlt_common", v = s.percent().floor() : {:.0}, missing = s.total - s.missing, total = s.total, need = s.need[0], need2 = s.need[1], need3 = s.need[2], need4 = s.need[3]);
    if s.unavailable > 0 {
        text.push_str(&tr!(
            "present.karten_gibt_arena_nicht",
            unavailable = s.unavailable
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
        return (Fit::Complete, tr!("present.komplett"));
    }
    let mut text = if s.need[2] + s.need[3] > 0 {
        format!("{} R / {} M", s.need[2], s.need[3])
    } else {
        tr!("present.nur_c_u", need = s.need[0] + s.need[1])
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
pub fn event_label(name: &str) -> String {
    use crate::games::arena::event;
    match name {
        event::LADDER => tr!("present.rangliste"),
        event::LADDER_BO3 => tr!("present.rangliste_bo3"),
        event::PLAY => tr!("present.spielen"),
        event::PLAY_BO3 => tr!("present.spielen_bo3"),
        event::STARTER_DECKS => tr!("present.starterdecks"),
        "" => "?".into(),
        e if e.starts_with(event::WELCOME_DUELS_PREFIX) => tr!("present.willkommens_duelle"),
        e if e.starts_with(event::COLOR_CHALLENGE_PREFIX) => tr!("deckcheck_groups.farb_challenge"),
        e => e.replace('_', " "),
    }
}

/// „vor 5 Min.“ usw. relativ zu `now_ms` (beides Unix-Millisekunden).
pub fn ago_at(now_ms: u64, ms: u64) -> String {
    let secs = now_ms.saturating_sub(ms) / 1000;
    match secs {
        0..=89 => tr!("present.gerade_eben"),
        90..=5399 => tr!("present.vor_min", v = secs / 60),
        5400..=172_799 => tr!("present.vor_std", v = secs / 3600),
        _ => tr!("present.vor_tagen", _400 = secs / 86_400),
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

/// Partien je Seite in „Meine Partien“.
pub const PAGE_SIZE: usize = 10;

/// Seitenaufteilung: (erster Index, Ende exklusiv, Seitenzahl, gültige Seite).
/// Eine zu große Seite wird auf die letzte begrenzt.
pub fn page_range(total: usize, page: usize, per_page: usize) -> (usize, usize, usize, usize) {
    let per_page = per_page.max(1);
    let pages = total.div_ceil(per_page).max(1);
    let page = page.min(pages - 1);
    let start = page * per_page;
    (start, (start + per_page).min(total), pages, page)
}

pub fn ratio(won: usize, total: usize) -> String {
    if total == 0 {
        "–".into()
    } else {
        tr!("present.gewonnen", won = won, total = total)
    }
}

#[cfg(test)]
#[path = "tests/present.rs"]
mod tests;
