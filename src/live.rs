//! Begleitfenster: liest das Arena-Log während einer Partie mit und berechnet
//! daraus Hinweise („Land legen“, „gefahrlos angreifen“, „blocken!“ …).
//!
//! Es wird nur die Logdatei gelesen, die Arena für Community-Tools schreibt
//! („Detaillierte Logs“). Ins Spiel greift das Fenster nicht ein.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, RwLock};
use std::thread::sleep;
use std::time::{Duration, Instant};

use eframe::egui;

use crate::carddb::{CardDb, name_key};
use crate::games::{GameRecord, LiveState, LogParser};

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

/// Anteil der Turnierlisten eines Archetyps, die eine Karte spielen.
pub struct ArchShare {
    pub name: String,
    pub share: HashMap<String, f32>,
}

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
fn guess_opponent(cards: &[u32], db: &CardDb, meta: &[ArchShare]) -> String {
    let seen: HashSet<String> = cards
        .iter()
        .filter_map(|g| db.cards.get(g))
        .filter(|c| !c.is_land())
        .map(|c| name_key(&c.name))
        .collect();
    let best = meta
        .iter()
        .map(|a| {
            let hits: Vec<f32> = seen
                .iter()
                .filter_map(|k| a.share.get(k).copied())
                .collect();
            (a, hits.len(), hits.iter().sum::<f32>())
        })
        .max_by(|a, b| a.2.total_cmp(&b.2));
    match best {
        Some((a, hits, _))
            if hits >= 3 || (seen.len() >= 2 && hits * 2 >= seen.len() && hits >= 2) =>
        {
            format!("vermutlich {}", a.name)
        }
        _ => {
            let ids: Vec<u32> = cards.to_vec();
            let colors = crate::games::deck_colors(&ids, db);
            if colors.is_empty() {
                "noch unbekannt".into()
            } else {
                crate::opponents::color_label(&colors)
            }
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
        if ls.my_turn {
            if !ls.land_played
                && let Some(land) = lands_in_hand.first()
            {
                v.hints.push((
                    Level::Warn,
                    format!("Land legen nicht vergessen ({}).", land.name),
                ));
            }
            if !ls.safe_attackers.is_empty() {
                v.hints.push((
                    Level::Good,
                    format!("Gefahrlos angreifen: {}", names(db, &ls.safe_attackers, 4)),
                ));
            }
            if ls.phase != "Phase_Ending" {
                let mana =
                    ls.lands_untapped + (!ls.land_played && !lands_in_hand.is_empty()) as u32;
                let playable: Vec<u32> = hand
                    .iter()
                    .filter(|c| !c.is_land() && c.cmc > 0 && c.cmc <= mana)
                    .map(|c| c.grp_id)
                    .collect();
                if !playable.is_empty() {
                    v.hints.push((
                        Level::Info,
                        format!(
                            "Bezahlbar mit {mana} Mana: {} (Farben nicht geprüft)",
                            names(db, &playable, 4)
                        ),
                    ));
                }
            }
        } else if ls.opp_attackers > 0 {
            if ls.opp_attack_power >= ls.life_me {
                v.hints.push((
                    Level::Warn,
                    format!(
                        "Achtung: {} Schaden greifen an, du hast {} Leben – blocken!",
                        ls.opp_attack_power, ls.life_me
                    ),
                ));
            } else if ls.life_me <= 10 && ls.my_untapped_creatures > 0 {
                v.hints.push((
                    Level::Warn,
                    "Wenig Leben – blocken lohnt sich, auch wenn deine Kreatur dabei stirbt."
                        .into(),
                ));
            }
        }
        if v.hints.is_empty() {
            v.hints
                .push((Level::Info, "Gerade nichts zu beachten.".into()));
        }
    }

    v.opponent = guess_opponent(&ls.opp_cards, db, meta);
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

fn find_log() -> Option<PathBuf> {
    crate::paths::player_log_candidates()
        .into_iter()
        .find(|p| p.is_file())
}

/// Startet das Mitlesen im Hintergrund.
pub fn spawn(db: Arc<CardDb>, shared: Arc<Shared>, ctx: egui::Context, viewport: egui::ViewportId) {
    std::thread::spawn(move || {
        let mut parser = LogParser::default();
        let mut pos = 0u64;
        let mut partial: Vec<u8> = Vec::new();
        let mut last: Option<(GameRecord, Instant)> = None;
        let mut last_id = String::new();
        let mut catching_up = true;
        loop {
            let Some(path) = find_log() else {
                let mut view = shared.view.lock().unwrap();
                view.status = "Arena-Log nicht gefunden".into();
                drop(view);
                sleep(Duration::from_secs(3));
                continue;
            };
            let len = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            if len < pos {
                // Arena wurde neu gestartet und hat ein neues Log begonnen.
                parser = LogParser::default();
                pos = 0;
                partial.clear();
                catching_up = true;
            }
            if len > pos
                && let Ok(mut file) = File::open(&path)
                && file.seek(SeekFrom::Start(pos)).is_ok()
            {
                let mut buf = Vec::new();
                if let Ok(n) = file.take(len - pos).read_to_end(&mut buf) {
                    pos += n as u64;
                    partial.extend_from_slice(&buf);
                    if let Some(cut) = partial.iter().rposition(|b| *b == b'\n') {
                        let rest = partial.split_off(cut + 1);
                        for line in partial.split(|b| *b == b'\n') {
                            parser.feed(line.strip_suffix(b"\r").unwrap_or(line));
                        }
                        partial = rest;
                    }
                    parser.take_finished();
                }
            }
            if let Some(g) = &parser.last_finished
                && g.id != last_id
            {
                last_id = g.id.clone();
                // Beim ersten Nachlesen des Logs beendete Partien nicht als „gerade vorbei“ melden.
                if !catching_up {
                    last = Some((g.clone(), Instant::now()));
                }
            }
            catching_up = false;
            let live = parser.live();
            let view = {
                let meta = shared.meta.read().unwrap();
                build_view(live.as_ref(), last.as_ref(), &db, &meta)
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
