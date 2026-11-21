//! Analysis of the games: typical mistakes, key moments, opponent cards.

use super::*;

/// A notable moment in a game.
#[derive(Clone, Debug)]
pub struct Finding {
    /// Fixed game number (see [`GameNumbers`]).
    pub game: usize,
    /// Turn; 0 = before the first turn (opening hand).
    pub turn: u32,
    pub text: String,
    /// Cards it is about – the interface shows their images on hover.
    pub cards: Vec<GrpId>,
}

/// A kind of mistake with all its findings.
#[derive(Clone, Debug)]
pub struct Category {
    pub kind: Mistake,
    pub items: Vec<Finding>,
}

impl Category {
    pub fn title(&self) -> String {
        self.kind.title()
    }

    /// What to do instead.
    pub fn advice(&self) -> String {
        self.kind.advice()
    }
}

#[derive(Clone, Debug, Default)]
pub struct Report {
    pub games: usize,
    pub wins: usize,
    pub losses: usize,
    pub on_play: (usize, usize),
    pub on_draw: (usize, usize),
    pub conceded: usize,
    pub avg_turns: f32,
    pub my_turns: u32,
    pub discards: u32,
    /// Opening hands with 2–4 lands: (wins, games); other opening hands likewise.
    pub good_keeps: (usize, usize),
    pub bad_keeps: (usize, usize),
    /// Kinds of mistakes, most frequent first.
    pub categories: Vec<Category>,
    /// (card name, number of games)
    pub opp_cards: Vec<(String, usize)>,
    /// What dealt the most damage to me: (card name, damage).
    pub damage_sources: Vec<(String, u32)>,
}

impl Report {
    /// Findings of one kind of mistake.
    pub fn count(&self, kind: Mistake) -> usize {
        self.categories
            .iter()
            .find(|c| c.kind == kind)
            .map_or(0, |c| c.items.len())
    }
}

pub(super) fn card(db: &CardDb, grp: GrpId) -> Option<&Card> {
    db.cards.get(&grp)
}

pub(super) fn names(db: &CardDb, grps: &[GrpId]) -> String {
    let mut list: Vec<String> = grps
        .iter()
        .filter_map(|g| card(db, *g))
        .map(|c| c.display_name().to_string())
        .collect();
    list.sort();
    list.dedup();
    list.join(", ")
}

/// Opening hands with this many lands count as good.
pub const GOOD_KEEP_LANDS: std::ops::RangeInclusive<usize> = 2..=4;

/// Analyses the games. Findings carry the fixed number from `numbers`.
pub fn report(games: &[GameRecord], numbers: &GameNumbers, db: &CardDb) -> Report {
    let mut r = Report {
        games: games.len(),
        ..Default::default()
    };
    let mut found = Findings::default();
    let mut opp: HashMap<String, usize> = HashMap::new();
    let mut damage: HashMap<String, u32> = HashMap::new();
    for g in games {
        let nr = numbers.of(g);
        count_result(&mut r, g);
        count_opponent(&mut opp, &mut damage, g, db);
        check_concede(&mut r, &mut found, g, nr);
        check_keep(&mut r, &mut found, g, db, nr);
        // Turn analyses only for completely recorded games: partial
        // recordings don't know the hand and battlefield completely.
        if g.complete {
            check_turns(&mut r, &mut found, g, db, nr);
        }
    }
    if !games.is_empty() {
        let turns: u32 = games.iter().map(GameRecord::last_turn).sum();
        r.avg_turns = turns as f32 / games.len() as f32;
    }
    r.categories = found.into_categories();
    r.opp_cards = most_common(opp, 15);
    r.damage_sources = most_common(damage, 10);
    r
}

/// Findings per kind of mistake (index = [`Mistake`]), collected during the analysis.
#[derive(Default)]
struct Findings([Vec<Finding>; Mistake::ALL.len()]);

impl Findings {
    fn push(&mut self, kind: Mistake, f: Finding) {
        self.0[kind as usize].push(f);
    }

    /// Kinds of mistakes with explanation, most frequent first; findings by game and turn.
    fn into_categories(self) -> Vec<Category> {
        let mut cats: Vec<Category> = Mistake::ALL
            .into_iter()
            .zip(self.0)
            .map(|(kind, mut items)| {
                items.sort_by_key(|f| (f.game, f.turn));
                Category { kind, items }
            })
            .collect();
        cats.sort_by_key(|c| std::cmp::Reverse(c.items.len()));
        cats
    }
}

/// The `n` most frequent entries, ties by name.
fn most_common<N: Ord + Copy>(counts: HashMap<String, N>, n: usize) -> Vec<(String, N)> {
    let mut list: Vec<(String, N)> = counts.into_iter().collect();
    list.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    list.truncate(n);
    list
}

/// Wins, losses, play/draw and discarded cards.
fn count_result(r: &mut Report, g: &GameRecord) {
    match g.won {
        Some(true) => r.wins += 1,
        Some(false) => r.losses += 1,
        None => {}
    }
    if let (Some(play), Some(won)) = (g.on_play, g.won) {
        let slot = if play { &mut r.on_play } else { &mut r.on_draw };
        slot.0 += won as usize;
        slot.1 += 1;
    }
    r.discards += g.discards_me;
}

/// Opponent cards (games they appeared in) and damage per source.
fn count_opponent(
    opp: &mut HashMap<String, usize>,
    damage: &mut HashMap<String, u32>,
    g: &GameRecord,
    db: &CardDb,
) {
    let seen: HashSet<String> = g
        .opp_cards
        .keys()
        .filter_map(|grp| card(db, *grp))
        .filter(|c| !c.basic)
        .map(|c| c.name.clone())
        .collect();
    for n in seen {
        *opp.entry(n).or_insert(0) += 1;
    }
    for (grp, dmg) in &g.damage_taken {
        if let Some(c) = card(db, *grp) {
            *damage.entry(c.name.clone()).or_insert(0) += dmg;
        }
    }
}

