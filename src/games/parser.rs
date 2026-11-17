//! Liest Logdateien Zeile für Zeile und sammelt die beendeten Partien.

use super::*;

/// Liest eine Logdatei Zeile für Zeile – für das komplette Einlesen ebenso
/// wie für das Mitlesen während einer laufenden Partie.
#[derive(Default)]
pub struct LogParser {
    out: Vec<GameRecord>,
    events: HashMap<String, String>,
    deck: Vec<u32>,
    cur: Option<Tracker>,
    /// Zuletzt beendete Partie (für die Anzeige „Partie vorbei“).
    pub last_finished: Option<GameRecord>,
    /// Zuletzt gemeldeter Rang.
    pub rank: Option<Rank>,
    /// Die zuletzt beendete Partie wartet noch auf den Rang danach.
    rank_pending: bool,
}

impl LogParser {
    pub fn feed(&mut self, line: &[u8]) {
        if line.first() != Some(&b'{') {
            return;
        }
        let text = String::from_utf8_lossy(line);
        if text.contains("\"constructedSeasonOrdinal\"") {
            if let Some(rank) = Rank::parse(&text) {
                if self.rank_pending
                    && let Some(last) = self.out.last_mut()
                {
                    last.rank_after = Some(rank.clone());
                }
                self.rank_pending = false;
                self.rank = Some(rank);
            }
            return;
        }
        let is_gre = text.contains("\"greToClientEvent\"");
        let is_room = text.contains("\"matchGameRoomStateChangedEvent\"");
        if !is_gre && !is_room {
            return;
        }
        let Ok(o) = serde_json::from_str::<Value>(&text) else {
            return;
        };
        let ts = o["timestamp"]
            .as_str()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0u64);

        if is_room {
            let cfg = &o["matchGameRoomStateChangedEvent"]["gameRoomInfo"]["gameRoomConfig"];
            if let Some(match_id) = cfg["matchId"].as_str() {
                let seat = self.cur.as_ref().map(|t| t.seat()).unwrap_or(1);
                for p in cfg["reservedPlayers"].as_array().into_iter().flatten() {
                    if u(&p["systemSeatId"]) == seat
                        && let Some(ev) = p["eventId"].as_str()
                    {
                        self.events.insert(match_id.to_string(), ev.to_string());
                    }
                }
            }
            return;
        }

