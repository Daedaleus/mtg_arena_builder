//! Computation away from drawing: evaluations are cached and only recomputed
//! when their basis changes; sample hands and deck building run on the task
//! thread (`tasks`) and report back via [`Msg`].
//!
//! The basis of an evaluation is `App::data_gen`: the counter rises with
//! every incoming message except card images, sample hands and progress
//! messages
//! (games, tournament lists, collection …).

use super::*;
use crate::i18n::{self, Locale};

/// What sample hands run for.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) enum SimTarget {
    /// Deck in the deck view (tournament deck, own list, built deck) with the
    /// state of the list at the start (see `SimState::list_gen`).
    Deck(DeckId, u64),
    /// Deck from "Improve deck" (grpIds of the list).
    Group(Vec<GrpId>),
}

/// "My games" analysis for a filter.
pub(super) struct GamesAnalysis {
    key: (u64, games::GamesFilter, Locale),
    /// Decks in the chosen mode for the selection: (display, key).
    pub decks: Vec<(String, Vec<String>)>,
    pub games: Vec<GameRecord>,
    /// Fixed numbers of all stored games (also when filtered by mode).
    pub numbers: games::GameNumbers,
    pub report: games::Report,
    pub opp_groups: Vec<opponents::OppGroup>,
    pub opp_names: Vec<String>,
    pub player: style::PlayerStyle,
    pub recs: Vec<style::Candidate>,
    /// Loaded tournament lists the recommendations come from.
    pub loaded: usize,
}

/// "Improve deck" analysis for a chosen deck.
pub(super) struct DeckAnalysis {
    key: (u64, usize, Locale, GameFormat),
    pub groups: Vec<deckcheck::DeckGroup>,
    /// Chosen deck (index in `groups`) and its analysis.
    pub sel: usize,
    pub check: Option<deckcheck::DeckCheck>,
}

impl App {
    /// "My games" analysis, only recomputed when its basis changed.
    /// `None` as long as card data is missing.
    pub(super) fn games_analysis(&mut self) -> Option<GamesAnalysis> {
        let key = (self.data_gen, self.games.filter.clone(), i18n::current());
        if let Some(a) = self.games.cache.take()
            && a.key == key
        {
            return Some(a);
        }
        let db = self.db.clone()?;
        let filter = &self.games.filter;
        let games: Vec<GameRecord> = self
            .games
            .list
            .iter()
            .filter(|g| filter.matches(g, &db))
            .cloned()
            .collect();
        let decks = self.deck_choices(&db);
        let numbers = games::GameNumbers::new(&self.games.list);
        let report = games::report(&games, &numbers, &db);
        let shares = opponents::arch_shares(&self.meta_archetypes(), &db);
        let opp_groups = opponents::analyze(&games, &db, &shares);
        let opp_names = games
            .iter()
            .map(|g| opponents::classify(g, &db, &shares))
            .collect();
        let player = style::analyze(&games, &db, &report);
        let candidates = self.style_candidates(&db);
        let recs = style::recommend(&candidates, player.recommended, self.wildcards(), 5);
        Some(GamesAnalysis {
            key,
            loaded: candidates.len(),
            decks,
            games,
            numbers,
            report,
            opp_groups,
            opp_names,
            player,
            recs,
        })
    }

    /// Decks of the games in the chosen mode, most recently played first.
    fn deck_choices(&self, db: &CardDb) -> Vec<(String, Vec<String>)> {
        let mode_only = games::GamesFilter {
            format: self.games.filter.format,
            deck: None,
        };
        let in_mode: Vec<GameRecord> = self
            .games
            .list
            .iter()
            .filter(|g| mode_only.matches(g, db))
            .cloned()
            .collect();
        deckcheck::group_decks(&in_mode, db)
            .into_iter()
            .map(|g| (g.label, games::deck_key(&g.deck, db)))
            .collect()
    }

    /// "Improve deck" analysis for the deck at position `sel`: only decks
    /// and games of the chosen game mode.
    pub(super) fn deck_analysis(&mut self, sel: usize) -> Option<DeckAnalysis> {
        let mode = self.settings.mode;
        let key = (self.data_gen, sel, i18n::current(), mode);
        if let Some(a) = self.deck_cache.take()
            && a.key == key
        {
            return Some(a);
        }
        let db = self.db.clone()?;
        let in_mode: Vec<GameRecord> = self
            .games
            .list
            .iter()
            .filter(|g| g.format() == mode)
            .cloned()
            .collect();
        let groups = deckcheck::group_decks(&in_mode, &db);
        let sel = sel.min(groups.len().saturating_sub(1));
        let check = groups.get(sel).map(|group| {
            let meta = self.meta_archetypes();
            deckcheck::check(group, &in_mode, &db, &self.owned, self.legal(), &meta)
        });
        Some(DeckAnalysis {
            key,
            groups,
            sel,
            check,
        })
    }

    /// Starts sample hands in the background (nothing if they are already running).
    pub(super) fn start_trial(&mut self, target: SimTarget, deck: Vec<GrpId>, rules: sim::Rules) {
        let Some(db) = self.db.clone() else { return };
        if !self.sims.running.insert(target.clone()) {
            return;
        }
        let history = self.games.list.clone();
        self.tasks.run(move || {
            let trial = sim::trial(&deck, &db, &history, sim::DEFAULT_GAMES, 1, rules);
            Msg::Trial(target, Box::new(trial))
        });
    }

    pub(super) fn trial_running(&self, target: &SimTarget) -> bool {
        self.sims.running.contains(target)
    }

    /// Takes over the result of sample hands.
    pub(super) fn finish_trial(&mut self, target: SimTarget, trial: sim::Trial) {
        self.sims.running.remove(&target);
        match target {
            // Meanwhile a different list under this id: discard the result.
            SimTarget::Deck(id, started) => {
                if started == self.sims.list_gen(id) {
                    self.sims.decks.insert(id, trial);
                }
            }
            SimTarget::Group(deck) => {
                self.sims.groups.insert(deck, trial);
            }
        }
    }

    /// Builds deck suggestions from the collection in the background.
    pub(super) fn start_build(&mut self) {
        let Some(db) = self.db.clone() else { return };
        if self.built.running {
            return;
        }
        self.built.running = true;
        // Tournament presence: in how many loaded tournament lists does a card appear?
        let meta_use = deck::usage_counts(
            self.evals
                .iter()
                .filter(|(id, _)| id.is_tournament())
                .map(|(_, e)| e),
        );
        let owned = self.owned.clone();
        let legal = self.legal().cloned();
        self.built.lists = self.lists.len();
        self.tasks
            .run(move || Msg::Built(builder::build_all(&db, &owned, legal.as_ref(), &meta_use)));
    }
}
