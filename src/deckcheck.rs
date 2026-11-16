//! „Deck verbessern“: wertet die eigenen Partien je Deck aus und schlägt
//! Karten zum Tauschen vor.
//!
//! Bewusst vorsichtig: Gewinnraten einzelner Karten sind erst nach vielen
//! Partien aussagekräftig. Bis dahin stützen sich die Vorschläge auf das, was
//! auch nach wenigen Partien belastbar ist – wie du eine Karte tatsächlich
//! nutzt, ob dein Mana passt und was Turnierspieler im selben Archetyp spielen.

use std::collections::{HashMap, HashSet};

use crate::carddb::{Card, CardDb, name_key};
use crate::deck::Deck;
use crate::games::GameRecord;
use crate::similar;

/// Ab so vielen vollständigen Partien zeigt die App Gewinnraten je Karte.
pub const MIN_GAMES_FOR_WINRATE: usize = 10;
const TYPE_INSTANT: u8 = 4;

/// Alle Partien, die mit derselben Deckliste gespielt wurden.
#[derive(Clone, Debug)]
pub struct DeckGroup {
    pub label: String,
    /// Deckliste (grpIds, Mehrfachnennung = Anzahl).
    pub deck: Vec<u32>,
    /// Indizes in die Partienliste.
    pub games: Vec<usize>,
    pub wins: usize,
    /// Vorgegebenes Deck eines Events, das man in Arena nicht ändern kann.
    pub fixed: bool,
}

fn event_label(event: &str) -> &str {
    match event {
        "Ladder" | "Traditional_Ladder" => "Rangliste",
        "Play" | "Traditional_Play" => "Spielen",
        "DualColorPrecons" => "Starterdeck",
        e if e.starts_with("WelcomeDeckDuels") => "Willkommens-Duell",
        e if e.starts_with("ColorChallenge") => "Farb-Challenge",
        _ => "Sonstiges",
    }
}

fn is_fixed_event(event: &str) -> bool {
    event.starts_with("WelcomeDeckDuels")
        || event.starts_with("ColorChallenge")
        || event == "DualColorPrecons"
}

/// Fasst die Partien nach Deckliste zusammen, zuletzt gespieltes Deck zuerst.
pub fn group_decks(games: &[GameRecord], db: &CardDb) -> Vec<DeckGroup> {
    let mut groups: Vec<DeckGroup> = Vec::new();
    // Gleiche Karte in anderem Druck = gleiches Deck: Schlüssel über die Namen.
    let key_of = |deck: &[u32]| -> Vec<String> {
        let mut k: Vec<String> = deck
            .iter()
            .map(|g| {
                db.cards
                    .get(g)
                    .map(|c| name_key(&c.name))
                    .unwrap_or_else(|| g.to_string())
            })
            .collect();
        k.sort();
        k
    };
    let mut keys: Vec<Vec<String>> = Vec::new();
    for (i, g) in games.iter().enumerate() {
        if g.deck.is_empty() {
            continue;
        }
        let key = key_of(&g.deck);
        let won = (g.won == Some(true)) as usize;
        match keys.iter().position(|k| *k == key) {
            Some(pos) => {
                groups[pos].games.push(i);
                groups[pos].wins += won;
                groups[pos].fixed &= is_fixed_event(&g.event);
            }
            None => {
                keys.push(key);
                groups.push(DeckGroup {
                    label: String::new(),
                    deck: g.deck.clone(),
                    games: vec![i],
                    wins: won,
                    fixed: is_fixed_event(&g.event),
                });
            }
        }
    }
    for grp in &mut groups {
        let first = &games[grp.games[0]];
        let colors = crate::games::deck_colors(&grp.deck, db);
        // Bekannteste Karte als Erkennungszeichen: seltenste Nichtland-Karte.
        let signature = grp
            .deck
            .iter()
            .filter_map(|g| db.cards.get(g))
            .filter(|c| !c.is_land())
            .max_by_key(|c| (c.rarity, c.cmc))
            .map(|c| c.name.clone())
            .unwrap_or_default();
        grp.label = format!(
            "{} {} – {} ({} Karten, {} Partien)",
            if colors.is_empty() {
                "?"
            } else {
                colors.as_str()
            },
            event_label(&first.event),
            signature,
            grp.deck.len(),
            grp.games.len()
        );
    }
    groups
}

