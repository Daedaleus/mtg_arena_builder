//! Lädt die Kartendatenbank von MTG Arena (SQLite).

use super::*;

pub(super) fn parse_list<T: std::str::FromStr>(s: &str) -> Vec<T> {
    s.split(',')
        .filter_map(|p| p.split(':').next())
        .filter_map(|p| p.trim().parse().ok())
        .collect()
}

/// Manawert aus Arenas Schreibweise, z. B. `o2oUoB` oder `o1o(R/G)`.
pub(super) fn cmc_of(mana: &str) -> u32 {
    mana.split('o')
        .filter(|t| !t.is_empty())
        .map(|t| match t.parse::<u32>() {
            Ok(n) => n,
            Err(_) if t.eq_ignore_ascii_case("x") => 0,
            Err(_) => 1,
        })
        .sum()
}

/// Lesbare Manakosten: `o2oUoB` -> `{2}{U}{B}`.
pub(super) fn pretty_mana(mana: &str) -> String {
    mana.split('o')
        .filter(|t| !t.is_empty())
        .map(|t| format!("{{{}}}", t.trim_matches(|c| c == '(' || c == ')')))
        .collect()
}

impl CardDb {
    pub fn load(path: &Path) -> Result<CardDb> {
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .with_context(|| {
            tr!(
                "carddb_load.kartendatenbank_konnte_nicht_geoeffnet",
                v = path.display()
            )
        })?;

        let version: String = conn
            .query_row(
                "SELECT Version FROM Versions WHERE Type = 'Data'",
                [],
                |r| r.get(0),
            )
            .unwrap_or_default();

        let mut stmt = conn.prepare(&cards_query(&conn))?;
        let rows = stmt.query_map([], card_from_row)?;
        let info = ability_info(&conn)?;
        let mut cards = Vec::new();
        for card in rows {
            let mut card = card?;
            if card.name.is_empty() {
                continue;
            }
            apply_abilities(&mut card, &info);
            cards.push(card);
        }
        Ok(CardDb::from_cards(cards, version))
    }
}

/// SQL für alle Karten; mit deutschem Namen, falls die Datenbank ihn enthält.
fn cards_query(conn: &Connection) -> String {
    let has_de: bool = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE name = 'Localizations_deDE'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .map(|n| n > 0)
        .unwrap_or(false);
    let (de_col, de_join) = if has_de {
        (
            "de.Loc",
            "LEFT JOIN Localizations_deDE de ON de.LocId = c.TitleId AND de.Formatted = 1",
        )
    } else {
        ("NULL", "")
    };
    format!(
        "SELECT c.GrpId, en.Loc, {de_col}, c.Rarity, c.ExpansionCode, c.CollectorNumber,
                c.IsPrimaryCard, c.OldSchoolManaText, c.Types, c.Subtypes, c.Supertypes,
                c.ColorIdentity, c.LinkedFaceType, c.AbilityIds, c.Power, c.Toughness,
                c.DigitalReleaseSet
         FROM Cards c
         JOIN Localizations_enUS en ON en.LocId = c.TitleId AND en.Formatted = 1
         {de_join}
         WHERE c.IsToken = 0 AND c.Rarity > 0"
    )
}

/// Eine Karte aus einer Zeile der Kartenabfrage (Rollen kommen später dazu).
fn card_from_row(r: &rusqlite::Row) -> rusqlite::Result<Card> {
    let text = |r: &rusqlite::Row, i: usize| -> String {
        r.get::<_, Option<String>>(i)
            .ok()
            .flatten()
            .unwrap_or_default()
    };
    let mana = text(r, 7);
    let colors = parse_list::<u8>(&text(r, 11))
        .into_iter()
        .filter(|c| (1..=5).contains(c))
        .fold(0u8, |m, c| m | (1 << (c - 1)));
    Ok(Card {
        grp_id: r.get::<_, i64>(0)? as u32,
        name: strip_tags(&text(r, 1)),
        name_de: r
            .get::<_, Option<String>>(2)
            .ok()
            .flatten()
            .map(|s| strip_tags(&s))
            .filter(|s| !s.is_empty() && !s.starts_with('#')),
        rarity: Rarity::from_arena(r.get::<_, i64>(3)? as u8),
        set: text(r, 4),
        collector: text(r, 5),
        is_primary: r.get::<_, i64>(6).unwrap_or(0) != 0,
        cmc: cmc_of(&mana),
        mana_cost: pretty_mana(&mana),
        types: parse_list(&text(r, 8)),
        subtypes: parse_list(&text(r, 9)),
        basic: parse_list::<u8>(&text(r, 10)).contains(&1),
        colors,
        back_face: r.get::<_, i64>(12).unwrap_or(0) == 1,
        abilities: parse_list(&text(r, 13)),
        power: text(r, 14),
        toughness: text(r, 15),
        regular: text(r, 16).is_empty(),
        roles: 0,
        burn: 0,
        enters_tapped: false,
        mana: None,
    })
}

/// Fähigkeitstexte, aus denen Rollen (Removal, Counter, Kartenziehen) folgen.
fn ability_info(conn: &Connection) -> Result<AbilityInfo> {
    let mut info = AbilityInfo::default();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT a.Id, l.Loc FROM Abilities a
         JOIN Localizations_enUS l ON l.LocId = a.TextId AND l.Formatted = 1",
    ) {
        let found = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)? as u32,
                r.get::<_, Option<String>>(1)?.unwrap_or_default(),
            ))
        })?;
        for (id, text) in found.flatten() {
            info.add(id, &text);
        }
    }

    Ok(info)
}
