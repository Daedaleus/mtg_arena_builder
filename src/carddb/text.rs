//! Properties from card texts: roles, burn damage, mana abilities, tapped lands.

use super::*;

/// Damage an ability text can deal to a player (0 = none).
/// Catches e.g. "deals 3 damage to any target" or "… to each opponent".
pub(super) fn burn_of(text: &str) -> u32 {
    let t = strip_tags(text).to_lowercase();
    for target in [
        "any target",
        "target player",
        "target opponent",
        "each opponent",
        "target player or planeswalker",
    ] {
        let needle = format!(" damage to {target}");
        if let Some(end) = t.find(&needle) {
            let before = &t[..end];
            if let Some(num) = before
                .rsplit("deals ")
                .next()
                .and_then(|n| n.trim().parse::<u32>().ok())
            {
                return num;
            }
        }
    }
    0
}

/// Derives rough roles from an (English) ability text.
pub(super) fn roles_of(text: &str) -> u8 {
    let t = strip_tags(text).to_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|n| t.contains(n));
    let mut roles = 0;
    let exile =
        t.contains("exile target") && !has(&["card from", "exile target card", "you control"]);
    if exile
        || (t.contains("destroy target") && !t.contains("destroy target land"))
        || has(&[
            "destroy all",
            "exile all creatures",
            "damage to any target",
            "damage to target creature",
            "damage to target permanent",
            "damage to up to",
            "damage to each creature",
            "damage divided",
            "target creature gets -",
            "creatures get -",
            "fights target",
            "fights up to",
            "fights another target",
            "damage equal to its power to target",
            "sacrifices a creature",
        ])
    {
        roles |= ROLE_REMOVAL;
    }
    if has(&[
        "destroy all",
        "exile all creatures",
        "damage to each creature",
        "creatures get -",
    ]) {
        roles |= ROLE_WIPE;
    }
    if t.contains("counter target") {
        roles |= ROLE_COUNTER;
    }
    if has(&[
        "draw a card",
        "draw two cards",
        "draw three cards",
        "draws two cards",
        "draw cards",
    ]) {
        roles |= ROLE_DRAW;
    }
    roles
}

pub(super) fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

/// Properties gained from the ability texts per ability id.
#[derive(Default)]
pub(super) struct AbilityInfo {
    roles: HashMap<u32, u8>,
    burn: HashMap<u32, u32>,
    tapped: HashSet<u32>,
    /// Activated abilities that cost tapping.
    tap_cost: HashSet<u32>,
    /// Mana abilities: colors produced (0 = colorless).
    mana: HashMap<u32, u8>,
}

impl AbilityInfo {
    pub(super) fn add(&mut self, id: u32, text: &str) {
        let roles = roles_of(text);
        if roles != 0 {
            self.roles.insert(id, roles);
        }
        if enters_tapped_of(text) {
            self.tapped.insert(id);
        }
        let burn = burn_of(text);
        if burn > 0 {
            self.burn.insert(id, burn);
        }
        if let Some(colors) = mana_of(text) {
            self.mana.insert(id, colors);
        }
        if tap_cost_of(text) {
            self.tap_cost.insert(id);
        }
    }
}

/// Derives roles, burn damage, "enters tapped" and mana abilities.
pub(super) fn apply_abilities(card: &mut Card, info: &AbilityInfo) {
    card.roles = card
        .abilities
        .iter()
        .filter_map(|a| info.roles.get(a))
        .fold(0, |m, r| m | r);
    card.enters_tapped = card.abilities.iter().any(|a| info.tapped.contains(a));
    card.tap_ability = card.abilities.iter().any(|a| info.tap_cost.contains(a));
    // Burn damage only counts for instants and sorceries – on creatures it
    // often sits in abilities that don't take effect right away.
    if card
        .types
        .iter()
        .any(|t| *t == TYPE_INSTANT || *t == TYPE_SORCERY)
    {
        card.burn = card
            .abilities
            .iter()
            .filter_map(|a| info.burn.get(a))
            .copied()
            .max()
            .unwrap_or(0);
    }
    // Lands provide their colors via `colors`; here only other mana sources.
    if !card.is_land() {
        card.mana = card
            .abilities
            .iter()
            .filter_map(|a| info.mana.get(a))
            .fold(None, |m: Option<u8>, c| Some(m.unwrap_or(0) | c));
    }
}

/// Does the activated ability cost tapping? The costs come before the
/// colon: "{oT}: …" or "{oT}, Sacrifice …: …".
pub(super) fn tap_cost_of(text: &str) -> bool {
    let t = strip_tags(text).to_lowercase();
    t.split_once(':')
        .is_some_and(|(cost, _)| cost.contains("{ot}"))
}

/// Colors of a mana ability like "{oT}: Add {oG}." (0 = colorless), otherwise `None`.
pub(super) fn mana_of(text: &str) -> Option<u8> {
    let t = strip_tags(text).to_lowercase();
    let (_, add) = t.split_once("{ot}: add ")?;
    if add.contains("any color") {
        return Some(0x1F);
    }
    let colors = ['w', 'u', 'b', 'r', 'g']
        .iter()
        .enumerate()
        .filter(|(_, c)| add.contains(&format!("{{o{c}}}")))
        .fold(0u8, |m, (i, _)| m | (1 << i));
    (colors != 0 || add.contains("{oc}")).then_some(colors)
}

/// Does a land always enter tapped according to its ability text?
pub(super) fn enters_tapped_of(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("enters tapped") && !lower.contains("unless")
}