/// Nutzung einer Karte über alle Partien eines Decks.
#[derive(Clone, Debug, Default)]
pub struct CardStat {
    pub key: String,
    pub name: String,
    pub copies: u32,
    pub rarity: u8,
    pub cmc: u32,
    pub land: bool,
    /// In wie vielen Partien gezogen bzw. gewirkt.
    pub seen: usize,
    pub cast: usize,
    /// In wie vielen Partien mindestens drei eigene Züge bezahlbar auf der Hand.
    pub held: usize,
    pub damage: u32,
    /// Siege in Partien, in denen die Karte gezogen wurde.
    pub won_when_seen: usize,
    /// Anteil der Turnierlisten des passenden Archetyps, die die Karte spielen.
    pub meta_share: Option<f32>,
}

#[derive(Clone, Debug, Default)]
pub struct ManaCheck {
    pub games: usize,
    /// Partien, in denen in den ersten drei eigenen Zügen Länder fehlten.
    pub screw: usize,
    /// Partien, in denen später Länder nutzlos auf der Hand lagen.
    pub flood: usize,
    pub lands: u32,
    pub size: u32,
    pub advice: String,
}

#[derive(Clone, Debug, Default)]
pub struct MetaMatch {
    pub id: u32,
    pub archetype: String,
    pub lists: usize,
    /// Übereinstimmung der Nichtland-Karten (0–1).
    pub overlap: f32,
    /// Karten, die mindestens die Hälfte der Turnierlisten spielen, du aber nicht:
    /// (Name, Anteil, Exemplare in deiner Sammlung).
    pub missing: Vec<(String, f32, u32)>,
}

#[derive(Clone, Debug)]
pub struct Swap {
    pub out: String,
    pub reason: String,
    /// Ersatz aus der Sammlung (Name, Begründung).
    pub replacement: Option<(String, String)>,
}

#[derive(Clone, Debug, Default)]
pub struct DeckCheck {
    pub games: usize,
    pub complete: usize,
    /// Partien mit Daten zu gezogenen und gewirkten Karten.
    pub tracked: usize,
    pub wins: usize,
    pub cards: Vec<CardStat>,
    pub mana: ManaCheck,
    pub meta: Option<MetaMatch>,
    pub swaps: Vec<Swap>,
}

impl DeckCheck {
    pub fn show_winrates(&self) -> bool {
        self.tracked >= MIN_GAMES_FOR_WINRATE
    }
}

/// Turnierlisten eines Archetyps für den Vergleich.
pub struct MetaArchetype<'a> {
    pub id: u32,
    pub name: String,
    pub lists: Vec<&'a Deck>,
}

fn nonland_keys(names: impl Iterator<Item = String>, db: &CardDb) -> HashSet<String> {
    names
        .filter_map(|n| db.resolve(&n))
        .filter(|k| db.representative(k).is_some_and(|c| !c.is_land()))
        .collect()
}

fn mana_check(group: &DeckGroup, games: &[&GameRecord], db: &CardDb) -> ManaCheck {
    let cards: Vec<&Card> = group.deck.iter().filter_map(|g| db.cards.get(g)).collect();
    let mut m = ManaCheck {
        lands: cards.iter().filter(|c| c.is_land()).count() as u32,
        size: group.deck.len() as u32,
        ..Default::default()
    };
    for g in games.iter().filter(|g| g.complete) {
        let mine: Vec<_> = g.turns.iter().filter(|t| t.mine).collect();
        if mine.len() < 3 {
            continue;
        }
        m.games += 1;
        // Zu wenig: im 3. eigenen Zug weniger als 3 Länder und kein Land auf der Hand.
        let third = mine[2];
        let land_in_hand = third
            .hand
            .iter()
            .any(|g| db.cards.get(g).is_some_and(|c| c.is_land()));
        if third.lands_me < 3 && !land_in_hand {
            m.screw += 1;
        }
        // Zu viel: ab dem 5. eigenen Zug mindestens 5 Länder im Spiel, zwei weitere
        // auf der Hand und höchstens eine andere Karte.
        let flooded = mine.iter().skip(4).any(|t| {
            let (lands, other): (Vec<_>, Vec<_>) = t
                .hand
                .iter()
                .filter_map(|g| db.cards.get(g))
                .partition(|c| c.is_land());
            t.lands_me >= 5 && lands.len() >= 2 && other.len() <= 1
        });
        m.flood += flooded as usize;
    }
    let share = |n: usize| {
        if m.games == 0 {
            0.0
        } else {
            n as f32 / m.games as f32
        }
    };
    m.advice = if m.games < 3 {
        "Für eine Aussage über die Länderzahl braucht es mindestens 3 vollständige Partien.".into()
    } else if share(m.screw) >= 0.3 && m.screw >= 2 * m.flood.max(1) {
        "Dir fehlten oft Länder: 1–2 Länder mehr spielen (statt der teuersten Karten).".into()
    } else if share(m.flood) >= 0.3 && m.flood >= 2 * m.screw.max(1) {
        "Du hattest oft zu viele Länder: 1–2 Länder durch günstige Karten ersetzen.".into()
    } else {
        "Die Länderzahl passt zu deinen bisherigen Partien.".into()
    };
    m
}

