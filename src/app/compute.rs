//! Rechenarbeit abseits des Zeichnens: Auswertungen werden zwischengespeichert
//! und nur neu berechnet, wenn sich ihre Grundlage ändert; Probehände und der
//! Deckbau laufen im Aufgaben-Thread (`tasks`) und melden sich per [`Msg`] zurück.
//!
//! Grundlage einer Auswertung ist `App::data_gen`: Der Zähler steigt mit
//! jeder eingetroffenen Nachricht außer Kartenbildern, Probehänden und
//! Fortschrittsmeldungen
//! (Partien, Turnierlisten, Sammlung …).

use super::*;
use crate::i18n::{self, Locale};

/// Wofür Probehände laufen.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) enum SimTarget {
    /// Deck in der Deckansicht (Turnierdeck, eigene Liste, gebautes Deck)
    /// mit dem Stand der Liste beim Start (siehe `SimState::list_gen`).
    Deck(DeckId, u64),
    /// Deck aus „Deck verbessern“ (grpIds der Liste).
    Group(Vec<GrpId>),
}

/// Auswertung „Meine Partien“ für einen Filter.
pub(super) struct GamesAnalysis {
    key: (u64, Option<String>, Locale),
    pub games: Vec<GameRecord>,
    /// Feste Nummern aller gespeicherten Partien (auch bei Filter nach Modus).
    pub numbers: games::GameNumbers,
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
        let key = (self.data_gen, self.games.event.clone(), i18n::current());
        if let Some(a) = self.games.cache.take()
            && a.key == key
        {
            return Some(a);
        }
        let db = self.db.clone()?;
        let games: Vec<GameRecord> = self
            .games
            .list
            .iter()
            .filter(|g| self.games.event.as_ref().is_none_or(|e| &g.event == e))
            .cloned()
            .collect();
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
            games,
            numbers,
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
        let groups = deckcheck::group_decks(&self.games.list, &db);
        let sel = sel.min(groups.len().saturating_sub(1));
        let check = groups.get(sel).map(|group| {
            let meta = self.meta_archetypes();
            let legal = self.tour.legal.get(&self.settings.format);
            deckcheck::check(group, &self.games.list, &db, &self.owned, legal, &meta)
        });
        Some(DeckAnalysis {
            key,
            groups,
            sel,
            check,
        })
    }

    /// Startet Probehände im Hintergrund (nichts, wenn sie schon laufen).
    pub(super) fn start_trial(&mut self, target: SimTarget, deck: Vec<GrpId>) {
        let Some(db) = self.db.clone() else { return };
        if !self.sims.running.insert(target.clone()) {
            return;
        }
        let history = self.games.list.clone();
        self.tasks.run(move || {
            let trial = sim::trial(&deck, &db, &history, sim::DEFAULT_GAMES, 1);
            Msg::Trial(target, Box::new(trial))
        });
    }

    pub(super) fn trial_running(&self, target: &SimTarget) -> bool {
        self.sims.running.contains(target)
    }

    /// Ergebnis von Probehänden übernehmen.
    pub(super) fn finish_trial(&mut self, target: SimTarget, trial: sim::Trial) {
        self.sims.running.remove(&target);
        match target {
            // Inzwischen eine andere Liste unter dieser Kennung: Ergebnis verwerfen.
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

    /// Baut Deckvorschläge aus der Sammlung im Hintergrund.
    pub(super) fn start_build(&mut self) {
        let Some(db) = self.db.clone() else { return };
        if self.built.running {
            return;
        }
        self.built.running = true;
        // Turnier-Präsenz: in wie vielen geladenen Turnierlisten kommt eine Karte vor?
        let meta_use = deck::usage_counts(
            self.evals
                .iter()
                .filter(|(id, _)| id.is_tournament())
                .map(|(_, e)| e),
        );
        let owned = self.owned.clone();
        let legal = self.tour.legal.get(&self.settings.format).cloned();
        self.built.lists = self.lists.len();
        self.tasks
            .run(move || Msg::Built(builder::build_all(&db, &owned, legal.as_ref(), &meta_use)));
    }
}
