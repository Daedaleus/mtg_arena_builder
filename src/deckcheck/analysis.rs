//! Kartennutzung, Vergleich mit dem Turnier-Archetyp und Tauschvorschläge.

use super::*;

pub(super) const TYPE_INSTANT: u8 = 4;

pub(super) fn nonland_keys(names: impl Iterator<Item = String>, db: &CardDb) -> HashSet<String> {
    names
        .filter_map(|n| db.resolve(&n))
        .filter(|k| db.representative(k).is_some_and(|c| !c.is_land()))
        .collect()
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
