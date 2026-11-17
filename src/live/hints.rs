//! Zugvorschlag und Blockvorschlag als Hinweistexte.

use super::*;

/// Blockvorschlag, während der Gegner angreift.
pub(super) fn block_hints(v: &mut CompanionView, ls: &LiveState, db: &CardDb) {
    use crate::combat::BlockKind;
    let plan = crate::combat::plan_blocks(&ls.opp_attacking, &ls.my_blockers, ls.life_me);
    let mut parts: Vec<String> = Vec::new();
    for b in &plan.blocks {
        let how = match b.kind {
            BlockKind::Win => "gewinnt",
            BlockKind::Safe => "hält auf, überlebt",
            BlockKind::Trade => "Tausch",
            BlockKind::Chump => "opfern, sonst tödlich",
        };
        let who: Vec<String> = b.blockers.iter().map(|g| card_name(db, *g)).collect();
        parts.push(format!(
            "{} {} {} ({how})",
            who.join(" und "),
            if who.len() > 1 { "blocken" } else { "blockt" },
            card_name(db, b.attacker)
        ));
    }
    if plan.lethal {
        v.hints.push((
            Level::Warn,
            format!(
                "Achtung: Selbst mit allen Blocks kommen {} Schaden durch, du hast {} Leben – nur ein Trick rettet dich.",
                plan.damage, ls.life_me
            ),
        ));
    }
    if parts.is_empty() {
        if !plan.lethal && plan.damage > 0 {
            v.hints.push((
                Level::Info,
                format!(
                    "Kein lohnender Block – nicht blocken, du nimmst {} Schaden.",
                    plan.damage
                ),
            ));
        }
        return;
    }
    let chump = plan.blocks.iter().any(|b| b.kind == BlockKind::Chump);
    v.hints.push((
        if chump { Level::Warn } else { Level::Good },
        format!(
            "Blocken: {}. Du nimmst {} statt {} Schaden.",
            parts.join("; "),
            plan.damage,
            plan.damage_unblocked
        ),
    ));
}

pub(super) fn card_name(db: &CardDb, grp: u32) -> String {
    db.cards
        .get(&grp)
        .map(|c| c.name.clone())
        .unwrap_or_default()
}

/// Zugvorschlag: Land, Zauber und was man zurückhalten sollte.
pub(super) fn turn_hints(v: &mut CompanionView, ls: &LiveState, db: &CardDb) {
    let plan = crate::turnplan::plan(
        db,
        &ls.hand,
        &ls.untapped_lands,
        &ls.mana_permanents,
        ls.land_played,
        &ls.opp_creatures,
        ls.creatures_me,
    );
    let mut steps: Vec<String> = Vec::new();
    if let Some(land) = plan.land {
        steps.push(format!(
            "{} legen{}",
            card_name(db, land),
            if plan.land_tapped {
                " (kommt getappt)"
            } else {
                ""
            }
        ));
    }
    for (i, g) in plan.spells.iter().enumerate() {
        let mut text = card_name(db, *g);
        if i == 0
            && let Some(t) = plan.target
            && db
                .cards
                .get(g)
                .is_some_and(|c| c.has_role(crate::carddb::ROLE_REMOVAL) && !c.is_creature())
        {
            text.push_str(&format!(" auf {}", card_name(db, t)));
        }
        steps.push(text);
    }
    if steps.is_empty() {
        return;
    }
    let mana = if plan.spells.is_empty() {
        if plan.mana_total > 0 {
            " – nichts Sinnvolles bezahlbar".to_string()
        } else {
            String::new()
        }
    } else {
        format!(" ({} von {} Mana)", plan.mana_used, plan.mana_total)
    };
    let late = ls.phase == "Phase_Main2" && plan.land.is_some();
    v.hints.push((
        if late { Level::Warn } else { Level::Good },
        format!("Vorschlag: {}{mana}", steps.join(", dann ")),
    ));
    if !plan.hold.is_empty() {
        v.hints.push((
            Level::Info,
            format!(
                "Danach Mana offen lassen für {} im Zug des Gegners.",
                names(db, &plan.hold, 2)
            ),
        ));
    }
    if ls.phase == "Phase_Main1"
        && !ls.safe_attackers.is_empty()
        && crate::turnplan::creatures_after_combat(db, &plan)
    {
        v.hints.push((
            Level::Info,
            "Tipp: Erst angreifen, die Kreaturen danach in der 2. Hauptphase spielen – so verrätst du dem Gegner weniger.".into(),
        ));
    }
}
