//! Deckvorschläge ausschließlich aus Karten der eigenen Sammlung.
//!
//! Für jede Farbe und jedes Farbpaar wird ein 60-Karten-Deck zusammengestellt:
//! die stärksten eigenen Karten auf einer sinnvollen Manakurve, genug Kreaturen
//! und Removal, dazu passende Länder. Die Kartenstärke ist eine Schätzung aus
//! Seltenheit, Turnier-Präsenz, Werten und Kartentext – kein Ersatz für ein
//! abgestimmtes Turnierdeck, aber deutlich besser als zufällig gemischte Karten.

use std::collections::{HashMap, HashSet};

use crate::carddb::{Card, CardDb, ROLE_DRAW, ROLE_REMOVAL};
use crate::deck::{Deck, DeckEntry};

const TYPE_CREATURE: u8 = 2;
const DECK_SIZE: u32 = 60;
const LANDS: u32 = 24;
const SPELLS: u32 = DECK_SIZE - LANDS;
const COLOR_NAMES: [&str; 5] = ["Weiß", "Blau", "Schwarz", "Rot", "Grün"];
const BASICS: [&str; 5] = ["Plains", "Island", "Swamp", "Mountain", "Forest"];
const COLOR_LETTERS: [char; 5] = ['W', 'U', 'B', 'R', 'G'];

#[derive(Clone, Debug)]
pub struct Built {
    pub label: String,
    pub score: f32,
    pub deck: Deck,
    pub creatures: u32,
    pub removal: u32,
    pub avg_cmc: f32,
    pub rares: u32,
    pub meta_cards: u32,
}

fn is_creature(c: &Card) -> bool {
    c.types.contains(&TYPE_CREATURE)
}

/// Geschätzte Stärke einer Karte (höher = besser).
fn rating(card: &Card, meta_use: u32, colors: u8) -> f32 {
    let mut r = match card.rarity {
        5 => 3.0,
        4 => 2.6,
        3 => 1.6,
        _ => 1.0,
    };
    // Karten, die in Turnierdecks gespielt werden, sind erwiesenermaßen gut.
    if meta_use > 0 {
        r += 1.5 + (meta_use as f32).ln().min(2.0) * 0.5;
    }
    if card.has_role(ROLE_REMOVAL) {
        r += 1.5;
    }
    if card.has_role(ROLE_DRAW) {
        r += 0.4;
    }
    if is_creature(card) {
        // Werte im Verhältnis zu den Kosten (eine 3/3 für 3 ist der Maßstab).
        if let (Ok(p), Ok(t)) = (card.power.parse::<f32>(), card.toughness.parse::<f32>()) {
            r += ((p + t) - (2.0 * card.cmc as f32)).clamp(-2.0, 2.0) * 0.2;
        }
        r += (card.abilities.len().min(3) as f32) * 0.15;
    }
    // Mehrfarbige Karten, die genau ins Farbpaar passen, sind meist stärker.
    if card.colors == colors && colors.count_ones() == 2 {
        r += 0.4;
    }
    r
}

fn curve_bucket(cmc: u32) -> usize {
    (cmc.clamp(1, 6) - 1) as usize
}

/// Obergrenzen je Manawert (1, 2, 3, 4, 5, 6+).
const CURVE_CAPS: [u32; 6] = [8, 12, 10, 7, 4, 2];

fn label_of(colors: u8) -> String {
    let names: Vec<&str> = (0..5)
        .filter(|i| colors & (1 << i) != 0)
        .map(|i| COLOR_NAMES[i])
        .collect();
    if names.len() == 1 {
        format!("Mono-{}", names[0])
    } else {
        names.join("-")
    }
}

struct Pick<'a> {
    card: &'a Card,
    qty: u32,
}

