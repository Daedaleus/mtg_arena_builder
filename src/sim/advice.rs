//! Empfehlungen aus den Kennzahlen der Probehände.

use super::*;

/// Empfehlungen aus den Kennzahlen.
pub(super) fn advice(r: &SimReport) -> Vec<String> {
    let pct = |x: f32| (x * 100.0).round() as u32;
    let mut out = Vec::new();
    if r.on_curve[2] < 0.6 {
        out.push(tr!(
            "sim_advice.wenige_laender_nur_partien",
            v = pct(r.on_curve[2])
        ));
    }
    if r.flood >= 0.25 {
        out.push(tr!(
            "sim_advice.viele_laender_partien_liegen",
            v = pct(r.flood)
        ));
    }
    if r.color_screw >= 0.2 {
        let color = r
            .missing_color
            .map(|i| tr!("sim_advice.meist", names = crate::carddb::color::name(i)))
            .unwrap_or_default();
        out.push(tr!(
            "sim_advice.farben_fehlen_partien_war",
            v = pct(r.color_screw),
            color = color
        ));
    }
    // Zu langsam nur, wenn man das Rennen nicht selbst gewinnt.
    if r.pressure.low >= 0.4 && r.pressure.ahead < r.pressure.low {
        out.push(tr!(
            "sim_advice.gegen_schnelle_decks_langsam",
            v = pct(r.pressure.low),
            turns = r.turns
        ));
    }
    let used: f32 = r.mana_used[..4].iter().sum();
    let avail: f32 = r.mana_avail[..4].iter().sum();
    if avail > 0.0 && used / avail < 0.55 {
        out.push(tr!("sim_advice.kurve_hoch_zuegen_1", v = pct(used / avail)));
    }
    if r.mulligan >= 0.25 {
        out.push(tr!(
            "sim_advice.viele_mulligans_laenderzahl_liegt",
            v = pct(r.mulligan)
        ));
    }
    out
}
