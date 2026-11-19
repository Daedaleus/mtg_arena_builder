//! Rechenarbeit abseits des Zeichnens: Auswertungen werden zwischengespeichert
//! und nur neu berechnet, wenn sich ihre Grundlage ändert; Probehände und der
//! Deckbau laufen in eigenen Threads und melden sich per [`Msg`] zurück.
//!
//! Grundlage einer Auswertung ist `App::data_gen`: Der Zähler steigt mit
//! jeder eingetroffenen Nachricht außer Kartenbildern und Probehänden
//! (Partien, Turnierlisten, Sammlung …).

use super::*;
use crate::i18n::{self, Locale};

/// Wofür Probehände laufen.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) enum SimTarget {
    /// Deck in der Deckansicht (Turnierdeck, eigene Liste, gebautes Deck).
    Deck(u32),
    /// Deck aus „Deck verbessern“ (grpIds der Liste).
    Group(Vec<u32>),
}

/// Auswertung „Meine Partien“ für einen Filter.
pub(super) struct GamesAnalysis {
    key: (u64, Option<String>, Locale),
    pub games: Vec<GameRecord>,
    pub report: games::Report,
    pub opp_groups: Vec<opponents::OppGroup>,
    pub opp_names: Vec<String>,
    pub player: style::PlayerStyle,
    pub recs: Vec<style::Candidate>,
    /// Geladene Turnierlisten, aus denen die Empfehlungen stammen.
    pub loaded: usize,
}

/// Auswertung „Deck verbessern“ für ein gewähltes Deck.
pub(super) struct DeckAnalysis {
    key: (u64, usize, Locale),
    pub groups: Vec<deckcheck::DeckGroup>,
    /// Gewähltes Deck (Index in `groups`) und seine Auswertung.
    pub sel: usize,
    pub check: Option<deckcheck::DeckCheck>,
}

impl App {
    /// Auswertung „Meine Partien“, nur bei geänderter Grundlage neu berechnet.
    /// `None`, solange Kartendaten fehlen.
    pub(super) fn games_analysis(&mut self) -> Option<GamesAnalysis> {
        let key = (self.data_gen, self.games_event.clone(), i18n::current());
        if let Some(a) = self.games_cache.take()
            && a.key == key
        {
            return Some(a);
        }
        let db = self.db.clone()?;
        let games: Vec<GameRecord> = self
            .games
            .iter()
            .filter(|g| self.games_event.as_ref().is_none_or(|e| &g.event == e))
            .cloned()
            .collect();
        let report = games::report(&games, &db);
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
            games,
            report,
            opp_groups,
            opp_names,
            player,
            recs,
        })
    }

    /// Auswertung „Deck verbessern“ für das Deck an Stelle `sel`.
    pub(super) fn deck_analysis(&mut self, sel: usize) -> Option<DeckAnalysis> {
        let key = (self.data_gen, sel, i18n::current());
        if let Some(a) = self.deck_cache.take()
            && a.key == key
        {
            return Some(a);
        }
        let db = self.db.clone()?;
        let groups = deckcheck::group_decks(&self.games, &db);
        let sel = sel.min(groups.len().saturating_sub(1));
        let check = groups.get(sel).map(|group| {
            let meta = self.meta_archetypes();
            let legal = self.legal.get(&self.settings.format);
            deckcheck::check(group, &self.games, &db, &self.owned, legal, &meta)
        });
        Some(DeckAnalysis {
            key,
            groups,
            sel,
            check,
        })
    }

    /// Startet Probehände im Hintergrund (nichts, wenn sie schon laufen).
    pub(super) fn start_trial(&mut self, target: SimTarget, deck: Vec<u32>) {
        let Some(db) = self.db.clone() else { return };
        if !self.sims_running.insert(target.clone()) {
            return;
        }
        let (tx, ctx, history) = (self.tx_msg.clone(), self.ctx.clone(), self.games.clone());
        std::thread::spawn(move || {
            let trial = sim::trial(&deck, &db, &history, sim::DEFAULT_GAMES, 1);
            let _ = tx.send(Msg::Trial(target, Box::new(trial)));
            ctx.request_repaint();
        });
    }

    pub(super) fn trial_running(&self, target: &SimTarget) -> bool {
        self.sims_running.contains(target)
    }

    /// Ergebnis von Probehänden übernehmen.
    pub(super) fn finish_trial(&mut self, target: SimTarget, trial: sim::Trial) {
        self.sims_running.remove(&target);
        match target {
            SimTarget::Deck(id) => {
                self.sims.insert(id, trial);
            }
            SimTarget::Group(deck) => {
                self.group_sims.insert(deck, trial);
            }
        }
    }

    /// Baut Deckvorschläge aus der Sammlung im Hintergrund.
    pub(super) fn start_build(&mut self) {
        let Some(db) = self.db.clone() else { return };
        if self.building {
            return;
        }
        self.building = true;
        // Turnier-Präsenz: in wie vielen geladenen Turnierlisten kommt eine Karte vor?
        let mut meta_use: HashMap<String, u32> = HashMap::new();
        for (_, eval) in self.evals.iter().filter(|(id, _)| **id > BUILT_MAX) {
            let keys: HashSet<&String> = eval.lines.iter().filter_map(|l| l.key.as_ref()).collect();
            for key in keys {
                *meta_use.entry(key.clone()).or_insert(0) += 1;
            }
        }
        let owned = self.owned.clone();
        let legal = self.legal.get(&self.settings.format).cloned();
        let (tx, ctx) = (self.tx_msg.clone(), self.ctx.clone());
        self.built_lists = self.lists.len();
        std::thread::spawn(move || {
            let built = builder::build_all(&db, &owned, legal.as_ref(), &meta_use);
            let _ = tx.send(Msg::Built(built));
            ctx.request_repaint();
        });
    }
}