/// Wertet ein Deck aus.
///
/// * `owned` – Sammlung je Kartenschlüssel.
/// * `legal` – im Format legale Karten (falls bekannt), für Ersatzvorschläge.
/// * `meta` – geladene Turnierlisten, nach Archetyp.
pub fn check(
    group: &DeckGroup,
    all_games: &[GameRecord],
    db: &CardDb,
    owned: &HashMap<String, u32>,
    legal: Option<&HashSet<String>>,
    meta: &[MetaArchetype],
) -> DeckCheck {
    let games: Vec<&GameRecord> = group.games.iter().map(|i| &all_games[*i]).collect();
    let mut r = DeckCheck {
        games: games.len(),
        complete: games.iter().filter(|g| g.complete).count(),
        wins: group.wins,
        mana: mana_check(group, &games, db),
        ..Default::default()
    };

    // --- Kartenstatistik (nach Name, Drucke zusammengefasst) ---
    let mut stats: HashMap<String, CardStat> = HashMap::new();
    for grp in &group.deck {
        let Some(c) = db.cards.get(grp) else { continue };
        let key = name_key(&c.name);
        let s = stats.entry(key.clone()).or_insert_with(|| CardStat {
            key,
            name: c.name.clone(),
            rarity: c.rarity,
            cmc: c.cmc,
            land: c.is_land(),
            ..Default::default()
        });
        s.copies += 1;
    }
    let key_of = |grp: &u32| db.cards.get(grp).map(|c| name_key(&c.name));
    // Gewirkte Karten werden erst seit Version 3 der Auswertung mitgeschrieben;
    // ältere Partien (Log schon überschrieben) zählen für die Kartennutzung nicht.
    let tracked: Vec<&&GameRecord> = games
        .iter()
        .filter(|g| g.turns.iter().any(|t| !t.cast.is_empty()))
        .collect();
    r.tracked = tracked.len();
    for g in tracked {
        let seen: HashSet<String> = g.seen_cards().iter().filter_map(key_of).collect();
        let cast: HashSet<String> = g.cast_cards().iter().filter_map(key_of).collect();
        for (key, s) in stats.iter_mut() {
            if seen.contains(key) {
                s.seen += 1;
                s.won_when_seen += (g.won == Some(true)) as usize;
            }
            s.cast += cast.contains(key) as usize;
        }
        for (grp, dmg) in &g.damage_dealt {
            if let Some(s) = key_of(grp).and_then(|k| stats.get_mut(&k)) {
                s.damage += dmg;
            }
        }
        if !g.complete {
            continue;
        }
        // Bezahlbar, aber mindestens drei eigene Züge lang nicht gespielt
        let mut castable: HashMap<String, u32> = HashMap::new();
        for t in g.turns.iter().filter(|t| t.mine) {
            let mut counted = HashSet::new();
            for c in t.hand.iter().filter_map(|grp| db.cards.get(grp)) {
                if !c.is_land()
                    && !c.types.contains(&TYPE_INSTANT)
                    && c.cmc > 0
                    && c.cmc <= t.lands_me
                    && counted.insert(name_key(&c.name))
                {
                    *castable.entry(name_key(&c.name)).or_insert(0) += 1;
                }
            }
        }
        for (key, n) in castable {
            if n >= 3
                && let Some(s) = stats.get_mut(&key)
            {
                s.held += 1;
            }
        }
    }

    // --- Vergleich mit dem ähnlichsten Turnier-Archetyp ---
    let mine = nonland_keys(stats.values().map(|s| s.name.clone()), db);
    let best = meta
        .iter()
        .filter(|a| !a.lists.is_empty())
        .map(|a| {
            let first = nonland_keys(a.lists[0].main.iter().map(|e| e.name.clone()), db);
            let inter = mine.intersection(&first).count() as f32;
            let union = mine.union(&first).count().max(1) as f32;
            (a, inter / union)
        })
        .max_by(|a, b| a.1.total_cmp(&b.1));
    if let Some((arch, overlap)) = best.filter(|(_, o)| *o >= 0.2) {
        let n = arch.lists.len() as f32;
        let mut share: HashMap<String, f32> = HashMap::new();
        for list in &arch.lists {
            let keys: HashSet<String> = list
                .main
                .iter()
                .filter_map(|e| db.resolve(&e.name))
                .collect();
            for k in keys {
                *share.entry(k).or_insert(0.0) += 1.0 / n;
            }
        }
        for s in stats.values_mut() {
            s.meta_share = Some(share.get(&s.key).copied().unwrap_or(0.0));
        }
        let mut missing: Vec<(String, f32, u32)> = share
            .iter()
            .filter(|(k, sh)| **sh >= 0.5 && !stats.contains_key(*k))
            .filter_map(|(k, sh)| {
                let c = db.representative(k)?;
                (!c.basic).then(|| (c.name.clone(), *sh, owned.get(k).copied().unwrap_or(0)))
            })
            .collect();
        missing.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        r.meta = Some(MetaMatch {
            id: arch.id,
            archetype: arch.name.clone(),
            lists: arch.lists.len(),
            overlap,
            missing,
        });
    }

    // --- Tausch-Kandidaten ---
    let colors = group
        .deck
        .iter()
        .filter_map(|g| db.cards.get(g))
        .filter(|c| !c.is_land())
        .fold(0u8, |m, c| m | c.colors);
    let in_deck: HashSet<String> = stats.keys().cloned().collect();
    let mut swaps: Vec<(u32, Swap)> = Vec::new();
    for s in stats.values().filter(|s| !s.land) {
        let mut reasons: Vec<String> = Vec::new();
        let mut weight = 0;
        if s.seen >= 2 && s.cast == 0 {
            reasons.push(format!("in {} Partien gezogen, aber nie gespielt", s.seen));
            weight += 3;
        }
        if s.held >= 2 {
            reasons.push(format!(
                "in {} Partien lange ungespielt auf der Hand",
                s.held
            ));
            weight += 2;
        }
        if s.meta_share.is_some_and(|m| m < 0.1) {
            reasons.push("spielt im Turnier-Archetyp kaum jemand".into());
            weight += 1;
        }
        if weight == 0 {
            continue;
        }
        let replacement = similar::suggest(db, owned, legal, colors, &in_deck, &s.key, 1)
            .into_iter()
            .next()
            .map(|sug| {
                (
                    sug.name,
                    format!("ähnliche Karte aus deiner Sammlung ({}×)", sug.owned),
                )
            });
        swaps.push((
            weight,
            Swap {
                out: s.name.clone(),
                reason: reasons.join("; "),
                replacement,
            },
        ));
    }
    swaps.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.out.cmp(&b.1.out)));
    r.swaps = swaps.into_iter().map(|(_, s)| s).collect();

    let mut cards: Vec<CardStat> = stats.into_values().collect();
    cards.sort_by(|a, b| {
        a.land
            .cmp(&b.land)
            .then(a.cmc.cmp(&b.cmc))
            .then_with(|| a.name.cmp(&b.name))
    });
    r.cards = cards;
    r
}

/// Deckliste als Text für „Eigene Liste prüfen“ bzw. den Arena-Import.
pub fn deck_text(deck: &[u32], db: &CardDb) -> String {
    let mut counts: Vec<(String, u32)> = Vec::new();
    for grp in deck {
        let name = db
            .cards
            .get(grp)
            .map(|c| c.name.clone())
            .unwrap_or_else(|| format!("#{grp}"));
        match counts.iter_mut().find(|(n, _)| *n == name) {
            Some((_, q)) => *q += 1,
            None => counts.push((name, 1)),
        }
    }
    let mut out = String::from("Deck\n");
    for (name, qty) in counts {
        out.push_str(&format!("{qty} {name}\n"));
    }
    out
}

#[cfg(test)]
#[path = "tests/deckcheck.rs"]
mod tests;