/// Conceded although enough life was left.
fn check_concede(r: &mut Report, found: &mut Findings, g: &GameRecord, nr: usize) {
    if g.won != Some(false) || !g.conceded() {
        return;
    }
    r.conceded += 1;
    let (life_me, life_opp) = g.final_life();
    if life_me >= 5 && !g.turns.is_empty() {
        found.push(
            Mistake::EarlyConcede,
            Finding {
                game: nr,
                turn: g.last_turn(),
                text: tr!(
                    "games_report.conceded_life_opponent",
                    life_me = life_me,
                    life_opp = life_opp
                ),
                cards: Vec::new(),
            },
        );
    }
}

/// Opening hand: good with [`GOOD_KEEP_LANDS`] lands, otherwise risky.
fn check_keep(r: &mut Report, found: &mut Findings, g: &GameRecord, db: &CardDb, nr: usize) {
    let hand: Vec<&Card> = g
        .opening_hand
        .iter()
        .filter_map(|grp| card(db, *grp))
        .collect();
    if hand.len() < 6 {
        return;
    }
    let lands = hand.iter().filter(|c| c.is_land()).count();
    let good = GOOD_KEEP_LANDS.contains(&lands);
    let slot = if good {
        &mut r.good_keeps
    } else {
        &mut r.bad_keeps
    };
    slot.0 += (g.won == Some(true)) as usize;
    slot.1 += 1;
    if !good {
        found.push(
            Mistake::RiskyKeep,
            Finding {
                game: nr,
                turn: 0,
                text: tr!(
                    "games_report.kept_cards_lands",
                    hand_n = hand.len(),
                    lands = lands
                ),
                cards: Vec::new(),
            },
        );
    }
}

/// All turns of a completely recorded game.
fn check_turns(r: &mut Report, found: &mut Findings, g: &GameRecord, db: &CardDb, nr: usize) {
    let mut prev_life = START_LIFE;
    let last = g.turns.len().saturating_sub(1);
    for (i, t) in g.turns.iter().enumerate() {
        let lost = prev_life - t.life_me;
        prev_life = t.life_me;
        if t.mine {
            r.my_turns += 1;
            // The game ended in the last turn (win, loss, concession) –
            // often before I could do anything. Judge nothing as missed.
            if i < last {
                check_my_turn(found, t, db, nr);
            }
        } else {
            check_opp_turn(found, t, lost, nr);
        }
    }
    for h in held_cards(g, db) {
        found.push(
            Mistake::HeldCard,
            Finding {
                game: nr,
                turn: h.last_turn,
                text: tr!(
                    "games_report.castable_hand_turns",
                    name = h.card.display_name(),
                    turns = h.turns
                ),
                cards: vec![h.card.grp_id],
            },
        );
    }
}

/// Opponent's turn: took a lot of damage at low life without blocking.
fn check_opp_turn(found: &mut Findings, t: &TurnStat, lost: i32, nr: usize) {
    let before = t.life_me + lost;
    if !t.blocked && t.my_untapped_creatures > 0 && lost >= 3 && before <= 10 {
        found.push(
            Mistake::NoBlock,
            Finding {
                game: nr,
                turn: t.turn,
                text: tr!(
                    "games_report.took_damage_life_creature",
                    lost = lost,
                    before = before,
                    my_untapped_creatures = t.my_untapped_creatures
                ),
                cards: Vec::new(),
            },
        );
    }
}

/// Own turn: land, attack, mana and battlefield.
fn check_my_turn(found: &mut Findings, t: &TurnStat, db: &CardDb, nr: usize) {
    let hand: Vec<&Card> = t.hand.iter().filter_map(|grp| card(db, *grp)).collect();
    let find = |text: String, cards: Vec<GrpId>| Finding {
        game: nr,
        turn: t.turn,
        text,
        cards,
    };
    if !t.land_played
        && let Some(land) = hand.iter().find(|c| c.is_land())
    {
        found.push(
            Mistake::MissedLand,
            find(
                tr!("games_report.hand", name = land.display_name()),
                vec![land.grp_id],
            ),
        );
    }
    let opp_power: i32 = t.opp_creatures.iter().map(|(_, p)| p).sum();
    let skipped = crate::combat::worth_attacking(db, &t.safe_idle, t.life_me, opp_power);
    if !skipped.is_empty() {
        found.push(
            Mistake::SkippedAttack,
            find(
                tr!(
                    "games_report.could_attacked_safely",
                    v = names(db, &skipped)
                ),
                skipped.clone(),
            ),
        );
    }
    if let Some(moment) = key_moment(t, db) {
        found.push(Mistake::BetterTurn, find(moment.text, moment.cards));
    } else if t.lands.is_empty() && t.untapped_lands >= 1 {
        // Older recording without lands: simple check without colors.
        let castable = hand
            .iter()
            .filter(|c| !c.is_land() && c.cmc > 0 && c.cmc <= t.untapped_lands && !c.is_instant())
            .max_by_key(|c| c.cmc);
        if let Some(c) = castable {
            found.push(
                Mistake::UnusedMana,
                find(
                    tr!(
                        "games_report.lands_free_hand",
                        untapped_lands = t.untapped_lands,
                        name = c.display_name(),
                        mana_cost = c.mana_cost
                    ),
                    vec![c.grp_id],
                ),
            );
        }
    }
    if (3..=6).contains(&t.turn) && t.creatures_me == 0 {
        found.push(
            Mistake::EmptyBoard,
            find(
                tr!("games_report.no_creature_yours_battlefield"),
                Vec::new(),
            ),
        );
    }
}
