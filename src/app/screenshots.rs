//! Screenshots for the README: `--screenshots DIR` goes through the tabs one
//! after another, selects an entry there, waits until data and card images
//! are loaded, saves one PNG per tab and closes the window.
//!
//! The interface is English in this mode, and nothing is saved to the
//! settings (see `App::save_settings`).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui;

use super::{App, Tab};

/// Tab, file name and waiting time after selecting. Tournament decks and
/// events only load their lists on selection, hence longer. Events first:
/// after choosing an archetype, many MTGTop8 fetches (at most every 5 s)
/// queue up before magic.gg.
const SHOTS: [(Tab, &str, u64); 5] = [
    (Tab::Events, "official-events.png", 10),
    (Tab::Meta, "tournament-decks.png", 15),
    (Tab::Games, "my-games.png", 4),
    (Tab::DeckCheck, "improve-deck.png", 4),
    (Tab::Built, "from-my-collection.png", 4),
];

/// Waiting time before the first image (card database, games, archetypes).
const FIRST_WAIT: Duration = Duration::from_secs(8);
/// Wait at most this long for something to select.
const PREPARE_LIMIT: Duration = Duration::from_secs(20);

pub(super) struct Shooter {
    dir: PathBuf,
    next: usize,
    since: Instant,
    prepared: bool,
    requested: bool,
}

impl Shooter {
    pub fn new(dir: PathBuf) -> Shooter {
        Shooter {
            dir,
            next: 0,
            since: Instant::now() + FIRST_WAIT,
            prepared: false,
            requested: false,
        }
    }
}

impl App {
    /// Drives the screenshot mode; without `shooter` nothing happens.
    pub(super) fn drive_screenshots(&mut self, ctx: &egui::Context) {
        let Some((next, since, prepared)) =
            self.shooter.as_ref().map(|s| (s.next, s.since, s.prepared))
        else {
            return;
        };
        ctx.request_repaint_after(Duration::from_millis(200));
        let Some(&(tab, file, wait)) = SHOTS.get(next) else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        };
        self.tab = tab;
        if Instant::now() < since {
            return;
        }
        if !prepared {
            let done = self.prepare_shot(tab) || since.elapsed() > PREPARE_LIMIT;
            if done && let Some(s) = self.shooter.as_mut() {
                s.prepared = true;
                s.since = Instant::now() + Duration::from_secs(wait);
            }
            return;
        }

        let image = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        let Some(shooter) = self.shooter.as_mut() else {
            return;
        };
        if let Some(image) = image.filter(|_| shooter.requested) {
            let path = shooter.dir.join(file);
            if let Err(e) = save_png(&image, &path) {
                eprintln!("{}: {e}", path.display());
            }
            shooter.next += 1;
            shooter.prepared = false;
            shooter.requested = false;
            shooter.since = Instant::now();
        } else if !shooter.requested {
            shooter.requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
        }
    }

    /// Selects something in the tab so the image isn't empty. `false` as long
    /// as there is nothing to select yet.
    fn prepare_shot(&mut self, tab: Tab) -> bool {
        match tab {
            Tab::Meta => {
                if self.tour.sel_arch.is_some() {
                    return true;
                }
                let Some(arch) = self.tour.archetypes.first().cloned() else {
                    return false;
                };
                self.select_archetype(arch);
                true
            }
            Tab::Events => {
                if self.gg.sel_event.is_some() {
                    return true;
                }
                let Some(slug) = self.gg.events.first().map(|e| e.slug.clone()) else {
                    return false;
                };
                self.select_event(slug);
                true
            }
            _ => true,
        }
    }
}

fn save_png(image: &egui::ColorImage, path: &std::path::Path) -> anyhow::Result<()> {
    let [w, h] = image.size;
    let buffer = image::RgbaImage::from_raw(w as u32, h as u32, image.as_raw().to_vec())
        .ok_or_else(|| anyhow::anyhow!("unexpected image size"))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    buffer.save(path)?;
    Ok(())
}
