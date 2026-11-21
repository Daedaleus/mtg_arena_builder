//! Deck suggestions only from cards of the own collection.
//!
//! For every color and every color pair a 60-card deck is put together: the
//! strongest own cards on a sensible mana curve, enough creatures and
//! removal, plus matching lands. Card strength is an estimate from rarity,
//! tournament presence, stats and card text – no substitute for a tuned
//! tournament deck, but much better than randomly mixed cards.

use std::collections::{HashMap, HashSet};

use crate::carddb::{Card, CardDb, ROLE_DRAW, ROLE_REMOVAL, ROLE_WIPE, Rarity, color};
use crate::deck::{Deck, DeckEntry};

const DECK_SIZE: u32 = 60;
const LANDS: u32 = 24;
const SPELLS: u32 = DECK_SIZE - LANDS;

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

/// Estimated strength of a card (higher = better).
fn rating(card: &Card, meta_use: u32, colors: u8, copies: u32) -> f32 {
    let mut r = match card.rarity {
        Rarity::Mythic => 3.0,
        Rarity::Rare => 2.6,
        Rarity::Uncommon => 1.6,
        _ => 1.0,
    };
    // Cards played in tournament decks are proven to be good.
    if meta_use > 0 {
        r += 1.5 + (meta_use as f32).ln().min(2.0) * 0.5;
    }
    if card.has_role(ROLE_REMOVAL) {
        r += 1.5;
    }
    if card.has_role(ROLE_DRAW) {
        r += 0.4;
    }
    if card.is_creature() {
        // Stats relative to the cost (a 3/3 for 3 is the benchmark).
        if let (Ok(p), Ok(t)) = (card.power.parse::<f32>(), card.toughness.parse::<f32>()) {
            r += ((p + t) - (2.0 * card.cmc as f32)).clamp(-2.0, 2.0) * 0.2;
        }
        r += (card.abilities.len().min(3) as f32) * 0.15;
    }
    // Multicolored cards that fit the color pair exactly are usually stronger.
    if card.colors == colors && colors.count_ones() == 2 {
        r += 0.4;
    }
    // Several copies make the deck more predictable than many single cards.
    r += 0.15 * (copies.saturating_sub(1)) as f32;
    // Expensive cards often get stuck in hand.
    if card.cmc >= 5 {
        r -= 0.5;
    }
    // In two-color decks, costs like {U}{U} or {R}{R}{R} are hard to pay.
    if colors.count_ones() == 2 {
        let max_pips = color::LETTERS
            .iter()
            .map(|l| card.mana_cost.matches(*l).count())
            .max()
            .unwrap_or(0);
        r -= 0.3 * max_pips.saturating_sub(1) as f32;
    }
    r
}

fn curve_bucket(cmc: u32) -> usize {
    (cmc.clamp(1, 6) - 1) as usize
}

/// Upper limits per mana value (1, 2, 3, 4, 5, 6+): first a fast curve, and
/// only if the collection isn't enough for it, a more generous one.
const CURVE_CAPS: [[u32; 6]; 2] = [[10, 14, 8, 4, 2, 0], [12, 16, 10, 7, 4, 2]];
/// At most this many non-creatures – many creatures withstand removal better.
const MAX_NONCREATURE: u32 = 14;
/// Of those, at most this many without a removal function.
const MAX_OTHER: u32 = 4;
/// At least this many cards per color in a two-color deck.
const MIN_PER_COLOR: u32 = 6;
/// At least this many creatures.
const MIN_CREATURES: u32 = 12;

/// Own card that would fit into the deck.
struct Candidate<'a> {
    card: &'a Card,
    rating: f32,
    /// Own copies, at most 4.
    copies: u32,
    /// Played in this many tournament lists.
    meta: u32,
}

/// Chosen card with count.
struct Pick<'a> {
    card: &'a Card,
    qty: u32,
}

