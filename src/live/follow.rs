//! Mitlesen des Arena-Logs im Hintergrund.

use super::*;

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

pub(super) fn find_log() -> Option<PathBuf> {
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
                    status: tr!("live_follow.arena_log_nicht_gefunden"),
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
