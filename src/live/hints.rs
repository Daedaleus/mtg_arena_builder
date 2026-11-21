//! Turn suggestion and block suggestion as hint texts.

use super::*;

/// Block suggestion while the opponent attacks.
pub(super) fn block_hints(v: &mut CompanionView, ls: &LiveState, db: &CardDb) {
    use crate::combat::BlockKind;
    let plan = crate::combat::plan_blocks(&ls.opp_attacking, &ls.my_blockers, ls.life_me);
    let mut parts: Vec<String> = Vec::new();
    for b in &plan.blocks {
        let how = match b.kind {
            BlockKind::Win => tr!("live_hints.wins"),
            BlockKind::Safe => tr!("live_hints.holds_survives"),
            BlockKind::Trade => tr!("live_hints.trade"),
            BlockKind::Chump => tr!("live_hints.chump_otherwise_lethal"),
        };
        let who: Vec<String> = b
            .blockers
            .iter()
            .map(|g| card_name(db, GrpId(*g)))
            .collect();
        let blockers = who.join(&tr!("common.and"));
        let attacker = card_name(db, GrpId(b.attacker));
        parts.push(if who.len() > 1 {
            tr!(
                "live_hints.block",
                blockers = blockers,
                attacker = attacker,
                how = how
            )
        } else {
            tr!(
                "live_hints.blocks",
                blockers = blockers,
                attacker = attacker,
                how = how
            )
        });
    }
    if plan.lethal {
        v.hints.push((
            Level::Warn,
            tr!(
                "live_hints.careful_even_blocks_damage",
                damage = plan.damage,
                life_me = ls.life_me
            ),
        ));
    }
    if parts.is_empty() {
        if !plan.lethal && plan.damage > 0 {
            v.hints.push((
                Level::Info,
                tr!("live_hints.no_worthwhile_block_dont", damage = plan.damage),
            ));
        }
        return;
    }
    let chump = plan.blocks.iter().any(|b| b.kind == BlockKind::Chump);
    v.hints.push((
        if chump { Level::Warn } else { Level::Good },
        tr!(
            "live_hints.block_take_instead_damage",
            v = parts.join("; "),
            damage = plan.damage,
            damage_unblocked = plan.damage_unblocked
        ),
    ));
}

pub(super) fn card_name(db: &CardDb, grp: GrpId) -> String {
    db.cards
        .get(&grp)
        .map(|c| c.display_name().to_string())
        .unwrap_or_default()
}

/// Turn suggestion: land, spells and what to hold back.
pub(super) fn turn_hints(v: &mut CompanionView, ls: &LiveState, db: &CardDb) {
    let hand = crate::turnplan::without_legend_duplicates(db, &ls.hand, &ls.board);
    let plan = crate::turnplan::plan(
        db,
        &hand,
        &ls.untapped_lands,
        &ls.mana_permanents,
        ls.land_played,
        &ls.opp_creatures,
        ls.creatures_me,
    );
    let mut steps: Vec<String> = Vec::new();
    if let Some(land) = plan.land {
        steps.push(tr!(
            "live_hints.play",
            v = card_name(db, land),
            v2 = if plan.land_tapped {
                tr!("live_hints.enters_tapped")
            } else {
                String::new()
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
            text.push_str(&tr!("live_hints.on", v = card_name(db, t)));
        }
        steps.push(text);
    }
    if steps.is_empty() {
        return;
    }
    let mana = if plan.spells.is_empty() {
        if plan.mana_total > 0 {
            tr!("live_hints.nothing_useful_castable")
        } else {
            String::new()
        }
    } else {
        tr!(
            "live_hints.mana",
            mana_used = plan.mana_used,
            mana_total = plan.mana_total
        )
    };
    let late = ls.phase == Phase::Main2 && plan.land.is_some();
    v.hints.push((
        if late { Level::Warn } else { Level::Good },
        tr!(
            "live_hints.suggestion",
            v = steps.join(&tr!("common.then")),
            mana = mana
        ),
    ));
    if !plan.hold.is_empty() {
        v.hints.push((
            Level::Info,
            tr!(
                "live_hints.keep_mana_open_during",
                v = names(db, &plan.hold, 2)
            ),
        ));
    }
    if ls.phase == Phase::Main1
        && !worth_attacking(ls, db).is_empty()
        && crate::turnplan::creatures_after_combat(db, &plan)
    {
        v.hints
            .push((Level::Info, tr!("live_hints.tip_attack_first_cast")));
    }
}
