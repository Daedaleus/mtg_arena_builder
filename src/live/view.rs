//! Berechnet die Anzeige des Begleitfensters aus dem Spielstand.

use super::*;

pub(super) fn phase_label(phase: &str) -> &'static str {
    match phase {
        "Phase_Beginning" => "Zugbeginn",
        "Phase_Main1" => "1. Hauptphase",
        "Phase_Combat" => "Kampf",
        "Phase_Main2" => "2. Hauptphase",
        "Phase_Ending" => "Zugende",
        _ => "",
    }
}

pub(super) fn names(db: &CardDb, grps: &[u32], limit: usize) -> String {
    let mut list: Vec<String> = Vec::new();
    for g in grps {
        if let Some(c) = db.cards.get(g)
            && !list.contains(&c.name)
        {
            list.push(c.name.clone());
        }
    }
    let more = list.len().saturating_sub(limit);
    list.truncate(limit);
    let mut text = list.join(", ");
    if more > 0 {
        text.push_str(&format!(" (+{more})"));
    }
    text
}

/// Erkennt das gegnerische Deck aus den bisher gesehenen Karten.
pub(super) fn guess_opponent<'a>(
    cards: &[u32],
    db: &CardDb,
    meta: &'a [ArchShare],
) -> (String, Option<&'a ArchShare>) {
    let seen = crate::opponents::seen_keys(cards, db);
    match crate::opponents::match_archetype(&seen, meta) {
        Some(a) => (format!("vermutlich {}", a.name), Some(a)),
        None => {
            let ids: Vec<u32> = cards.to_vec();
            let colors = crate::games::deck_colors(&ids, db);
            let label = if colors.is_empty() {
                "noch unbekannt".into()
            } else {
                crate::carddb::color::label(&colors)
            };
            (label, None)
        }
    }
}

/// Zeigt eine beendete Partie noch so lange an.
const SHOW_LAST_GAME: Duration = Duration::from_secs(180);

pub fn build_view(
    live: Option<&LiveState>,
    last: Option<&(GameRecord, Instant)>,
    db: &CardDb,
    meta: &[ArchShare],
) -> CompanionView {
    let Some(ls) = live else {
        return idle_view(
            last.filter(|(_, at)| at.elapsed() < SHOW_LAST_GAME)
                .map(|(g, _)| g),
        );
    };
    let mut v = CompanionView {
        life: Some((ls.life_me, ls.life_opp)),
        board: format!(
            "Länder: {} ({} frei) · Kreaturen: du {}, Gegner {}",
            ls.lands_me, ls.lands_untapped, ls.creatures_me, ls.creatures_opp
        ),
        ..Default::default()
    };
    let (opponent, arch) = guess_opponent(&ls.opp_cards, db, meta);
    let hand: Vec<&Card> = ls.hand.iter().filter_map(|g| db.cards.get(g)).collect();
    if ls.mulligan {
        v.status = "Starthand".into();
        mulligan_hint(&mut v, &hand);
    } else {
        v.status = turn_status(ls);
        let lethal = lethal_hint(&mut v, ls, db, &hand);
        if !lethal && ls.life_opp <= 5 {
            v.hints.push((
                Level::Good,
                format!(
                    "Gegner hat nur noch {} Leben – nicht aufgeben!",
                    ls.life_opp
                ),
            ));
        }
        if ls.my_turn {
            my_turn_hints(&mut v, ls, db);
        } else if ls.opp_attackers > 0 {
            defense_hints(&mut v, ls, db);
        }
        trick_warning(&mut v, ls, db, arch);
        if v.hints.is_empty() {
            v.hints
                .push((Level::Info, "Gerade nichts zu beachten.".into()));
        }
    }
    v.opponent = opponent;
    v.opp_cards = opp_card_names(ls, db);
    v
}

/// Keine laufende Partie: Ergebnis der letzten oder Warten.
fn idle_view(last: Option<&GameRecord>) -> CompanionView {
    let mut v = CompanionView::default();
    match last {
        Some(g) => {
            v.status = match g.won {
                Some(true) => "Partie vorbei – gewonnen!".into(),
                Some(false) => "Partie vorbei – verloren.".into(),
                None => "Partie vorbei.".into(),
            };
            v.hints.push((
                Level::Info,
                "Die Auswertung steht gleich unter „Meine Partien“.".into(),
            ));
        }
        None => {
            v.status = "Warte auf eine Partie …".into();
            v.hints.push((
                Level::Info,
                "Tipp: Arena im Fenstermodus oder randlosen Vollbild spielen, damit dieses Fenster sichtbar bleibt."
                    .into(),
            ));
        }
    }
    v
}

/// Starthand behalten oder Mulligan?
fn mulligan_hint(v: &mut CompanionView, hand: &[&Card]) {
    if hand.len() < 6 {
        return;
    }
    let n = hand.iter().filter(|c| c.is_land()).count();
    if crate::games::GOOD_KEEP_LANDS.contains(&n) {
        v.hints.push((
            Level::Good,
            format!("{n} Länder bei {} Karten – behalten.", hand.len()),
        ));
    } else {
        v.hints.push((
            Level::Warn,
            format!(
                "{n} Länder bei {} Karten – Mulligan empfohlen (gut sind 2–4).",
                hand.len()
            ),
        ));
    }
}

