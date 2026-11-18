//! Empfehlungen aus den Kennzahlen der Probehände.

use super::*;

/// Empfehlungen aus den Kennzahlen.
pub(super) fn advice(r: &SimReport) -> Vec<String> {
    let pct = |x: f32| (x * 100.0).round() as u32;
    let mut out = Vec::new();
    if r.on_curve[2] < 0.6 {
        out.push(format!(
            "Zu wenige Länder: Nur in {} % der Partien liegen im 3. Zug drei Länder. 1–2 Länder mehr spielen.",
            pct(r.on_curve[2])
        ));
    }
    if r.flood >= 0.25 {
        out.push(format!(
            "Zu viele Länder: In {} % der Partien liegen im 6. Zug noch zwei oder mehr Länder auf der Hand. 1–2 Länder durch günstige Karten ersetzen.",
            pct(r.flood)
        ));
    }
    if r.color_screw >= 0.2 {
        let color = r
            .missing_color
            .map(|i| format!(" (meist {})", crate::carddb::color::NAMES[i]))
            .unwrap_or_default();
        out.push(format!(
            "Farben fehlen: In {} % der Partien war in Zug 2–4 eine Karte nur wegen der Farbe nicht spielbar{color}. Mehr Länder dieser Farbe oder Zweifarbländer.",
            pct(r.color_screw)
        ));
    }
    // Zu langsam nur, wenn man das Rennen nicht selbst gewinnt.
    if r.pressure.low >= 0.4 && r.pressure.ahead < r.pressure.low {
        out.push(format!(
            "Gegen schnelle Decks zu langsam: In {} % der Partien hast du nach Zug {} höchstens 5 Leben. \
             Mehr günstige Kreaturen zum Blocken oder Entfernung mit Manawert 1–2.",
            pct(r.pressure.low),
            r.turns
        ));
    }
    let used: f32 = r.mana_used[..4].iter().sum();
    let avail: f32 = r.mana_avail[..4].iter().sum();
    if avail > 0.0 && used / avail < 0.55 {
        out.push(format!(
            "Kurve zu hoch: In den Zügen 1–4 wird nur {} % des Manas genutzt. Mehr günstige Karten (Manawert 1–2).",
            pct(used / avail)
        ));
    }
    if r.mulligan >= 0.25 {
        out.push(format!(
            "Viele Mulligans ({} %): Die Länderzahl liegt weit weg von etwa 40 % des Decks.",
            pct(r.mulligan)
        ));
    }
    if out.is_empty() {
        out.push("Das Deck läuft in den Probehänden rund – Länder und Kurve passen.".into());
    }
    out
}
