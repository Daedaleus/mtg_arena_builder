//! Recommendations from the key figures of the sample hands.

use super::*;

/// Recommendations from the key figures.
pub(super) fn advice(r: &SimReport) -> Vec<String> {
    let pct = |x: f32| (x * 100.0).round() as u32;
    let mut out = Vec::new();
    if r.on_curve[2] < 0.6 {
        out.push(tr!("sim_advice.too_few_lands_only", v = pct(r.on_curve[2])));
    }
    if r.flood >= 0.25 {
        out.push(tr!("sim_advice.too_many_lands_games", v = pct(r.flood)));
    }
    if r.color_screw >= 0.2 {
        let color = r
            .missing_color
            .map(|i| tr!("sim_advice.mostly", names = crate::carddb::color::name(i)))
            .unwrap_or_default();
        out.push(tr!(
            "sim_advice.colors_missing_games_card",
            v = pct(r.color_screw),
            color = color
        ));
    }
    // Too slow only if you don't win the race yourself.
    if r.pressure.low >= 0.4 && r.pressure.ahead < r.pressure.low {
        out.push(tr!(
            "sim_advice.too_slow_against_fast",
            v = pct(r.pressure.low),
            turns = r.turns
        ));
    }
    let used: f32 = r.mana_used[..4].iter().sum();
    let avail: f32 = r.mana_avail[..4].iter().sum();
    if avail > 0.0 && used / avail < 0.55 {
        out.push(tr!(
            "sim_advice.curve_too_high_turns",
            v = pct(used / avail)
        ));
    }
    if r.mulligan >= 0.25 {
        out.push(tr!(
            "sim_advice.many_mulligans_land_count",
            v = pct(r.mulligan)
        ));
    }
    out
}
