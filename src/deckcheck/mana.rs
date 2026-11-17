//! Länder-Check: zu wenige oder zu viele Länder in den eigenen Partien.

use super::*;

pub(super) fn mana_check(group: &DeckGroup, games: &[&GameRecord], db: &CardDb) -> ManaCheck {
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
