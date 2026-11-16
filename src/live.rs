//! Begleitfenster: liest das Arena-Log während einer Partie mit und berechnet
//! daraus Hinweise („Land legen“, „gefahrlos angreifen“, „blocken!“ …).
//!
//! Es wird nur die Logdatei gelesen, die Arena für Community-Tools schreibt
//! („Detaillierte Logs“). Ins Spiel greift das Fenster nicht ein.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, RwLock};
use std::thread::sleep;
use std::time::{Duration, Instant};

use eframe::egui;

use crate::carddb::{Card, CardDb};
use crate::games::{GameRecord, LiveCreature, LiveState, LogParser};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Level {
    Info,
    Good,
    Warn,
}

/// Was das Begleitfenster gerade anzeigt.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CompanionView {
    pub status: String,
    /// (meine Leben, gegnerische Leben)
    pub life: Option<(i32, i32)>,
    /// Kurze Spielfeld-Übersicht (Länder, Kreaturen).
    pub board: String,
    pub hints: Vec<(Level, String)>,
    pub opponent: String,
    pub opp_cards: Vec<String>,
}

pub use crate::opponents::{ArchShare, arch_shares};

/// Zwischen Live-Thread, Begleitfenster und Hauptfenster geteilter Zustand.
#[derive(Default)]
pub struct Shared {
    pub view: Mutex<CompanionView>,
    pub meta: RwLock<Vec<ArchShare>>,
    /// Das Begleitfenster wurde über sein „X“ geschlossen.
    pub close_requested: AtomicBool,
}

fn phase_label(phase: &str) -> &'static str {
    match phase {
        "Phase_Beginning" => "Zugbeginn",
        "Phase_Main1" => "1. Hauptphase",
        "Phase_Combat" => "Kampf",
        "Phase_Main2" => "2. Hauptphase",
        "Phase_Ending" => "Zugende",
        _ => "",
    }
}

fn names(db: &CardDb, grps: &[u32], limit: usize) -> String {
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
fn guess_opponent<'a>(
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
                crate::opponents::color_label(&colors)
            };
            (label, None)
        }
    }
}

