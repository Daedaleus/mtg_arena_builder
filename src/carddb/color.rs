//! Die fünf Farben in Arena-Reihenfolge (W, U, B, R, G). Der Index entspricht
//! dem Bit in [`Card::colors`](super::Card::colors).

/// Manasymbole.
pub const LETTERS: [char; 5] = ['W', 'U', 'B', 'R', 'G'];
/// Namen wie in der deutschen Arena-Version.
pub const NAMES: [&str; 5] = ["Weiß", "Blau", "Schwarz", "Rot", "Grün"];
/// Standardländer (englische Kartennamen).
pub const BASIC_LANDS: [&str; 5] = ["Plains", "Island", "Swamp", "Mountain", "Forest"];

/// Farbmaske als Buchstaben, z. B. `0b01010` wird zu „UR“.
pub fn letters(mask: u8) -> String {
    LETTERS
        .iter()
        .enumerate()
        .filter(|(i, _)| mask & (1 << i) != 0)
        .map(|(_, l)| *l)
        .collect()
}

/// Lesbarer Name aus Farbbuchstaben: „Mono-Rot“, „Blau-Rot“; ohne Farbe „Unbekannt“.
pub fn label(letters: &str) -> String {
    let name = |c: char| {
        LETTERS
            .iter()
            .position(|l| *l == c)
            .map_or("?", |i| NAMES[i])
    };
    match letters.len() {
        0 => "Unbekannt".into(),
        1 => format!("Mono-{}", name(letters.chars().next().unwrap_or('?'))),
        _ => letters.chars().map(name).collect::<Vec<_>>().join("-"),
    }
}
