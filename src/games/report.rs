//! Auswertung der Partien: typische Fehler, Schlüsselmomente, Gegnerkarten.

use super::*;

pub(super) const TYPE_INSTANT: u8 = 4;

/// Ein auffälliger Moment in einer Partie.
#[derive(Clone, Debug)]
pub struct Finding {
    /// Partie-Nummer (1 = erste im übergebenen Slice).
    pub game: usize,
    /// Zug; 0 = vor dem ersten Zug (Starthand).
    pub turn: u32,
    pub text: String,
}

/// Eine Art von Fehler mit Erklärung und allen Fundstellen.
#[derive(Clone, Debug)]
pub struct Category {
    pub title: &'static str,
    /// Was man stattdessen tun sollte.
    pub advice: &'static str,
    pub items: Vec<Finding>,
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

pub(super) fn card(db: &CardDb, grp: u32) -> Option<&Card> {
    db.cards.get(&grp)
}

pub(super) fn names(db: &CardDb, grps: &[u32]) -> String {
    let mut list: Vec<String> = grps
        .iter()
        .filter_map(|g| card(db, *g))
        .map(|c| c.name.clone())
        .collect();
    list.sort();
    list.dedup();
    list.join(", ")
}

/// Wertet die Partien aus. Partie-Nummern zählen ab 1 in der übergebenen Reihenfolge.
pub fn report(games: &[GameRecord], db: &CardDb) -> Report {
    let mut r = Report {
        games: games.len(),
        ..Default::default()
    };
    let mut keeps = Vec::new();
    let mut missed_lands = Vec::new();
    let mut safe_attacks = Vec::new();
    let mut unused_mana = Vec::new();
    let mut better_turns = Vec::new();
    let mut held = Vec::new();
    let mut no_block = Vec::new();
    let mut empty_board = Vec::new();
    let mut concedes = Vec::new();
    let mut turn_sum = 0;
    let mut opp: HashMap<String, usize> = HashMap::new();
    let mut damage: HashMap<String, u32> = HashMap::new();

    for (gi, g) in games.iter().enumerate() {
        let nr = gi + 1;
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
        turn_sum += g.last_turn();
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

        // Aufgegeben, obwohl noch genug Leben übrig war
        let (life_me, life_opp) = g.final_life();
        if g.won == Some(false) && g.conceded() {
            r.conceded += 1;
            if life_me >= 5 && !g.turns.is_empty() {
                concedes.push(Finding {
                    game: nr,
                    turn: g.last_turn(),
                    text: format!("aufgegeben mit {life_me} Leben (Gegner {life_opp})"),
                });
            }
        }

        // Starthand
        let hand: Vec<&Card> = g
            .opening_hand
            .iter()
            .filter_map(|grp| card(db, *grp))
            .collect();
        if hand.len() >= 6 {
            let lands = hand.iter().filter(|c| c.is_land()).count();
            let good = (2..=4).contains(&lands);
            let slot = if good {
                &mut r.good_keeps
            } else {
                &mut r.bad_keeps
            };
            slot.0 += (g.won == Some(true)) as usize;
            slot.1 += 1;
            if !good {
                keeps.push(Finding {
                    game: nr,
                    turn: 0,
                    text: format!("{} Karten mit {lands} Ländern behalten", hand.len()),
                });
            }
        }

        // Zug-Auswertungen nur für vollständig mitgeschriebene Partien:
        // Teil-Mitschnitte kennen Hand und Feld nicht vollständig.
        if !g.complete {
            continue;
        }
        let mut castable_turns: HashMap<u32, (u32, u32)> = HashMap::new();
        let mut prev_life = 20;
        for t in &g.turns {
            let lost = prev_life - t.life_me;
            prev_life = t.life_me;
            if !t.mine {
                let before = t.life_me + lost;
                if !t.blocked && t.my_untapped_creatures > 0 && lost >= 3 && before <= 10 {
                    no_block.push(Finding {
                        game: nr,
                        turn: t.turn,
                        text: format!(
                            "{lost} Schaden bei {before} Leben genommen, {} eigene Kreatur(en) hätten blocken können",
                            t.my_untapped_creatures
                        ),
                    });
                }
                continue;
            }
            r.my_turns += 1;
            let hand: Vec<&Card> = t.hand.iter().filter_map(|grp| card(db, *grp)).collect();
            let find = |text: String| Finding {
                game: nr,
                turn: t.turn,
                text,
            };
            if !t.land_played
                && let Some(land) = hand.iter().find(|c| c.is_land())
            {
                missed_lands.push(find(format!("{} lag auf der Hand", land.name)));
            }
            if !t.safe_idle.is_empty() {
                safe_attacks.push(find(format!(
                    "{} hätte(n) gefahrlos angreifen können",
                    names(db, &t.safe_idle)
                )));
            }
            if let Some(text) = key_moment(t, db) {
                better_turns.push(find(text));
            } else if t.lands.is_empty() && t.untapped_lands >= 1 {
                // Ältere Aufzeichnung ohne Länder: einfache Prüfung ohne Farben.
                let castable = hand
                    .iter()
                    .filter(|c| {
                        !c.is_land()
                            && c.cmc > 0
                            && c.cmc <= t.untapped_lands
                            && !c.types.contains(&TYPE_INSTANT)
                    })
                    .max_by_key(|c| c.cmc);
                if let Some(c) = castable {
                    unused_mana.push(find(format!(
                        "{} Länder frei, {} ({}) auf der Hand",
                        t.untapped_lands, c.name, c.mana_cost
                    )));
                }
            }
            if (3..=6).contains(&t.turn) && t.creatures_me == 0 {
                empty_board.push(find("keine eigene Kreatur auf dem Feld".into()));
            }
            // Wie lange lag eine bezahlbare Karte auf der Hand? Spontanzauber
            // (z. B. Schutz-Tricks) hält man oft bewusst zurück – die zählen nicht.
            let mut counted = HashSet::new();
            for c in &hand {
                if !c.is_land()
                    && !c.types.contains(&TYPE_INSTANT)
                    && c.cmc > 0
                    && c.cmc <= t.lands_me
                    && counted.insert(c.grp_id)
                {
                    let e = castable_turns.entry(c.grp_id).or_insert((0, t.turn));
                    e.0 += 1;
                    e.1 = t.turn;
                }
            }
        }
        for (grp, (n, last)) in castable_turns {
            if n >= 3
                && let Some(c) = card(db, grp)
            {
                held.push(Finding {
                    game: nr,
                    turn: last,
                    text: format!("{} lag {n} eigene Züge bezahlbar auf der Hand", c.name),
                });
            }
        }
    }
    if !games.is_empty() {
        r.avg_turns = turn_sum as f32 / games.len() as f32;
    }

    let mut cats = vec![
        Category {
            title: "Gefahrlosen Angriff ausgelassen",
            advice: "Wenn keine gegnerische Kreatur deinen Angreifer blocken und dabei töten kann, greif an. \
                     Schlimmstenfalls opfert der Gegner einen Blocker, sonst verliert er Leben. \
                     (Fliegend, Reichweite, Todesberührung und Bedrohlich sind berücksichtigt, Zaubertricks des Gegners nicht.)",
            items: safe_attacks,
        },
        Category {
            title: "Riskante Starthand behalten",
            advice: "Behalte nur Starthände mit 2–4 Ländern. Mit 0–1 Land fehlt dir das Mana, mit 5 oder mehr fehlen die Zauber. \
                     Ein Mulligan auf 6 Karten ist meist besser als so eine Hand.",
            items: keeps,
        },
        Category {
            title: "Land nicht gespielt",
            advice: "Lege jeden Zug ein Land, wenn du eins hast – auch wenn du es gerade nicht brauchst. Später fehlt es sonst.",
            items: missed_lands,
        },
        Category {
            title: "Mana ungenutzt",
            advice: "Nutze dein Mana in jedem Zug. Eine Kreatur auf der Hand macht keinen Schaden – spiel sie, sobald du kannst. \
                     (Die Farben prüft die App nicht; manchmal fehlte also die passende Farbe.)",
            items: unused_mana,
        },
        Category {
            title: "Besserer Zug möglich",
            advice: "Der Zugvorschlag des Begleitfensters hätte in diesen Zügen mehr aus deinem Mana gemacht – \
                     mit Blick auf Farben und gegnerische Kreaturen. Oft lohnen zwei günstige Karten statt einer teuren \
                     oder Entfernung vor der nächsten Kreatur.",
            items: better_turns,
        },
        Category {
            title: "Karte lange nicht gespielt",
            advice: "Diese Karten lagen mehrere Züge bezahlbar auf deiner Hand. Spiel sie früher – \
                     oder tausch sie im Deck gegen etwas, das du wirklich spielst. (Spontanzauber zählen hier nicht mit.)",
            items: held,
        },
        Category {
            title: "Bei wenig Leben nicht geblockt",
            advice: "Bei wenig Leben lohnt es sich zu blocken, auch wenn deine Kreatur dabei stirbt. \
                     Dann sind deine Lebenspunkte wichtiger als die Kreatur.",
            items: no_block,
        },
        Category {
            title: "Keine Kreatur auf dem Feld",
            advice: "In Zug 3–6 lag keine eigene Kreatur auf dem Feld – dann kannst du keinen Druck machen und nicht blocken. \
                     Spiel früh günstige Kreaturen; ein Deck braucht genug davon für die Züge 1–3.",
            items: empty_board,
        },
        Category {
            title: "Früh aufgegeben",
            advice: "Gerade auf niedrigen Rängen machen Gegner oft Fehler, und Partien drehen sich. \
                     Spiel weiter, solange du noch Leben hast.",
            items: concedes,
        },
    ];
    for c in &mut cats {
        c.items.sort_by_key(|f| (f.game, f.turn));
    }
    cats.sort_by_key(|c| std::cmp::Reverse(c.items.len()));
    r.categories = cats;

    let mut opp: Vec<(String, usize)> = opp.into_iter().collect();
    opp.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    opp.truncate(15);
    r.opp_cards = opp;
    let mut damage: Vec<(String, u32)> = damage.into_iter().collect();
    damage.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    damage.truncate(10);
    r.damage_sources = damage;
    r
}

/// Mindestens so viel Mana mehr hätte der Zugvorschlag nutzen müssen,
/// damit ein Zug als „besserer Zug möglich“ gilt.
pub(super) const KEY_MOMENT_MANA: u32 = 2;

/// Vergleicht einen eigenen Zug mit dem Zugvorschlag: Hand zu Zugbeginn
/// (Hand am Ende plus gewirkte Karten) und Länder am Zugende.
pub fn key_moment(t: &TurnStat, db: &CardDb) -> Option<String> {
    if !t.mine || t.lands.is_empty() {
        return None;
    }
    let mut hand = t.hand.clone();
    hand.extend(&t.cast);
    let plan = crate::turnplan::plan(
        db,
        &hand,
        &t.lands,
        &[],
        true,
        &t.opp_creatures,
        t.creatures_at_start,
    );
    let used: u32 = t
        .cast
        .iter()
        .filter_map(|g| card(db, *g))
        .map(|c| c.cmc)
        .sum();
    if plan.spells.is_empty() || plan.mana_used < used + KEY_MOMENT_MANA {
        return None;
    }
    let names: Vec<String> = plan
        .spells
        .iter()
        .filter_map(|g| card(db, *g))
        .map(|c| c.name.clone())
        .collect();
    Some(format!(
        "{used} von {} Mana genutzt – möglich: {} ({} Mana)",
        plan.mana_total,
        names.join(", dann "),
        plan.mana_used
    ))
}