pub fn build_view(
    live: Option<&LiveState>,
    last: Option<&(GameRecord, Instant)>,
    db: &CardDb,
    meta: &[ArchShare],
) -> CompanionView {
    let mut v = CompanionView::default();
    let Some(ls) = live else {
        match last.filter(|(_, at)| at.elapsed() < Duration::from_secs(180)) {
            Some((g, _)) => {
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
        return v;
    };

    v.life = Some((ls.life_me, ls.life_opp));
    let (opponent, arch) = guess_opponent(&ls.opp_cards, db, meta);
    v.board = format!(
        "Länder: {} ({} frei) · Kreaturen: du {}, Gegner {}",
        ls.lands_me, ls.lands_untapped, ls.creatures_me, ls.creatures_opp
    );
    let hand: Vec<_> = ls.hand.iter().filter_map(|g| db.cards.get(g)).collect();
    let lands_in_hand: Vec<_> = hand.iter().filter(|c| c.is_land()).collect();

    if ls.mulligan {
        v.status = "Starthand".into();
        let n = lands_in_hand.len();
        if hand.len() >= 6 {
            if (2..=4).contains(&n) {
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
    } else {
        let phase = phase_label(&ls.phase);
        v.status = format!(
            "Zug {} · {}{}",
            ls.turn,
            if ls.my_turn {
                "dein Zug"
            } else {
                "Gegner am Zug"
            },
            if phase.is_empty() {
                String::new()
            } else {
                format!(" · {phase}")
            }
        );
        // --- Siegchancen: Kann ich den Gegner jetzt besiegen? ---
        let mut lethal = false;
        if ls.my_turn && ls.phase != "Phase_Ending" && ls.life_opp > 0 {
            let mana = ls.lands_untapped + (!ls.land_played && !lands_in_hand.is_empty()) as u32;
            let combat = unblocked_damage(&ls.attackers, &ls.blockers);
            let (burn, burn_cards) = best_burn(&hand, mana);
            let total = combat + burn as i32;
            let burn_text = if burn > 0 {
                format!(" + Brand {burn} ({})", burn_cards.join(", "))
            } else {
                String::new()
            };
            if total >= ls.life_opp {
                lethal = true;
                v.hints.push((
                    Level::Good,
                    format!(
                        "Tödlich möglich! Angriff mit allem {combat}{burn_text} reicht für die {} Leben des Gegners – sofern er keinen Trick hat.",
                        ls.life_opp
                    ),
                ));
            } else if total > 0 && ls.life_opp <= total + 4 {
                v.hints.push((
                    Level::Info,
                    format!(
                        "Gegner fast besiegt: {} Leben, du schaffst jetzt mindestens {total} (Angriff {combat}{burn_text}).",
                        ls.life_opp
                    ),
                ));
            }
        }
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
                turn_hints(&mut v, ls, db);
            }
        } else if ls.opp_attackers > 0 {
            let removal = crate::turnplan::instant_removal(db, &ls.hand, &ls.untapped_lands);
            if !removal.is_empty() {
                v.hints.push((
                    Level::Good,
                    format!(
                        "Jetzt spontan einsetzbar: {} – gegen einen Angreifer.",
                        names(db, &removal, 2)
                    ),
                ));
            }
            block_hints(&mut v, ls, db);
        }
        // Offenes Mana des Gegners: typische Tricks seines Decks.
        let combat_ahead = if ls.my_turn {
            matches!(ls.phase.as_str(), "Phase_Main1" | "Phase_Combat")
        } else {
            ls.opp_attackers > 0
        };
        if combat_ahead && ls.opp_untapped_lands > 0 {
            let tricks =
                crate::combat::likely_tricks(db, arch, &ls.opp_cards, ls.opp_untapped_lands);
            if !tricks.is_empty() {
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
        }
        if v.hints.is_empty() {
            v.hints
                .push((Level::Info, "Gerade nichts zu beachten.".into()));
        }
    }

    v.opponent = opponent;
    let mut seen: Vec<String> = ls
        .opp_cards
        .iter()
        .filter_map(|g| db.cards.get(g))
        .filter(|c| !c.is_land())
        .map(|c| c.name.clone())
        .collect();
    seen.sort();
    seen.dedup();
    v.opp_cards = seen;
    v
}

/// Blockvorschlag, während der Gegner angreift.
fn block_hints(v: &mut CompanionView, ls: &LiveState, db: &CardDb) {
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
        parts.push(format!(
            "{} blockt {} ({how})",
            card_name(db, b.blocker),
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

fn card_name(db: &CardDb, grp: u32) -> String {
    db.cards
        .get(&grp)
        .map(|c| c.name.clone())
        .unwrap_or_default()
}

/// Zugvorschlag: Land, Zauber und was man zurückhalten sollte.
fn turn_hints(v: &mut CompanionView, ls: &LiveState, db: &CardDb) {
    let plan = crate::turnplan::plan(
        db,
        &ls.hand,
        &ls.untapped_lands,
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

/// Schaden, der beim Angriff mit allen Kreaturen mindestens durchkommt, wenn
/// der Gegner bestmöglich blockt (er blockt die stärksten Angreifer, die er
/// blocken kann). Zaubertricks und Trampelschaden bleiben unberücksichtigt.
pub fn unblocked_damage(attackers: &[LiveCreature], blockers: &[LiveCreature]) -> i32 {
    let mut atk: Vec<&LiveCreature> = attackers.iter().collect();
    atk.sort_by_key(|a| std::cmp::Reverse(a.power));
    let mut ground = blockers.iter().filter(|b| !b.reach).count();
    let mut air = blockers.iter().filter(|b| b.reach).count();
    let mut damage = 0;
    for a in atk {
        let need = if a.menace { 2 } else { 1 };
        let blocked = if a.flying {
            // Flieger können nur Kreaturen mit Fliegend/Reichweite blocken.
            if air >= need {
                air -= need;
                true
            } else {
                false
            }
        } else if ground + air >= need {
            // Erst Bodenblocker einsetzen, Luftabwehr für Flieger aufheben.
            let g = need.min(ground);
            ground -= g;
            air -= need - g;
            true
        } else {
            false
        };
        if !blocked {
            damage += a.power;
        }
    }
    damage
}

/// Bester Brandschaden aus der Hand, den das verfügbare Mana bezahlen kann.
fn best_burn(hand: &[&Card], mana: u32) -> (u32, Vec<String>) {
    let burn: Vec<&&Card> = hand
        .iter()
        .filter(|c| c.burn > 0 && c.cmc <= mana)
        .take(10)
        .collect();
    let mut best = (0u32, Vec::new());
    for mask in 1u32..(1 << burn.len()) {
        let chosen: Vec<&&&Card> = burn
            .iter()
            .enumerate()
            .filter(|(i, _)| mask & (1 << i) != 0)
            .map(|(_, c)| c)
            .collect();
        let cost: u32 = chosen.iter().map(|c| c.cmc).sum();
        let dmg: u32 = chosen.iter().map(|c| c.burn).sum();
        if cost <= mana && dmg > best.0 {
            best = (dmg, chosen.iter().map(|c| c.name.clone()).collect());
        }
    }
    best
}

/// Liest eine wachsende Logdatei zeilenweise nach.
#[derive(Default)]
pub struct LogTail {
    pos: u64,
    partial: Vec<u8>,
}

impl LogTail {
    /// Übergibt alle seit dem letzten Aufruf vollständig geschriebenen Zeilen
    /// (ohne Zeilenende). Ist die Datei kürzer geworden, wurde sie neu
    /// begonnen: Dann wird nichts gelesen, der Zustand zurückgesetzt und
    /// `true` zurückgegeben.
    pub fn read(&mut self, path: &Path, mut on_line: impl FnMut(&[u8])) -> bool {
        let len = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        if len < self.pos {
            self.pos = 0;
            self.partial.clear();
            return true;
        }
        if len > self.pos
            && let Ok(mut file) = File::open(path)
            && file.seek(SeekFrom::Start(self.pos)).is_ok()
        {
            let mut buf = Vec::new();
            if let Ok(n) = file.take(len - self.pos).read_to_end(&mut buf) {
                self.pos += n as u64;
                self.partial.extend_from_slice(&buf);
                if let Some(cut) = self.partial.iter().rposition(|b| *b == b'\n') {
                    let rest = self.partial.split_off(cut + 1);
                    for line in self.partial.split(|b| *b == b'\n') {
                        if !line.is_empty() {
                            on_line(line.strip_suffix(b"\r").unwrap_or(line));
                        }
                    }
                    self.partial = rest;
                }
            }
        }
        false
    }
}

fn find_log() -> Option<PathBuf> {
    crate::paths::player_log_candidates()
        .into_iter()
        .find(|p| p.is_file())
}

/// Verfolgt das Arena-Log und merkt sich die zuletzt beendete Partie.
pub struct Follower {
    parser: LogParser,
    tail: LogTail,
    last: Option<(GameRecord, Instant)>,
    last_id: String,
    catching_up: bool,
}

impl Default for Follower {
    fn default() -> Follower {
        Follower {
            parser: LogParser::default(),
            tail: LogTail::default(),
            last: None,
            last_id: String::new(),
            catching_up: true,
        }
    }
}

impl Follower {
    /// Liest neue Logzeilen und berechnet die Anzeige.
    pub fn poll(&mut self, path: &Path, db: &CardDb, meta: &[ArchShare]) -> CompanionView {
        let parser = &mut self.parser;
        if self.tail.read(path, |line| parser.feed(line)) {
            // Arena wurde neu gestartet und hat ein neues Log begonnen.
            self.parser = LogParser::default();
            self.catching_up = true;
            let parser = &mut self.parser;
            self.tail.read(path, |line| parser.feed(line));
        }
        self.parser.take_finished();
        if let Some(g) = &self.parser.last_finished
            && g.id != self.last_id
        {
            self.last_id = g.id.clone();
            // Beim ersten Nachlesen des Logs beendete Partien nicht als „gerade vorbei“ melden.
            if !self.catching_up {
                self.last = Some((g.clone(), Instant::now()));
            }
        }
        self.catching_up = false;
        let live = self.parser.live();
        build_view(live.as_ref(), self.last.as_ref(), db, meta)
    }
}

/// Startet das Mitlesen im Hintergrund.
pub fn spawn(db: Arc<CardDb>, shared: Arc<Shared>, ctx: egui::Context, viewport: egui::ViewportId) {
    std::thread::spawn(move || {
        let mut follower = Follower::default();
        loop {
            let view = match find_log() {
                Some(path) => {
                    let meta = shared.meta.read().unwrap();
                    follower.poll(&path, &db, &meta)
                }
                None => CompanionView {
                    status: "Arena-Log nicht gefunden".into(),
                    ..Default::default()
                },
            };
            let changed = {
                let mut cur = shared.view.lock().unwrap();
                let changed = *cur != view;
                if changed {
                    *cur = view;
                }
                changed
            };
            if changed {
                ctx.request_repaint_of(viewport);
            }
            sleep(Duration::from_millis(300));
        }
    });
}

/// Zeichnet das Begleitfenster.
pub fn draw(ui: &mut egui::Ui, view: &CompanionView) {
    let green = egui::Color32::from_rgb(110, 200, 120);
    let yellow = egui::Color32::from_rgb(235, 195, 80);
    ui.heading(&view.status);
    if let Some((me, opp)) = view.life {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(format!("Du: {me}")).strong().size(18.0));
            ui.label(egui::RichText::new(format!("Gegner: {opp}")).size(18.0));
        });
    }
    if !view.board.is_empty() {
        ui.label(egui::RichText::new(&view.board).weak());
    }
    ui.separator();
    for (level, text) in &view.hints {
        let rich = egui::RichText::new(text);
        ui.label(match level {
            Level::Good => rich.color(green).strong(),
            Level::Warn => rich.color(yellow).strong(),
            Level::Info => rich,
        });
    }
    if !view.opponent.is_empty() {
        ui.separator();
        ui.label(egui::RichText::new(format!("Gegner: {}", view.opponent)).strong());
        if !view.opp_cards.is_empty() {
            ui.label(egui::RichText::new(view.opp_cards.join(", ")).weak());
        }
    }
}

#[cfg(test)]
#[path = "tests/live.rs"]
mod tests;
