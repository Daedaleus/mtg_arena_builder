//! Auswertung der Partien: typische Fehler, Schlüsselmomente, Gegnerkarten.

use super::*;

/// Ein auffälliger Moment in einer Partie.
#[derive(Clone, Debug)]
pub struct Finding {
    /// Feste Partie-Nummer (siehe [`GameNumbers`]).
    pub game: usize,
    /// Zug; 0 = vor dem ersten Zug (Starthand).
    pub turn: u32,
    pub text: String,
}

/// Eine Art von Fehler mit allen Fundstellen.
#[derive(Clone, Debug)]
pub struct Category {
    pub kind: Mistake,
    pub items: Vec<Finding>,
}

impl Category {
    pub fn title(&self) -> String {
        self.kind.title()
    }

    /// Was man stattdessen tun sollte.
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
    /// Starthand mit 2–4 Ländern: (Siege, Partien); übrige Starthände ebenso.
    pub good_keeps: (usize, usize),
    pub bad_keeps: (usize, usize),
    /// Fehlerarten, häufigste zuerst.
    pub categories: Vec<Category>,
    /// (Kartenname, Anzahl Partien)
    pub opp_cards: Vec<(String, usize)>,
    /// Was mir am meisten Schaden gemacht hat: (Kartenname, Schaden).
    pub damage_sources: Vec<(String, u32)>,
}

impl Report {
    /// Fundstellen einer Fehlerart.
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

/// Starthände mit so vielen Ländern gelten als gut.
pub const GOOD_KEEP_LANDS: std::ops::RangeInclusive<usize> = 2..=4;

/// Wertet die Partien aus. Fundstellen tragen die feste Nummer aus `numbers`.
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
        // Zug-Auswertungen nur für vollständig mitgeschriebene Partien:
        // Teil-Mitschnitte kennen Hand und Feld nicht vollständig.
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

/// Fundstellen je Fehlerart (Index = [`Mistake`]), während der Auswertung gesammelt.
#[derive(Default)]
struct Findings([Vec<Finding>; Mistake::ALL.len()]);

impl Findings {
    fn push(&mut self, kind: Mistake, f: Finding) {
        self.0[kind as usize].push(f);
    }

    /// Fehlerarten mit Erklärung, häufigste zuerst; Fundstellen nach Partie und Zug.
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

/// Die `n` häufigsten Einträge, bei Gleichstand nach Name.
fn most_common<N: Ord + Copy>(counts: HashMap<String, N>, n: usize) -> Vec<(String, N)> {
    let mut list: Vec<(String, N)> = counts.into_iter().collect();
    list.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    list.truncate(n);
    list
}

/// Siege, Niederlagen, Anfangen/Ziehen und abgeworfene Karten.
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

/// Gegnerkarten (Partien, in denen sie vorkamen) und Schaden je Quelle.
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

/// Aufgegeben, obwohl noch genug Leben übrig war.
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
                    "games_report.aufgegeben_leben_gegner",
                    life_me = life_me,
                    life_opp = life_opp
                ),
            },
        );
    }
}

/// Starthand: gut mit [`GOOD_KEEP_LANDS`] Ländern, sonst riskant.
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
                    "games_report.karten_laendern_behalten",
                    hand_n = hand.len(),
                    lands = lands
                ),
            },
        );
    }
}

/// Alle Züge einer vollständig mitgeschriebenen Partie.
fn check_turns(r: &mut Report, found: &mut Findings, g: &GameRecord, db: &CardDb, nr: usize) {
    let mut prev_life = START_LIFE;
    for t in &g.turns {
        let lost = prev_life - t.life_me;
        prev_life = t.life_me;
        if t.mine {
            r.my_turns += 1;
            check_my_turn(found, t, db, nr);
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
                    "games_report.lag_eigene_zuege_bezahlbar",
                    name = h.card.display_name(),
                    turns = h.turns
                ),
            },
        );
    }
}

/// Gegnerzug: viel Schaden bei wenig Leben genommen, ohne zu blocken.
fn check_opp_turn(found: &mut Findings, t: &TurnStat, lost: i32, nr: usize) {
    let before = t.life_me + lost;
    if !t.blocked && t.my_untapped_creatures > 0 && lost >= 3 && before <= 10 {
        found.push(
            Mistake::NoBlock,
            Finding {
                game: nr,
                turn: t.turn,
                text: tr!(
                    "games_report.schaden_bei_leben_genommen",
                    lost = lost,
                    before = before,
                    my_untapped_creatures = t.my_untapped_creatures
                ),
            },
        );
    }
}

/// Eigener Zug: Land, Angriff, Mana und Feld.
fn check_my_turn(found: &mut Findings, t: &TurnStat, db: &CardDb, nr: usize) {
    let hand: Vec<&Card> = t.hand.iter().filter_map(|grp| card(db, *grp)).collect();
    let find = |text: String| Finding {
        game: nr,
        turn: t.turn,
        text,
    };
    if !t.land_played
        && let Some(land) = hand.iter().find(|c| c.is_land())
    {
        found.push(
            Mistake::MissedLand,
            find(tr!("games_report.lag_hand", name = land.display_name())),
        );
    }
    if !t.safe_idle.is_empty() {
        found.push(
            Mistake::SkippedAttack,
            find(tr!(
                "games_report.haette_n_gefahrlos_angreifen",
                v = names(db, &t.safe_idle)
            )),
        );
    }
    if let Some(text) = key_moment(t, db) {
        found.push(Mistake::BetterTurn, find(text));
    } else if t.lands.is_empty() && t.untapped_lands >= 1 {
        // Ältere Aufzeichnung ohne Länder: einfache Prüfung ohne Farben.
        let castable = hand
            .iter()
            .filter(|c| !c.is_land() && c.cmc > 0 && c.cmc <= t.untapped_lands && !c.is_instant())
            .max_by_key(|c| c.cmc);
        if let Some(c) = castable {
            found.push(
                Mistake::UnusedMana,
                find(tr!(
                    "games_report.laender_frei_hand",
                    untapped_lands = t.untapped_lands,
                    name = c.display_name(),
                    mana_cost = c.mana_cost
                )),
            );
        }
    }
    if (3..=6).contains(&t.turn) && t.creatures_me == 0 {
        found.push(
            Mistake::EmptyBoard,
            find(tr!("games_report.keine_eigene_kreatur_feld")),
        );
    }
}
