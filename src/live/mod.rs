//! Companion window: follows the Arena log during a game and computes hints
//! from it ("play a land", "attack safely", "block!" …).
//!
//! Only the log file Arena writes for community tools is read ("Detailed
//! Logs"). The window never acts in the game.
//!
//! Layout: `view` (compute the display), `hints` (turn and block
//! suggestion), `damage` (chances to win) and `follow` (follow the log).
//! Drawing happens in the interface (`app::companion`); this module knows no egui.

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

/// What the companion window shows right now.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CompanionView {
    pub status: String,
    /// (my life, opponent's life)
    pub life: Option<(i32, i32)>,
    /// Short battlefield overview (lands, creatures).
    pub board: String,
    pub hints: Vec<(Level, String)>,
    pub opponent: String,
    pub opp_cards: Vec<String>,
}

pub use crate::opponents::{ArchShare, arch_shares};

/// Safe attackers that are also worth it (see
/// [`crate::combat::worth_attacking`]).
fn worth_attacking(ls: &LiveState, db: &CardDb) -> Vec<GrpId> {
    let opp_power: i32 = ls.opp_creatures.iter().map(|(_, p)| p).sum();
    crate::combat::worth_attacking(db, &ls.safe_attackers, ls.life_me, opp_power)
}

/// State shared between the live thread, the companion and the main window.
#[derive(Default)]
pub struct Shared {
    pub view: Mutex<CompanionView>,
    pub meta: RwLock<Vec<ArchShare>>,
    /// The companion window was closed with its "X".
    pub close_requested: AtomicBool,
    /// Stop following (the thread ends on its next pass).
    pub stop: AtomicBool,
}

#[cfg(test)]
#[path = "../tests/live.rs"]
mod tests;
