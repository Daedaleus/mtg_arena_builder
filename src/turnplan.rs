//! Zugvorschlag für das Begleitfenster: welches Land legen und welche Karten
//! aus der Hand ausspielen, damit das Mana gut genutzt wird.
//!
//! Feste Regeln statt Spiel-KI: Manakosten und Farben werden exakt geprüft,
//! die Wichtigkeit einer Karte nur grob über ihre Rolle (Entfernung,
//! Kartenziehen, Kreatur …) geschätzt. Mana aus anderen Quellen als Ländern
//! bleibt unberücksichtigt.

use crate::carddb::{
    Card, CardDb, GrpId, ROLE_COUNTER, ROLE_DRAW, ROLE_REMOVAL, ROLE_WIPE, keyword,
};

/// Schlüsselwort-Fähigkeit „Eile“ in der Arena-Datenbank.
/// Höchstens so viele Handkarten werden kombiniert (2^n Möglichkeiten).
const MAX_CANDIDATES: usize = 12;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    /// Land, das gelegt werden soll.
    pub land: Option<GrpId>,
    /// Das empfohlene Land kommt getappt ins Spiel.
    pub land_tapped: bool,
    /// Auszuspielende Karten in sinnvoller Reihenfolge.
    pub spells: Vec<GrpId>,
    pub mana_used: u32,
    pub mana_total: u32,
    /// Spontane Karten, die man besser für den Zug des Gegners aufhebt und
    /// die mit dem übrigen Mana noch bezahlbar sind.
    pub hold: Vec<GrpId>,
    /// Stärkste gegnerische Kreatur – Ziel für Entfernung.
    pub target: Option<GrpId>,
}

/// Farbmaske, die ein Land liefert (unbekannte Länder gelten als beliebig).
fn land_colors(db: &CardDb, grp: GrpId) -> u8 {
    db.cards.get(&grp).map(|c| c.colors).unwrap_or(0x1F)
}

/// Manaquellen (Farbmasken) aus ungetappten Ländern und weiteren Permanents
/// mit Manafähigkeit (z. B. Llanowar Elves); Permanents ohne sie zählen nicht.
pub fn sources(db: &CardDb, lands: &[GrpId], permanents: &[GrpId]) -> Vec<u8> {
    lands
        .iter()
        .map(|g| land_colors(db, *g))
        .chain(
            permanents
                .iter()
                .filter_map(|g| db.cards.get(g).and_then(|c| c.mana)),
        )
        .collect()
}

/// Lässt sich die Summe der Kosten mit diesen Manaquellen (Farbmasken) bezahlen?
pub fn can_pay(cards: &[&Card], sources: &[u8]) -> bool {
    let mut generic = 0u32;
    let mut pips: Vec<u8> = Vec::new();
    for c in cards {
        let (g, colored) = c.pips();
        generic += g;
        for (i, n) in colored.iter().enumerate() {
            pips.extend(std::iter::repeat_n(1u8 << i, *n as usize));
        }
    }
    if (generic as usize + pips.len()) > sources.len() {
        return false;
    }
    // Farbige Symbole per bipartitem Matching auf Länder verteilen.
    let mut owner: Vec<Option<usize>> = vec![None; sources.len()];
    fn assign(
        pip: usize,
        pips: &[u8],
        sources: &[u8],
        owner: &mut [Option<usize>],
        seen: &mut [bool],
    ) -> bool {
        for s in 0..sources.len() {
            if sources[s] & pips[pip] != 0 && !seen[s] {
                seen[s] = true;
                if owner[s].is_none_or(|o| assign(o, pips, sources, owner, seen)) {
                    owner[s] = Some(pip);
                    return true;
                }
            }
        }
        false
    }
    for p in 0..pips.len() {
        let mut seen = vec![false; sources.len()];
        if !assign(p, &pips, sources, &mut owner, &mut seen) {
            return false;
        }
    }
    true
}

/// Grobe Wichtigkeit einer Karte für diesen Zug.
fn value(c: &Card, threat: i32) -> i32 {
    let mut v = c.cmc as i32 * 10;
    if c.has_role(ROLE_REMOVAL) && !c.is_creature() {
        v += 20 + threat * 2;
    }
    if c.is_creature() {
        v += 2;
    }
    if c.has_role(ROLE_DRAW) {
        v += 1;
    }
    if c.is_instant() && !c.has_role(ROLE_REMOVAL) {
        // Spontane Tricks lieber offen halten.
        v -= 5;
    }
    v
}

fn order_key(c: &Card) -> u8 {
    if c.has_role(ROLE_REMOVAL) && !c.is_creature() {
        0
    } else if c.has_role(ROLE_DRAW) {
        1
    } else if !c.is_creature() {
        2
    } else {
        3
    }
}

/// Beste Auswahl aus `cands` für diese Manaquellen: (Wert, gewählte Karten).
fn best_subset<'a>(cands: &[&'a Card], sources: &[u8], threat: i32) -> (i32, Vec<&'a Card>) {
    let mut best: (i32, Vec<&Card>) = (0, Vec::new());
    for mask in 1u32..(1 << cands.len()) {
        let chosen: Vec<&Card> = cands
            .iter()
            .enumerate()
            .filter(|(i, _)| mask & (1 << i) != 0)
            .map(|(_, c)| *c)
            .collect();
        let cost: u32 = chosen.iter().map(|c| c.cmc).sum();
        if cost as usize > sources.len() {
            continue;
        }
        let score: i32 = chosen.iter().map(|c| value(c, threat)).sum();
        if score > best.0 && can_pay(&chosen, sources) {
            best = (score, chosen);
        }
    }
    best
}

