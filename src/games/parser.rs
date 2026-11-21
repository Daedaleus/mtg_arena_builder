//! Reads log files line by line and collects the finished games.

use super::*;

/// Reads a log file line by line – for reading it completely as well as for
/// following a running game.
#[derive(Default)]
pub struct LogParser {
    out: Vec<GameRecord>,
    events: HashMap<String, String>,
    deck: Vec<GrpId>,
    cur: Option<Tracker>,
    /// Last finished game (for the "game over" display).
    pub last_finished: Option<GameRecord>,
    /// Last reported rank.
    pub rank: Option<Rank>,
    /// The last finished game is still waiting for the rank after it.
    rank_pending: bool,
}

impl LogParser {
    /// Processes one line of `Player.log`. Only JSON lines are evaluated:
    /// rank, game room (event) and messages of the game engine.
    pub fn feed(&mut self, line: &[u8]) {
        if line.first() != Some(&b'{') {
            return;
        }
        let text = String::from_utf8_lossy(line);
        if text.contains("\"constructedSeasonOrdinal\"") {
            self.on_rank(&text);
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
        if is_room {
            self.on_room(&o);
            return;
        }
        let ts = o["timestamp"]
            .as_str()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0u64);
        for m in o["greToClientEvent"]["greToClientMessages"]
            .as_array()
            .into_iter()
            .flatten()
        {
            self.on_message(m, ts);
        }
    }

    /// Rank according to Arena; after a game it belongs to that game.
    fn on_rank(&mut self, text: &str) {
        let Some(rank) = Rank::parse(text) else {
            return;
        };
        if self.rank_pending
            && let Some(last) = self.out.last_mut()
        {
            last.rank_after = Some(rank.clone());
        }
        self.rank_pending = false;
        self.rank = Some(rank);
    }

    /// Game room: remember the event of the game (e.g. ranked) for the own seat.
    fn on_room(&mut self, o: &Value) {
        let cfg = &o["matchGameRoomStateChangedEvent"]["gameRoomInfo"]["gameRoomConfig"];
        let Some(match_id) = cfg["matchId"].as_str() else {
            return;
        };
        let seat = self.cur.as_ref().map(|t| t.seat()).unwrap_or(1);
        for p in cfg["reservedPlayers"].as_array().into_iter().flatten() {
            if u(&p["systemSeatId"]) == seat
                && let Some(ev) = p["eventId"].as_str()
            {
                self.events.insert(match_id.to_string(), ev.to_string());
            }
        }
    }

    /// A message of the game engine (GRE).
    fn on_message(&mut self, m: &Value, ts: u64) {
        let seat = m["systemSeatIds"][0].as_u64().unwrap_or(1) as u32;
        match m["type"].as_str().unwrap_or("") {
            message::CONNECT_RESP => {
                self.deck = m["connectResp"]["deckMessage"]["deckCards"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|v| GrpId(u(v)))
                    .collect();
            }
            message::MULLIGAN_REQ => {
                if let Some(t) = self.cur.as_mut() {
                    t.mulligan_reqs += 1;
                    // The hand offered last is the one kept.
                    t.rec.opening_hand = t.hand();
                }
            }
            _ => {}
        }
        let g = &m["gameStateMessage"];
        if !g.is_null() {
            self.on_game_state(g, seat, ts);
        }
    }

    /// New game state: assign the game (or start a new one) and apply it.
    fn on_game_state(&mut self, g: &Value, seat: u32, ts: u64) {
        let info = &g["gameInfo"];
        let known = info["matchID"]
            .as_str()
            .map(|m| format!("{m}#{}", info["gameNumber"].as_u64().unwrap_or(1)));
        if self.is_new_game(g, known.as_deref(), seat) {
            // A previous game without a result is dropped.
            let mut t = Tracker::new(seat, ts);
            t.rec.deck = std::mem::take(&mut self.deck);
            self.cur = Some(t);
        }
        let Some(t) = self.cur.as_mut() else { return };
        if let Some(id) = known {
            t.rec.id = id;
        }
        t.apply(g);
        if info["stage"].as_str() == Some(message::GAME_OVER) {
            self.on_game_over(g, info);
        }
    }

    /// Does the game state belong to a new game? Arena doesn't send the game
    /// id with every message. If the log starts in the middle of a game, it
    /// is only known at the end.
    fn is_new_game(&self, g: &Value, known: Option<&str>, seat: u32) -> bool {
        match (&self.cur, known) {
            (None, _) => true,
            (Some(t), Some(_)) if t.rec.id.is_empty() => {
                // After the end of a game Arena still sends messages without an
                // id; the tracker created from them carries the old game's high
                // counter. If the counter jumps back, a new game begins.
                g["gameStateId"].as_u64().unwrap_or(0) < t.last_state_id
            }
            (Some(t), Some(id)) => t.rec.id != id,
            (Some(t), None) => t.seat() != seat,
        }
    }

    /// End of the game: determine the result and finish the game.
    fn on_game_over(&mut self, g: &Value, info: &Value) {
        let Some(r) = info["results"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|r| r["scope"].as_str() == Some(message::SCOPE_GAME))
        else {
            return;
        };
        let Some(done) = self.cur.take() else { return };
        let my_team = g["players"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|p| u(&p["systemSeatNumber"]) == done.seat())
            .map(|p| u(&p["teamId"]))
            .filter(|team| *team > 0)
            .unwrap_or(done.seat());
        let won = match r["result"].as_str() {
            Some(message::RESULT_WIN_LOSS) => Some(u(&r["winningTeamId"]) == my_team),
            _ => None,
        };
        // Without an id (the log began in the middle of a game) it is dropped.
        if done.rec.id.is_empty() {
            return;
        }
        let reason = r["reason"].as_str().unwrap_or("");
        let mut rec = done.finish(won, reason);
        let match_id = rec.id.split('#').next().unwrap_or("");
        rec.event = self.events.get(match_id).cloned().unwrap_or_default();
        self.last_finished = Some(rec.clone());
        self.out.push(rec);
        self.rank_pending = true;
    }

    /// State of the running game, if one is running.
    pub fn live(&self) -> Option<LiveState> {
        self.cur
            .as_ref()
            .filter(|t| t.turn + t.mulligan_reqs > 0)
            .map(|t| t.live())
    }

    /// Collect the games finished so far (following doesn't need to keep them).
    pub fn take_finished(&mut self) -> Vec<GameRecord> {
        std::mem::take(&mut self.out)
    }

    /// All finished games, only the best recording per game.
    pub fn finish(mut self) -> Vec<GameRecord> {
        for rec in &mut self.out {
            let match_id = rec.id.split('#').next().unwrap_or("");
            rec.event = self.events.get(match_id).cloned().unwrap_or_default();
        }
        // Arena often reports the end of a game twice; the second report creates
        // an empty copy of the same game. Keep only the best recording per game.
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

/// Reads all (finished) games from a log file.
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

/// How complete is a recording? Higher = better.
pub(super) fn quality(g: &GameRecord) -> (bool, usize, bool, bool) {
    (
        g.complete,
        g.turns.len(),
        !g.opening_hand.is_empty(),
        !g.deck.is_empty(),
    )
}

/// Does the log say that "Detailed Logs" are turned off?
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
