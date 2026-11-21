//! One simulated game: deal (with mulligan), own turn following the turn
//! suggestion, the opponent's turn, measurements per turn – and the totals
//! over all games.

use super::*;

/// The simulation keeps opening hands with this many lands, otherwise mulligan.
const KEEP_LANDS: std::ops::RangeInclusive<usize> = 2..=5;

/// Measurements of a single simulated game.
#[derive(Default)]
struct GameStats {
    mulligan: bool,
    /// At least T lands in play on turn T (T = 1..4).
    on_curve: [bool; 4],
    mana_used: [u32; TURNS],
    mana_avail: [u32; TURNS],
    life: [i32; TURNS],
    opp_life: [i32; TURNS],
    /// On turns 2–4 a card was unaffordable only because of its colors.
    color_screw: bool,
    /// The colors missing then (W, U, B, R, G).
    missing: [bool; 5],
    flood: bool,
}

/// A simulated game from the point of view of the own deck.
pub(super) struct Game<'a> {
    db: &'a CardDb,
    pub(super) on_play: bool,
    library: Vec<GrpId>,
    hand: Vec<GrpId>,
    /// Own lands in play.
    lands: Vec<GrpId>,
    /// Non-lands with a mana ability in play (e.g. Llanowar Elves).
    mana_sources: Vec<GrpId>,
    board: Board,
    /// Mana of the turn suggestion left unused this turn.
    mana_left: u32,
    stats: GameStats,
}