/// Spells chosen so far and their key figures.
#[derive(Default)]
struct Selection<'a> {
    picks: Vec<Pick<'a>>,
    total: u32,
    score: f32,
    curve: [u32; 6],
    creatures: u32,
    removal: u32,
    rares: u32,
    meta_cards: u32,
    noncreature: u32,
    /// Non-creatures without a removal function.
    other: u32,
}

impl<'a> Selection<'a> {
    /// Best cards first, limited by curve and card types; if the fast curve
    /// isn't enough, it continues with the more generous one.
    fn choose(pool: &[Candidate<'a>]) -> Selection<'a> {
        let mut sel = Selection::default();
        for caps in &CURVE_CAPS {
            for c in pool {
                if sel.total >= SPELLS {
                    return sel;
                }
                let qty = sel.room_for(c, caps);
                if qty > 0 {
                    sel.add(c, qty);
                }
            }
        }
        sel
    }

    fn taken(&self, card: &Card) -> u32 {
        self.picks
            .iter()
            .find(|p| p.card.grp_id == card.grp_id)
            .map_or(0, |p| p.qty)
    }

    /// How many copies still fit (curve, card type, deck size).
    fn room_for(&self, c: &Candidate, caps: &[u32; 6]) -> u32 {
        let bucket = curve_bucket(c.card.cmc);
        let type_room = if c.card.is_creature() {
            SPELLS
        } else if c.card.has_role(ROLE_REMOVAL) {
            MAX_NONCREATURE.saturating_sub(self.noncreature)
        } else {
            (MAX_OTHER.saturating_sub(self.other))
                .min(MAX_NONCREATURE.saturating_sub(self.noncreature))
        };
        (c.copies - self.taken(c.card))
            .min(SPELLS - self.total)
            .min(caps[bucket].saturating_sub(self.curve[bucket]))
            .min(type_room)
    }

    fn add(&mut self, c: &Candidate<'a>, qty: u32) {
        let card = c.card;
        let removal = card.has_role(ROLE_REMOVAL);
        self.total += qty;
        self.curve[curve_bucket(card.cmc)] += qty;
        self.score += c.rating * qty as f32;
        if card.is_creature() {
            self.creatures += qty;
        } else {
            self.noncreature += qty;
            if !removal {
                self.other += qty;
            }
        }
        if removal {
            self.removal += qty;
        }
        if card.rarity >= Rarity::Rare {
            self.rares += qty;
        }
        if c.meta > 0 {
            self.meta_cards += qty;
        }
        match self.picks.iter_mut().find(|p| p.card.grp_id == card.grp_id) {
            Some(p) => p.qty += qty,
            None => self.picks.push(Pick { card, qty }),
        }
    }

    /// Copies with a color (bit).
    fn of_color(&self, bit: u8) -> u32 {
        self.picks
            .iter()
            .filter(|p| p.card.colors & bit != 0)
            .map(|p| p.qty)
            .sum()
    }

    /// Color symbols in the mana costs of all picks (W, U, B, R, G).
    fn pips(&self) -> [u32; 5] {
        let mut pips = [0u32; 5];
        for p in &self.picks {
            for (i, letter) in color::LETTERS.iter().enumerate() {
                pips[i] += p.qty * p.card.mana_cost.matches(*letter).count() as u32;
            }
        }
        pips
    }
}

/// Own cards from the collection that the format allows (at most 4 per card).
fn owned_cards<'a>(
    db: &'a CardDb,
    owned: &'a HashMap<String, u32>,
    legal: Option<&'a HashSet<String>>,
) -> impl Iterator<Item = (&'a String, &'a Card, u32)> + 'a {
    owned
        .iter()
        .filter(move |(key, n)| **n > 0 && legal.is_none_or(|l| l.contains(*key)))
        .filter_map(move |(key, n)| Some((key, db.representative(key)?, (*n).min(4))))
}

