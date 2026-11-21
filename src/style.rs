//! Play style: what kind of deck do you play, what do you win with – and
//! which tournament decks therefore suit you?
//!
//! Decks are roughly split into three types (aggro, midrange, control) by
//! mana curve, share of creatures and interaction (removal, counterspells,
//! card draw). Habits from the own games are added.

use crate::carddb::{Card, CardDb, GrpId, ROLE_COUNTER, ROLE_DRAW, ROLE_REMOVAL};
use crate::deck::Deck;
use crate::games::{GameRecord, Mistake, Report, TurnStat};

/// At least this many games per type before the win rate counts.
pub const MIN_GAMES: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Style {
    Aggro,
    Midrange,
    Control,
}

pub const STYLES: [Style; 3] = [Style::Aggro, Style::Midrange, Style::Control];

impl Style {
    pub fn name(self) -> String {
        match self {
            Style::Aggro => tr!("style.aggro"),
            Style::Midrange => tr!("style.midrange"),
            Style::Control => tr!("style.control"),
        }
    }

    /// Short description for beginners.
    pub fn description(self) -> String {
        match self {
            Style::Aggro => {
                tr!("style.many_cheap_creatures_attack")
            }
            Style::Midrange => {
                tr!("style.solid_creatures_some_removal")
            }
            Style::Control => {
                tr!("style.removal_counterspells_card_draw")
            }
        }
    }
}

/// Key figures of the non-land cards of a deck.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DeckProfile {
    pub avg_cmc: f32,
    /// Share of creatures (0..1).
    pub creatures: f32,
    /// Share of removal, counterspells and card draw (0..1).
    pub interaction: f32,
}

impl DeckProfile {
    pub fn of(cards: &[&Card]) -> DeckProfile {
        let spells: Vec<&&Card> = cards.iter().filter(|c| !c.is_land()).collect();
        if spells.is_empty() {
            return DeckProfile::default();
        }
        let n = spells.len() as f32;
        let share = |f: &dyn Fn(&Card) -> bool| spells.iter().filter(|c| f(c)).count() as f32 / n;
        DeckProfile {
            avg_cmc: spells.iter().map(|c| c.cmc as f32).sum::<f32>() / n,
            creatures: share(&|c| c.is_creature()),
            interaction: share(&|c| {
                !c.is_creature()
                    && (c.has_role(ROLE_REMOVAL)
                        || c.has_role(ROLE_COUNTER)
                        || c.has_role(ROLE_DRAW))
            }),
        }
    }

    /// Profile of a decklist (main deck, by names).
    pub fn of_deck(deck: &Deck, db: &CardDb) -> DeckProfile {
        let cards: Vec<&Card> = deck
            .main
            .iter()
            .filter_map(|e| {
                let c = db.resolve(&e.name).and_then(|k| db.representative(&k))?;
                Some(std::iter::repeat_n(c, e.qty as usize))
            })
            .flatten()
            .collect();
        DeckProfile::of(&cards)
    }

    /// Profile of a deck from grpIds (e.g. own games).
    pub fn of_ids(deck: &[GrpId], db: &CardDb) -> DeckProfile {
        let cards: Vec<&Card> = deck.iter().filter_map(|g| db.cards.get(g)).collect();
        DeckProfile::of(&cards)
    }

    pub fn style(&self) -> Style {
        if self.avg_cmc <= 2.6 && self.creatures >= 0.5 {
            Style::Aggro
        } else if self.interaction >= 0.35 || self.creatures < 0.3 {
            Style::Control
        } else {
            Style::Midrange
        }
    }
}

/// Record with one deck type.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StyleRecord {
    pub style: Style,
    pub games: usize,
    pub wins: usize,
}

impl StyleRecord {
    pub fn rate(&self) -> f32 {
        if self.games == 0 {
            0.0
        } else {
            self.wins as f32 / self.games as f32
        }
    }
}

/// Result of the play style analysis.
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerStyle {
    pub games: usize,
    /// Record per deck type (only played ones), most played first.
    pub records: Vec<StyleRecord>,
    /// Share of own turns (from turn 3) with an attack.
    pub attack_rate: f32,
    pub avg_turns: f32,
    /// Notable habits in words.
    pub traits: Vec<String>,
    pub recommended: Style,
    /// Why this type is recommended.
    pub reason: String,
}

/// From this share of own turns on, a mistake counts as a habit.
const HABIT_SHARE: f32 = 0.15;

/// Analyses the games. `report` is the analysis of the same games.
pub fn analyze(games: &[GameRecord], db: &CardDb, report: &Report) -> PlayerStyle {
    let records = style_records(games, db);
    let attack_rate = attack_rate(games);
    let per_turn = |n: usize| {
        if report.my_turns == 0 {
            0.0
        } else {
            n as f32 / report.my_turns as f32
        }
    };
    let habits = Habits {
        passive: per_turn(
            report.count(Mistake::UnusedMana)
                + report.count(Mistake::BetterTurn)
                + report.count(Mistake::HeldCard),
        ),
        skipped_attacks: per_turn(report.count(Mistake::SkippedAttack)),
    };
    let (recommended, reason) = recommend_style(&records, &habits);
    PlayerStyle {
        games: games.len(),
        traits: traits(attack_rate, report.avg_turns, &habits),
        records,
        attack_rate: attack_rate.unwrap_or(0.0),
        avg_turns: report.avg_turns,
        recommended,
        reason,
    }
}

