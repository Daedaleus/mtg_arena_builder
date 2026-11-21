//! The five colors in Arena order (W, U, B, R, G). The index matches the
//! bit in [`Card::colors`](super::Card::colors).

/// Mana symbols.
pub const LETTERS: [char; 5] = ['W', 'U', 'B', 'R', 'G'];
/// Name of a color (index as in [`LETTERS`]) in the selected language, as
/// Arena calls it.
pub fn name(index: usize) -> String {
    match index {
        0 => tr!("carddb_color.white"),
        1 => tr!("carddb_color.blue"),
        2 => tr!("carddb_color.black"),
        3 => tr!("carddb_color.red"),
        4 => tr!("carddb_color.green"),
        _ => "?".to_string(),
    }
}
/// Basic lands (English card names).
pub const BASIC_LANDS: [&str; 5] = ["Plains", "Island", "Swamp", "Mountain", "Forest"];

/// Color mask as letters, e.g. `0b01010` becomes "UR".
pub fn letters(mask: u8) -> String {
    LETTERS
        .iter()
        .enumerate()
        .filter(|(i, _)| mask & (1 << i) != 0)
        .map(|(_, l)| *l)
        .collect()
}

/// Readable name from color letters: "Mono-Red", "Blue-Red"; without color "Unknown".
pub fn label(letters: &str) -> String {
    let of = |c: char| {
        LETTERS
            .iter()
            .position(|l| *l == c)
            .map_or_else(|| "?".to_string(), name)
    };
    match letters.len() {
        0 => tr!("carddb_color.unknown"),
        1 => tr!(
            "carddb_color.mono",
            v = of(letters.chars().next().unwrap_or('?'))
        ),
        _ => letters.chars().map(of).collect::<Vec<_>>().join("-"),
    }
}
