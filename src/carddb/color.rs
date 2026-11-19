//! Die fünf Farben in Arena-Reihenfolge (W, U, B, R, G). Der Index entspricht
//! dem Bit in [`Card::colors`](super::Card::colors).

/// Manasymbole.
pub const LETTERS: [char; 5] = ['W', 'U', 'B', 'R', 'G'];
/// Name einer Farbe (Index wie in [`LETTERS`]) in der eingestellten Sprache,
/// so wie Arena sie nennt.
pub fn name(index: usize) -> String {
    match index {
        0 => tr!("carddb_color.weiss"),
        1 => tr!("carddb_color.blau"),
        2 => tr!("carddb_color.schwarz"),
        3 => tr!("carddb_color.rot"),
        4 => tr!("carddb_color.gruen"),
        _ => "?".to_string(),
    }
}
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
    let of = |c: char| {
        LETTERS
            .iter()
            .position(|l| *l == c)
            .map_or_else(|| "?".to_string(), name)
    };
    match letters.len() {
        0 => tr!("carddb_color.unbekannt"),
        1 => tr!(
            "carddb_color.mono",
            v = of(letters.chars().next().unwrap_or('?'))
        ),
        _ => letters.chars().map(of).collect::<Vec<_>>().join("-"),
    }
}