        for m in o["greToClientEvent"]["greToClientMessages"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let seat = m["systemSeatIds"][0].as_u64().unwrap_or(1) as u32;
            match m["type"].as_str().unwrap_or("") {
                "GREMessageType_ConnectResp" => {
                    self.deck = m["connectResp"]["deckMessage"]["deckCards"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(u)
                        .collect();
                }
                "GREMessageType_MulliganReq" => {
                    if let Some(t) = self.cur.as_mut() {
                        t.mulligan_reqs += 1;
                        // Die zuletzt angebotene Hand ist die behaltene.
                        t.rec.opening_hand = t.hand();
                    }
                }
                _ => {}
            }
            let g = &m["gameStateMessage"];
            if g.is_null() {
                continue;
            }
            let info = &g["gameInfo"];
            let known = info["matchID"]
                .as_str()
                .map(|m| format!("{m}#{}", info["gameNumber"].as_u64().unwrap_or(1)));
            // Arena schickt die Partie-Kennung nicht in jeder Nachricht mit. Beginnt
            // das Log mitten in einer Partie, steht sie erst am Ende fest.
            let new_game = match (&self.cur, &known) {
                (None, _) => true,
                (Some(t), Some(id)) if t.rec.id.is_empty() => {
                    // Nach Spielende schickt Arena noch Nachrichten ohne Kennung; der
                    // daraus entstandene Tracker trägt den hohen Zählerstand der alten
                    // Partie. Springt der Zähler zurück, beginnt eine neue Partie.
                    g["gameStateId"].as_u64().unwrap_or(0) < t.last_state_id
                }
                (Some(t), Some(id)) => &t.rec.id != id,
                (Some(t), None) => t.seat() != seat,
            };
            if new_game {
                // Eine vorige Partie ohne Ergebnis wird verworfen.
                let mut t = Tracker::new(seat, ts);
                t.rec.deck = std::mem::take(&mut self.deck);
                self.cur = Some(t);
            }
            if let (Some(t), Some(id)) = (self.cur.as_mut(), known) {
                t.rec.id = id;
            }
            let Some(t) = self.cur.as_mut() else { continue };
            if t.finished {
                continue;
            }
            t.apply(g);
            if info["stage"].as_str() == Some("GameStage_GameOver") {
                let result = info["results"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|r| r["scope"].as_str() == Some("MatchScope_Game"));
                if let Some(r) = result {
                    t.finished = true;
                    let my_team = g["players"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .find(|p| u(&p["systemSeatNumber"]) == t.seat())
                        .map(|p| u(&p["teamId"]))
                        .filter(|team| *team > 0)
                        .unwrap_or(t.seat());
                    let won = match r["result"].as_str() {
                        Some("ResultType_WinLoss") => Some(u(&r["winningTeamId"]) == my_team),
                        _ => None,
                    };
                    let reason = r["reason"].as_str().unwrap_or("").to_string();
                    let done = self.cur.take().unwrap();
                    if !done.rec.id.is_empty() {
                        let mut rec = done.finish(won, &reason);
                        let match_id = rec.id.split('#').next().unwrap_or("");
                        rec.event = self.events.get(match_id).cloned().unwrap_or_default();
                        self.last_finished = Some(rec.clone());
                        self.out.push(rec);
                        self.rank_pending = true;
                    }
                }
            }
        }
    }

    /// Zustand der laufenden Partie, falls gerade eine läuft.
    pub fn live(&self) -> Option<LiveState> {
        self.cur
            .as_ref()
            .filter(|t| !t.finished && t.turn + t.mulligan_reqs > 0)
            .map(|t| t.live())
    }

    /// Bisher beendete Partien abholen (das Mitlesen braucht sie nicht aufzuheben).
    pub fn take_finished(&mut self) -> Vec<GameRecord> {
        std::mem::take(&mut self.out)
    }

    /// Alle beendeten Partien, je Partie nur die beste Aufzeichnung.
    pub fn finish(mut self) -> Vec<GameRecord> {
        for rec in &mut self.out {
            let match_id = rec.id.split('#').next().unwrap_or("");
            rec.event = self.events.get(match_id).cloned().unwrap_or_default();
        }
        // Arena meldet das Spielende oft doppelt; die zweite Meldung erzeugt eine
        // leere Kopie derselben Partie. Pro Partie nur die beste Aufzeichnung behalten.
        let mut best: Vec<GameRecord> = Vec::new();
        for rec in self.out {
            match best.iter_mut().find(|b| b.id == rec.id) {
                Some(b) => {
                    let deck = if b.deck.is_empty() {
                        rec.deck.clone()
                    } else {
                        Vec::new()
                    };
                    let rank = b.rank_after.clone().or_else(|| rec.rank_after.clone());
                    if quality(&rec) > quality(b) {
                        *b = rec;
                    }
                    if b.deck.is_empty() {
                        b.deck = deck;
                    }
                    b.rank_after = rank;
                }
                None => best.push(rec),
            }
        }
        best
    }
}

/// Liest alle (abgeschlossenen) Partien aus einer Logdatei.
pub fn parse_log(path: &Path) -> Vec<GameRecord> {
    let Ok(file) = File::open(path) else {
        return Vec::new();
    };
    let mut parser = LogParser::default();
    for line in BufReader::new(file).split(b'\n').map_while(Result::ok) {
        parser.feed(&line);
    }
    parser.finish()
}

/// Wie vollständig ist eine Aufzeichnung? Höher = besser.
pub(super) fn quality(g: &GameRecord) -> (bool, usize, bool, bool) {
    (
        g.complete,
        g.turns.len(),
        !g.opening_hand.is_empty(),
        !g.deck.is_empty(),
    )
}

/// Steht im Log, dass „Detaillierte Logs“ ausgeschaltet sind?
pub fn detailed_logs_disabled(path: &Path) -> bool {
    let Ok(file) = File::open(path) else {
        return false;
    };
    BufReader::new(file)
        .lines()
        .take(2000)
        .map_while(Result::ok)
        .any(|l| l.contains("DETAILED LOGS: DISABLED"))
}