impl<'a> Game<'a> {
    /// Shuffles, draws seven cards and takes one mulligan with too few or too
    /// many lands (London: draw 7, put one card on the bottom of the library –
    /// a land with many lands, otherwise the most expensive card).
    pub(super) fn deal(deck: &[GrpId], db: &'a CardDb, on_play: bool, rng: &mut Rng) -> Game<'a> {
        let mut library = deck.to_vec();
        rng.shuffle(&mut library);
        let mut hand: Vec<GrpId> = library.drain(..7).collect();
        let mulligan = !KEEP_LANDS.contains(&lands_in(db, &hand));
        if mulligan {
            library.append(&mut hand);
            rng.shuffle(&mut library);
            hand = library.drain(..7).collect();
            let bottom = if lands_in(db, &hand) >= 4 {
                hand.iter().position(|g| is_land(db, *g))
            } else {
                hand.iter()
                    .enumerate()
                    .filter(|(_, g)| !is_land(db, **g))
                    .max_by_key(|(_, g)| db.cards.get(g).map_or(0, |c| c.cmc))
                    .map(|(i, _)| i)
            };
            if let Some(i) = bottom {
                library.push(hand.remove(i));
            }
        }
        Game {
            db,
            on_play,
            library,
            hand,
            lands: Vec::new(),
            mana_sources: Vec::new(),
            board: Board::new(),
            mana_left: 0,
            stats: GameStats {
                mulligan,
                ..Default::default()
            },
        }
    }

    /// The opponent's turn: new creature, maybe removal, attack with a trick.
    pub(super) fn opp_turn(&mut self, turn: usize, opponent: &Opponent, rng: &mut Rng) {
        let new = opponent.creature(turn, rng);
        let removal = opponent.removal > 0.0 && rng.chance(opponent.removal);
        let trick = opponent.tricks > 0.0 && rng.chance(opponent.tricks);
        let died = self.board.opp_turn_with(new, removal, trick);
        self.lose_mana_sources(&died);
    }

    /// Own turn: draw, land and spells following the turn suggestion, attack.
    pub(super) fn my_turn(
        &mut self,
        turn: usize,
        opponent: &Opponent,
        habits: Option<&Habits>,
        rng: &mut Rng,
    ) {
        let db = self.db;
        self.board.start_my_turn();
        if (turn > 1 || !self.on_play) && !self.library.is_empty() {
            self.hand.push(self.library.remove(0));
        }
        // Legend rule: only one copy per name in play.
        let board: Vec<GrpId> = self.board.mine.iter().map(|u| u.card).collect();
        let hand = turnplan::without_legend_duplicates(db, &self.hand, &board);
        let plan = turnplan::plan(
            db,
            &hand,
            &self.lands,
            &self.mana_sources,
            false,
            &self.board.opp_creatures(),
            self.board.mine.len() as u32,
        );
        if let Some(land) = plan.land
            && let Some(i) = self.hand.iter().position(|g| *g == land)
        {
            self.lands.push(self.hand.remove(i));
        }
        let mut spells = plan.spells.clone();
        let mut mana_used = plan.mana_used;
        if let Some(h) = habits {
            skip_priciest(db, h, &mut spells, &mut mana_used, rng);
        }
        for g in &spells {
            self.cast(*g);
        }
        self.stats.mana_used[turn - 1] = mana_used;
        self.stats.mana_avail[turn - 1] = plan.mana_total;
        if turn <= 4 && self.lands.len() >= turn {
            self.stats.on_curve[turn - 1] = true;
        }
        let hesitant = habits.is_some_and(|h| rng.chance(h.skip_attack));
        let trick = opponent.tricks > 0.0 && rng.chance(opponent.tricks);
        let died = self.board.my_attack_with(hesitant, trick);
        self.lose_mana_sources(&died);
        self.mana_left = plan.mana_total - plan.mana_used;
    }

    /// Plays a card from the hand.
    fn cast(&mut self, grp: GrpId) {
        let Some(i) = self.hand.iter().position(|h| *h == grp) else {
            return;
        };
        let card = self.hand.remove(i);
        if let Some(c) = self.db.cards.get(&card) {
            if c.mana.is_some() {
                self.mana_sources.push(card);
            }
            self.board.cast(c);
        }
    }

    /// Mana creatures that died produce no more mana.
    fn lose_mana_sources(&mut self, died: &[GrpId]) {
        for grp in died {
            if let Some(i) = self.mana_sources.iter().position(|g| g == grp) {
                self.mana_sources.remove(i);
            }
        }
    }

    /// Measurements at the end of the round.
    pub(super) fn end_turn(&mut self, turn: usize) {
        if turn == FLOOD_TURN {
            self.stats.flood = self.lands.len() >= 5 && lands_in(self.db, &self.hand) >= 2;
        }
        self.stats.life[turn - 1] = self.board.life.max(0);
        self.stats.opp_life[turn - 1] = self.board.opp_life.max(0);
        if (2..=4).contains(&turn) {
            self.check_colors();
        }
    }

    /// Color missing: a card would have been affordable by mana value, but not
    /// with the available colors.
    fn check_colors(&mut self) {
        let sources = turnplan::sources(self.db, &self.lands, &self.mana_sources);
        let have = sources.iter().fold(0u8, |m, c| m | c);
        for c in self.hand.iter().filter_map(|g| self.db.cards.get(g)) {
            if c.is_land()
                || c.cmc == 0
                || c.cmc > self.mana_left
                || turnplan::can_pay(&[c], &sources)
            {
                continue;
            }
            self.stats.color_screw = true;
            let (_, pips) = c.pips();
            for (i, n) in pips.iter().enumerate() {
                if *n > 0 && have & (1 << i) == 0 {
                    self.stats.missing[i] = true;
                }
            }
        }
    }
}

/// Habit "better play possible": sometimes don't cast the most expensive
/// spell of the turn suggestion.
fn skip_priciest(
    db: &CardDb,
    h: &Habits,
    spells: &mut Vec<GrpId>,
    mana_used: &mut u32,
    rng: &mut Rng,
) {
    if spells.is_empty() || !rng.chance(h.weaker_turn) {
        return;
    }
    if let Some((i, cmc)) = spells
        .iter()
        .enumerate()
        .map(|(i, g)| (i, db.cards.get(g).map_or(0, |c| c.cmc)))
        .max_by_key(|(_, cmc)| *cmc)
    {
        spells.remove(i);
        *mana_used -= cmc;
    }
}

/// Totals over all simulated games.
#[derive(Default)]
pub(super) struct Tally {
    games: usize,
    mulligans: usize,
    color_screws: usize,
    floods: usize,
    on_curve: [usize; 4],
    missing: [usize; 5],
    mana_used: [u64; TURNS],
    mana_avail: [u64; TURNS],
    life: [i64; TURNS],
    opp_life: [i64; TURNS],
    low: usize,
    dead: usize,
    won: usize,
    ahead: usize,
    kills: u64,
}

impl Tally {
    pub(super) fn add(&mut self, g: &Game) {
        let (s, b) = (&g.stats, &g.board);
        self.games += 1;
        self.mulligans += s.mulligan as usize;
        self.color_screws += s.color_screw as usize;
        self.floods += s.flood as usize;
        for t in 0..4 {
            self.on_curve[t] += s.on_curve[t] as usize;
        }
        for i in 0..5 {
            self.missing[i] += s.missing[i] as usize;
        }
        for t in 0..TURNS {
            self.mana_used[t] += u64::from(s.mana_used[t]);
            self.mana_avail[t] += u64::from(s.mana_avail[t]);
            self.life[t] += i64::from(s.life[t]);
            self.opp_life[t] += i64::from(s.opp_life[t]);
        }
        self.dead += (b.life <= 0) as usize;
        self.won += (b.opp_life <= 0) as usize;
        self.ahead += (b.life > 0 && b.opp_life < b.life) as usize;
        self.low += (b.life <= 5) as usize;
        self.kills += u64::from(b.kills);
    }

    /// Enter shares and averages into the report.
    pub(super) fn fill(&self, r: &mut SimReport) {
        let n = self.games as f32;
        let share = |k: usize| k as f32 / n;
        r.mulligan = share(self.mulligans);
        r.color_screw = share(self.color_screws);
        r.flood = share(self.floods);
        r.on_curve = self.on_curve.map(share);
        r.missing_color = (0..5)
            .filter(|i| self.missing[*i] > 0)
            .max_by_key(|i| self.missing[*i]);
        for t in 0..TURNS {
            r.mana_used[t] = self.mana_used[t] as f32 / n;
            r.mana_avail[t] = self.mana_avail[t] as f32 / n;
            r.pressure.life[t] = self.life[t] as f32 / n;
            r.pressure.opp_life[t] = self.opp_life[t] as f32 / n;
        }
        r.pressure.low = share(self.low);
        r.pressure.dead = share(self.dead);
        r.pressure.won = share(self.won);
        r.pressure.ahead = share(self.ahead);
        r.pressure.kills = self.kills as f32 / n;
    }
}