/// „Zug 5 · dein Zug · Kampf“
fn turn_status(ls: &LiveState) -> String {
    let whose = if ls.my_turn {
        "dein Zug"
    } else {
        "Gegner am Zug"
    };
    match phase_label(&ls.phase) {
        "" => format!("Zug {} · {whose}", ls.turn),
        phase => format!("Zug {} · {whose} · {phase}", ls.turn),
    }
}

/// Siegchancen: Kann ich den Gegner jetzt besiegen? Liefert `true`, wenn ein
/// tödlicher Angriff möglich ist.
fn lethal_hint(v: &mut CompanionView, ls: &LiveState, db: &CardDb, hand: &[&Card]) -> bool {
    if !ls.my_turn || ls.phase == "Phase_Ending" || ls.life_opp <= 0 {
        return false;
    }
    let extra = crate::turnplan::sources(db, &[], &ls.mana_permanents).len() as u32;
    let land_to_play = !ls.land_played && hand.iter().any(|c| c.is_land());
    let mana = ls.lands_untapped + extra + land_to_play as u32;
    let combat = unblocked_damage(&ls.attackers, &ls.blockers);
    let (burn, burn_cards) = best_burn(hand, mana);
    let total = combat + burn as i32;
    let burn_text = if burn > 0 {
        format!(" + Brand {burn} ({})", burn_cards.join(", "))
    } else {
        String::new()
    };
    if total >= ls.life_opp {
        v.hints.push((
            Level::Good,
            format!(
                "Tödlich möglich! Angriff mit allem {combat}{burn_text} reicht für die {} Leben des Gegners – sofern er keinen Trick hat.",
                ls.life_opp
            ),
        ));
        return true;
    }
    if total > 0 && ls.life_opp <= total + 4 {
        v.hints.push((
            Level::Info,
            format!(
                "Gegner fast besiegt: {} Leben, du schaffst jetzt mindestens {total} (Angriff {combat}{burn_text}).",
                ls.life_opp
            ),
        ));
    }
    false
}

/// Eigener Zug: gefahrlose Angreifer und in den Hauptphasen der Zugvorschlag.
fn my_turn_hints(v: &mut CompanionView, ls: &LiveState, db: &CardDb) {
    if !ls.safe_attackers.is_empty() {
        v.hints.push((
            Level::Good,
            format!("Gefahrlos angreifen: {}", names(db, &ls.safe_attackers, 4)),
        ));
    }
    if matches!(
        ls.phase.as_str(),
        "Phase_Beginning" | "Phase_Main1" | "Phase_Main2" | ""
    ) {
        turn_hints(v, ls, db);
    }
}

/// Der Gegner greift an: spontane Entfernung und Blockvorschlag.
fn defense_hints(v: &mut CompanionView, ls: &LiveState, db: &CardDb) {
    let removal =
        crate::turnplan::instant_removal(db, &ls.hand, &ls.untapped_lands, &ls.mana_permanents);
    if !removal.is_empty() {
        v.hints.push((
            Level::Good,
            format!(
                "Jetzt spontan einsetzbar: {} – gegen einen Angreifer.",
                names(db, &removal, 2)
            ),
        ));
    }
    block_hints(v, ls, db);
}

/// Offenes Mana des Gegners vor einem Kampf: typische Tricks seines Decks.
fn trick_warning(v: &mut CompanionView, ls: &LiveState, db: &CardDb, arch: Option<&ArchShare>) {
    let combat_ahead = if ls.my_turn {
        matches!(ls.phase.as_str(), "Phase_Main1" | "Phase_Combat")
    } else {
        ls.opp_attackers > 0
    };
    if !combat_ahead || ls.opp_untapped_lands == 0 {
        return;
    }
    let tricks = crate::combat::likely_tricks(db, arch, &ls.opp_cards, ls.opp_untapped_lands);
    if tricks.is_empty() {
        return;
    }
    let list: Vec<String> = tricks
        .iter()
        .take(3)
        .map(|(n, share)| match share {
            Some(s) => format!("{n} ({:.0} %)", s * 100.0),
            None => format!("{n} (schon gesehen)"),
        })
        .collect();
    v.hints.push((
        Level::Warn,
        format!(
            "Vorsicht, Gegner hat {} Mana offen – möglich: {}",
            ls.opp_untapped_lands,
            list.join(", ")
        ),
    ));
}

/// Gesehene Nichtland-Karten des Gegners, alphabetisch.
fn opp_card_names(ls: &LiveState, db: &CardDb) -> Vec<String> {
    let mut seen: Vec<String> = ls
        .opp_cards
        .iter()
        .filter_map(|g| db.cards.get(g))
        .filter(|c| !c.is_land())
        .map(|c| c.name.clone())
        .collect();
    seen.sort();
    seen.dedup();
    seen
}