/// Own legal non-land cards in the colors, strongest first.
fn candidates<'a>(
    db: &'a CardDb,
    owned: &'a HashMap<String, u32>,
    legal: Option<&'a HashSet<String>>,
    meta_use: &HashMap<String, u32>,
    colors: u8,
) -> Vec<Candidate<'a>> {
    let mut pool: Vec<Candidate> = owned_cards(db, owned, legal)
        .filter(|(_, card, _)| {
            !card.is_land()
                && !card.back_face
                && card.cmc > 0
                && card.colors & !colors == 0
                // Board wipes hurt a creature deck more than they help.
                && !card.has_role(ROLE_WIPE)
        })
        .map(|(key, card, copies)| {
            let meta = meta_use.get(key).copied().unwrap_or(0);
            Candidate {
                card,
                rating: rating(card, meta, colors, copies),
                copies,
                meta,
            }
        })
        .collect();
    pool.sort_by(|a, b| {
        b.rating
            .total_cmp(&a.rating)
            .then_with(|| a.card.name.cmp(&b.card.name))
    });
    pool
}

/// Lands: own dual lands (at most 8), the rest basic lands by color need –
/// every color at least a third.
fn mana_base(
    db: &CardDb,
    owned: &HashMap<String, u32>,
    legal: Option<&HashSet<String>>,
    colors: u8,
    pips: [u32; 5],
) -> Vec<DeckEntry> {
    let mut lands: Vec<DeckEntry> = Vec::new();
    let mut dual_count = 0u32;
    if colors.count_ones() == 2 {
        let mut duals: Vec<(&Card, u32)> = owned_cards(db, owned, legal)
            .filter(|(_, c, _)| c.is_land() && !c.basic && c.colors == colors)
            .map(|(_, c, n)| (c, n))
            .collect();
        // Rarer dual lands usually enter untapped – those first.
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
                name: color::BASIC_LANDS[*i].to_string(),
            });
        }
    }
    lands
}

fn build_one(
    db: &CardDb,
    owned: &HashMap<String, u32>,
    legal: Option<&HashSet<String>>,
    meta_use: &HashMap<String, u32>,
    colors: u8,
) -> Option<Built> {
    let pool = candidates(db, owned, legal, meta_use, colors);
    let mut sel = Selection::choose(&pool);
    if sel.total < SPELLS || sel.creatures < MIN_CREATURES {
        return None; // too few playable cards in these colors
    }
    // Every color of a two-color deck needs its own cards – otherwise it would
    // be a mono-colored deck with useless lands of the second color.
    if colors.count_ones() == 2
        && (0..5)
            .map(|i| 1u8 << i)
            .any(|bit| colors & bit != 0 && sel.of_color(bit) < MIN_PER_COLOR)
    {
        return None;
    }
    let lands = mana_base(db, owned, legal, colors, sel.pips());

    // Output: sorted by mana value, lands at the end.
    sel.picks.sort_by(|a, b| {
        a.card
            .cmc
            .cmp(&b.card.cmc)
            .then_with(|| a.card.name.cmp(&b.card.name))
    });
    let cmc_sum: u32 = sel.picks.iter().map(|p| p.card.cmc * p.qty).sum();
    let mut main: Vec<DeckEntry> = sel
        .picks
        .iter()
        .map(|p| DeckEntry {
            qty: p.qty,
            name: p.card.name.clone(),
        })
        .collect();
    main.extend(lands);
    Some(Built {
        label: color::label(&color::letters(colors)),
        score: sel.score / SPELLS as f32,
        deck: Deck {
            main,
            ..Default::default()
        },
        creatures: sel.creatures,
        removal: sel.removal,
        avg_cmc: cmc_sum as f32 / SPELLS as f32,
        rares: sel.rares,
        meta_cards: sel.meta_cards,
    })
}

/// Builds suggestions for all mono- and two-colored combinations, best first.
///
/// * `legal` – card names legal in the format (if known).
/// * `meta_use` – in how many tournament lists a card appears.
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

#[cfg(test)]
#[path = "tests/builder.rs"]
mod tests;