/// Berechnet den Vorschlag für den eigenen Zug.
pub fn plan(
    db: &CardDb,
    hand: &[GrpId],
    untapped_lands: &[GrpId],
    mana_permanents: &[GrpId],
    land_played: bool,
    opp_creatures: &[(GrpId, i32)],
    my_creatures: u32,
) -> Plan {
    let cards: Vec<&Card> = hand.iter().filter_map(|g| db.cards.get(g)).collect();
    let target = opp_creatures
        .iter()
        .max_by_key(|(_, p)| *p)
        .map(|(g, _)| *g);
    let threat = opp_creatures.iter().map(|(_, p)| *p).max().unwrap_or(0);

    let mut cands: Vec<&Card> = Vec::new();
    let mut held: Vec<&Card> = Vec::new();
    for c in cards.iter().filter(|c| !c.is_land() && c.cmc > 0) {
        let removal = c.has_role(ROLE_REMOVAL) && !c.is_creature();
        if c.has_role(ROLE_COUNTER) && !removal {
            held.push(c);
        } else if c.has_role(ROLE_WIPE) && opp_creatures.len() as u32 <= my_creatures {
            // Flächenzauber nur, wenn der Gegner mehr Kreaturen hat.
        } else if removal && opp_creatures.is_empty() {
            if c.is_instant() {
                held.push(c);
            }
        } else {
            cands.push(c);
        }
    }
    // Teure Karten zuerst, damit bei zu vielen Handkarten die wichtigen bleiben.
    cands.sort_by_key(|c| std::cmp::Reverse(c.cmc));
    cands.truncate(MAX_CANDIDATES);

    let base = sources(db, untapped_lands, mana_permanents);
    let mut land_options: Vec<Option<&Card>> = vec![None];
    if !land_played {
        let mut lands: Vec<&Card> = cards.iter().filter(|c| c.is_land()).copied().collect();
        lands.sort_by_key(|c| c.grp_id);
        lands.dedup_by_key(|c| c.grp_id);
        if !lands.is_empty() {
            land_options = lands.into_iter().map(Some).collect();
        }
    }
    // Farben, die die Hand braucht, aber noch keine Quelle liefert.
    let base_mask = base.iter().fold(0u8, |m, c| m | c);
    let wanted = cards
        .iter()
        .filter(|c| !c.is_land())
        .fold(0u8, |m, c| m | c.colors)
        & !base_mask;

    let mut best: Option<((i32, bool, u32), Plan)> = None;
    for land in land_options {
        let mut sources = base.clone();
        let tapped = land.is_some_and(|l| l.enters_tapped);
        if let Some(l) = land
            && !l.enters_tapped
        {
            sources.push(l.colors);
        }
        let (score, mut chosen) = best_subset(&cands, &sources, threat);
        chosen.sort_by_key(|c| (order_key(c), std::cmp::Reverse(c.cmc)));
        let used: u32 = chosen.iter().map(|c| c.cmc).sum();
        let left = sources.len() as u32 - used;
        let new_colors = land.map_or(0, |l| (l.colors & wanted).count_ones());
        // Gleicher Nutzen: lieber das getappte Land jetzt legen, dann neue Farben.
        let key = (score, tapped, new_colors);
        let p = Plan {
            land: land.map(|l| l.grp_id),
            land_tapped: tapped,
            spells: chosen.iter().map(|c| c.grp_id).collect(),
            mana_used: used,
            mana_total: sources.len() as u32,
            hold: held
                .iter()
                .filter(|c| c.cmc <= left)
                .map(|c| c.grp_id)
                .collect(),
            target: if chosen
                .iter()
                .any(|c| c.has_role(ROLE_REMOVAL) && !c.is_creature())
            {
                target
            } else {
                None
            },
        };
        if best.as_ref().is_none_or(|(k, _)| key > *k) {
            best = Some((key, p));
        }
    }
    best.map(|(_, p)| p).unwrap_or_default()
}

/// Kreaturen ohne Eile im Plan, die man auch nach dem Angriff spielen kann.
pub fn creatures_after_combat(db: &CardDb, plan: &Plan) -> bool {
    let cards: Vec<&Card> = plan.spells.iter().filter_map(|g| db.cards.get(g)).collect();
    !cards.is_empty()
        && cards
            .iter()
            .all(|c| c.is_creature() && !c.has_ability(keyword::HASTE))
}

/// Spontane Entfernung, die jetzt (im Zug des Gegners) bezahlbar ist.
pub fn instant_removal(
    db: &CardDb,
    hand: &[GrpId],
    untapped_lands: &[GrpId],
    mana_permanents: &[GrpId],
) -> Vec<GrpId> {
    let sources = sources(db, untapped_lands, mana_permanents);
    hand.iter()
        .filter_map(|g| db.cards.get(g))
        .filter(|c| c.is_instant() && c.has_role(ROLE_REMOVAL) && can_pay(&[c], &sources))
        .map(|c| c.grp_id)
        .collect()
}

#[cfg(test)]
#[path = "tests/turnplan.rs"]
mod tests;
