//! Hintergrundaufgaben: ein Thread arbeitet sie nacheinander ab und schickt
//! jedes Ergebnis als [`Msg`] an die Oberfläche (Partien einlesen, Legalität,
//! Sammlung lesen, Probehände, Deckbau). Statt eines Threads je Aktion gibt
//! es damit einen festen, der mit der App endet: Wird `Tasks` fallen
//! gelassen, schließt der Kanal und der Thread beendet sich.
//!
//! Netzabrufe für Turnierdecks laufen weiter im eigenen Worker (`worker`),
//! weil MTGTop8 5 s Abstand verlangt und sie sonst alles andere aufhielten.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::{Sender, channel};

use eframe::egui;

use super::Msg;

type Task = Box<dyn FnOnce() -> Msg + Send>;

pub(super) struct Tasks {
    tx: Sender<Task>,
}

impl Tasks {
    /// Startet den Aufgaben-Thread; Ergebnisse gehen an `results`.
    pub fn spawn(results: Sender<Msg>, ctx: egui::Context) -> Tasks {
        let (tx, rx) = channel::<Task>();
        std::thread::Builder::new()
            .name("aufgaben".into())
            .spawn(move || {
                while let Ok(task) = rx.recv() {
                    // Eine abstürzende Aufgabe darf die folgenden nicht mitreißen.
                    if let Ok(msg) = catch_unwind(AssertUnwindSafe(task)) {
                        let _ = results.send(msg);
                        ctx.request_repaint();
                    }
                }
            })
            .expect("Aufgaben-Thread starten");
        Tasks { tx }
    }

    /// Reiht eine Aufgabe ein.
    pub fn run(&self, task: impl FnOnce() -> Msg + Send + 'static) {
        let _ = self.tx.send(Box::new(task));
    }
}
