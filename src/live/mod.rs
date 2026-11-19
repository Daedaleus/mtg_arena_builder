//! Begleitfenster: liest das Arena-Log während einer Partie mit und berechnet
//! daraus Hinweise („Land legen“, „gefahrlos angreifen“, „blocken!“ …).
//!
//! Es wird nur die Logdatei gelesen, die Arena für Community-Tools schreibt
//! („Detaillierte Logs“). Ins Spiel greift das Fenster nicht ein.
//!
//! Aufbau: `view` (Anzeige berechnen), `hints` (Zug- und Blockvorschlag),
//! `damage` (Siegchancen) und `follow` (Log mitlesen). Gezeichnet wird in
//! der Oberfläche (`app::companion`); dieses Modul kennt kein egui.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, RwLock};
use std::thread::sleep;
use std::time::{Duration, Instant};

use crate::carddb::{Card, CardDb, GrpId};
use crate::combat::unblocked_damage;
use crate::games::arena::Phase;
use crate::games::{GameRecord, LiveState, LogParser};

mod damage;
mod follow;
mod hints;
mod view;

use damage::*;
pub use follow::*;
use hints::*;
pub use view::*;

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
    /// Mitlesen beenden (der Thread endet beim nächsten Durchlauf).
    pub stop: AtomicBool,
}

#[cfg(test)]
#[path = "../tests/live.rs"]
mod tests;
