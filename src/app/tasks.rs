//! Background tasks: one thread works through them in order and sends each
//! result as a [`Msg`] to the interface (reading games, legality, reading the
//! collection, sample hands, deck building). Instead of one thread per action
//! there is one fixed thread that ends with the app: when `Tasks` is dropped,
//! the channel closes and the thread ends.
//!
//! Network fetches for tournament decks still run in their own worker
//! (`worker`), because MTGTop8 demands 5 s gaps and would otherwise hold up everything else.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::{Sender, channel};

use eframe::egui;

use super::Msg;

type Task = Box<dyn FnOnce() -> Msg + Send>;

pub(super) struct Tasks {
    tx: Sender<Task>,
}

impl Tasks {
    /// Starts the task thread; results go to `results`.
    pub fn spawn(results: Sender<Msg>, ctx: egui::Context) -> Tasks {
        let (tx, rx) = channel::<Task>();
        std::thread::Builder::new()
            .name("tasks".into())
            .spawn(move || {
                while let Ok(task) = rx.recv() {
                    // A crashing task must not take the following ones down with it.
                    if let Ok(msg) = catch_unwind(AssertUnwindSafe(task)) {
                        let _ = results.send(msg);
                        ctx.request_repaint();
                    }
                }
            })
            .expect("start the task thread");
        Tasks { tx }
    }

    /// Queues a task.
    pub fn run(&self, task: impl FnOnce() -> Msg + Send + 'static) {
        let _ = self.tx.send(Box::new(task));
    }
}