fn build_one(
    db: &CardDb,
    owned: &HashMap<String, u32>,
    legal: Option<&HashSet<String>>,
    meta_use: &HashMap<String, u32>,
    colors: u8,
) -> Option<Built> {
    let is_legal = |key: &String| legal.is_none_or(|l| l.contains(key));

    // --- Kandidaten: eigene, legale Nichtland-Karten in den Farben ---
    let mut pool: Vec<(f32, &Card, u32, u32)> = owned
        .iter()
        .filter(|(key, n)| **n > 0 && is_legal(key))
        .filter_map(|(key, n)| {
            let card = db.representative(key)?;
            let meta = meta_use.get(key).copied().unwrap_or(0);
            let fits =
                !card.is_land() && !card.back_face && card.cmc > 0 && card.colors & !colors == 0;
            fits.then(|| (rating(card, meta, colors), card, (*n).min(4), meta))
        })
        .collect();
    pool.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.name.cmp(&b.1.name)));

    // --- Auswahl: beste Karten zuerst, begrenzt durch Kurve und Kartentypen ---
    let mut picks: Vec<Pick> = Vec::new();
    let (mut total, mut score) = (0u32, 0.0f32);
    let mut curve = [0u32; 6];
    let (mut creatures, mut removal, mut rares, mut meta_cards) = (0u32, 0u32, 0u32, 0u32);
    let (mut noncreature, mut other) = (0u32, 0u32);
    for (r, card, copies, meta) in &pool {
        if total >= SPELLS {
            break;
        }
        let bucket = curve_bucket(card.cmc);
        let creature = is_creature(card);
        let rem = card.has_role(ROLE_REMOVAL);
        // Höchstens 16 Nicht-Kreaturen, davon höchstens 6 ohne Removal-Funktion.
        let type_room = if creature {
            SPELLS
        } else if rem {
            16u32.saturating_sub(noncreature)
        } else {
            (6u32.saturating_sub(other)).min(16u32.saturating_sub(noncreature))
        };
        let qty = (*copies)
            .min(SPELLS - total)
            .min(CURVE_CAPS[bucket].saturating_sub(curve[bucket]))
            .min(type_room);
        if qty == 0 {
            continue;
        }
        total += qty;
        curve[bucket] += qty;
        score += r * qty as f32;
        if creature {
            creatures += qty;
        } else {
            noncreature += qty;
            if !rem {
                other += qty;
            }
        }
        if rem {
            removal += qty;
        }
        if card.rarity >= 4 {
            rares += qty;
        }
        if *meta > 0 {
            meta_cards += qty;
        }
        picks.push(Pick { card, qty });
    }
    if total < SPELLS || creatures < 12 {
        return None; // zu wenige spielbare Karten in diesen Farben
    }

    // --- Länder: eigene Zweifarbländer, Rest Standardländer nach Farbbedarf ---
    let mut pips = [0u32; 5];
    for p in &picks {
        for (i, letter) in COLOR_LETTERS.iter().enumerate() {
            pips[i] += p.qty * p.card.mana_cost.matches(*letter).count() as u32;
        }
    }
    let mut lands: Vec<DeckEntry> = Vec::new();
    let mut dual_count = 0u32;
    if colors.count_ones() == 2 {
        let mut duals: Vec<(&Card, u32)> = owned
            .iter()
            .filter(|(key, n)| **n > 0 && is_legal(key))
            .filter_map(|(key, n)| {
                let c = db.representative(key)?;
                (c.is_land() && !c.basic && c.colors == colors).then_some((c, (*n).min(4)))
            })
            .collect();
        // Seltenere Zweifarbländer kommen meist ungetappt – die zuerst.
        duals.sort_by(|a, b| {
            b.0.rarity
                .cmp(&a.0.rarity)
                .then_with(|| a.0.name.cmp(&b.0.name))
        });
        for (card, n) in duals {
            let qty = n.min(8 - dual_count);
            if qty == 0 {
                break;
            }
            dual_count += qty;
            lands.push(DeckEntry {
                qty,
                name: card.name.clone(),
            });
        }
    }
    let basics = LANDS - dual_count;
    let active: Vec<usize> = (0..5).filter(|i| colors & (1 << i) != 0).collect();
    let pip_sum: u32 = active.iter().map(|i| pips[*i]).sum::<u32>().max(1);
    let mut left = basics;
    for (n, i) in active.iter().enumerate() {
        let qty = if n + 1 == active.len() {
            left
        } else {
            let share = (basics as f32 * pips[*i] as f32 / pip_sum as f32).round() as u32;
            share.clamp(basics / 3, basics - basics / 3).min(left)
        };
        left -= qty;
        if qty > 0 {
            lands.push(DeckEntry {
                qty,
                name: BASICS[*i].to_string(),
            });
        }
    }

    // --- Ausgabe: nach Manawert sortiert, Länder am Ende ---
    picks.sort_by(|a, b| {
        a.card
            .cmc
            .cmp(&b.card.cmc)
            .then_with(|| a.card.name.cmp(&b.card.name))
    });
    let cmc_sum: u32 = picks.iter().map(|p| p.card.cmc * p.qty).sum();
    let mut main: Vec<DeckEntry> = picks
        .iter()
        .map(|p| DeckEntry {
            qty: p.qty,
            name: p.card.name.clone(),
        })
        .collect();
    main.extend(lands);
    Some(Built {
        label: label_of(colors),
        score: score / SPELLS as f32,
        deck: Deck {
            main,
            side: Vec::new(),
        },
        creatures,
        removal,
        avg_cmc: cmc_sum as f32 / SPELLS as f32,
        rares,
        meta_cards,
    })
}

/// Baut Vorschläge für alle ein- und zweifarbigen Kombinationen, beste zuerst.
///
/// * `legal` – im Format legale Kartennamen (falls bekannt).
/// * `meta_use` – in wie vielen Turnierlisten eine Karte vorkommt.
pub fn build_all(
    db: &CardDb,
    owned: &HashMap<String, u32>,
    legal: Option<&HashSet<String>>,
    meta_use: &HashMap<String, u32>,
) -> Vec<Built> {
    let mut combos: Vec<u8> = (0..5).map(|i| 1u8 << i).collect();
    for a in 0..5 {
        for b in a + 1..5 {
            combos.push((1 << a) | (1 << b));
        }
    }
    let mut out: Vec<Built> = combos
        .into_iter()
        .filter_map(|colors| build_one(db, owned, legal, meta_use, colors))
        .collect();
    out.sort_by(|a, b| b.score.total_cmp(&a.score));
    out
}