/// Shares of own turns with typical mistakes.
struct Habits {
    /// Mana unused, better play possible or card held back for long.
    passive: f32,
    /// Safe attack skipped.
    skipped_attacks: f32,
}

impl Habits {
    fn noticeable(&self) -> bool {
        self.passive >= HABIT_SHARE || self.skipped_attacks >= HABIT_SHARE
    }
}

/// Record per deck type (games with known deck and result), most played first.
fn style_records(games: &[GameRecord], db: &CardDb) -> Vec<StyleRecord> {
    let mut records: Vec<StyleRecord> = Vec::new();
    for g in games
        .iter()
        .filter(|g| g.won.is_some() && !g.deck.is_empty())
    {
        let style = DeckProfile::of_ids(&g.deck, db).style();
        let i = match records.iter().position(|r| r.style == style) {
            Some(i) => i,
            None => {
                records.push(StyleRecord {
                    style,
                    games: 0,
                    wins: 0,
                });
                records.len() - 1
            }
        };
        records[i].games += 1;
        records[i].wins += (g.won == Some(true)) as usize;
    }
    records.sort_by_key(|r| std::cmp::Reverse(r.games));
    records
}

/// Share of own turns from turn 3 with an attack (`None` without such turns).
fn attack_rate(games: &[GameRecord]) -> Option<f32> {
    let turns: Vec<&TurnStat> = games
        .iter()
        .filter(|g| g.complete)
        .flat_map(|g| g.turns.iter().filter(|t| t.mine && t.turn >= 3))
        .collect();
    (!turns.is_empty())
        .then(|| turns.iter().filter(|t| t.attacked).count() as f32 / turns.len() as f32)
}

/// Notable habits in words.
fn traits(attack_rate: Option<f32>, avg_turns: f32, habits: &Habits) -> Vec<String> {
    let mut traits = Vec::new();
    if let Some(rate) = attack_rate {
        traits.push(tr!("style.attack_turns_turn_3", v = rate * 100.0 : {:.0}));
    }
    if avg_turns > 0.0 {
        let length = if avg_turns >= 14.0 {
            tr!("style.rather_long_games")
        } else if avg_turns <= 9.0 {
            tr!("style.rather_short_games")
        } else {
            String::new()
        };
        traits.push(
            tr!("style.games_last_turns_average", avg_turns = avg_turns : {:.0}, length = length),
        );
    }
    if habits.passive >= HABIT_SHARE {
        traits.push(tr!("style.often_leave_mana_unused"));
    }
    if habits.skipped_attacks >= HABIT_SHARE {
        traits.push(tr!("style.often_skip_safe_attacks"));
    }
    traits
}

/// Recommended deck type with reasoning: the one with a clearly best win
/// rate, otherwise aggro for passive habits, otherwise the most played.
fn recommend_style(records: &[StyleRecord], habits: &Habits) -> (Style, String) {
    let best = records
        .iter()
        .filter(|r| r.games >= MIN_GAMES)
        .max_by(|a, b| a.rate().total_cmp(&b.rate()));
    let others_best = |s: Style| {
        records
            .iter()
            .filter(|r| r.style != s && r.games >= MIN_GAMES)
            .map(StyleRecord::rate)
            .fold(0.0f32, f32::max)
    };
    match best {
        Some(b) if b.rate() >= others_best(b.style) + 0.1 => (
            b.style,
            tr!(
                "style.win_most_often_decks",
                style = b.style.name(),
                wins = b.wins,
                games = b.games
            ),
        ),
        _ if habits.noticeable() => (Style::Aggro, tr!("style.aggro_decks_easiest_play")),
        _ => match records.first() {
            Some(r) => (
                r.style,
                tr!("style.like_playing_best_too", style = r.style.name()),
            ),
            None => (Style::Aggro, tr!("style.no_games_known_deck")),
        },
    }
}

/// A tournament deck as a recommendation candidate.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub deck_id: u32,
    /// Archetype or name.
    pub label: String,
    /// Where the list comes from, e.g. "Tournament decks".
    pub source: String,
    pub style: Style,
    pub summary: crate::deck::Summary,
}

/// Up to `n` decks of the wanted type, cheapest for the own collection
/// first; per name only the cheapest.
pub fn recommend(
    cands: &[Candidate],
    style: Style,
    wildcards: [u32; 4],
    n: usize,
) -> Vec<Candidate> {
    let mut list: Vec<&Candidate> = cands.iter().filter(|c| c.style == style).collect();
    list.sort_by_key(|c| crate::present::cost_key(&c.summary, wildcards));
    let mut out: Vec<Candidate> = Vec::new();
    for c in list {
        if out.len() >= n {
            break;
        }
        if !out.iter().any(|o| o.label == c.label) {
            out.push(c.clone());
        }
    }
    out
}

#[cfg(test)]
#[path = "tests/style.rs"]
mod tests;
