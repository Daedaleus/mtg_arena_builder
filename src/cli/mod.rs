//! Diagnose-Kommandozeile. Ohne Argument startet `main` die Oberfläche.

use std::path::Path;

use anyhow::{Context, anyhow};
use mtg_arena_builder::{
    budget, builder, carddb, collection, deck, deckcheck, games, images, live, meta, opponents,
    paths, progress, sim, similar,
};

mod played;
mod setup;
mod tournament;

/// Fehler ausgeben; Rückgabe ist der Exit-Code.
fn done(result: anyhow::Result<()>) -> i32 {
    match result {
        Ok(()) => 0,
        Err(e) => {
            println!("FEHLER: {e:#}");
            1
        }
    }
}

/// Führt einen Kommandozeilen-Aufruf aus. `None`, wenn kein bekannter Befehl
/// angegeben ist (dann startet die Oberfläche); sonst der Exit-Code.
pub fn run(args: &[String]) -> Option<i32> {
    let arg = |i: usize, default: &str| {
        args.get(i)
            .map(String::as_str)
            .unwrap_or(default)
            .to_string()
    };
    let rest = args.get(2..).unwrap_or_default();
    let code = match args.get(1).map(String::as_str)? {
        "--sync-collection" => setup::sync_collection(&arg(2, "")),
        "--check" => setup::check(),
        "--image" => {
            setup::image(rest);
            0
        }
        "--report" => done(tournament::report(&arg(2, "ST"))),
        "--variants" => done(tournament::variants(&arg(2, "ST"), &arg(3, ""))),
        "--build" => done(tournament::build(&arg(2, "ST"))),
        "--budget" => done(tournament::budget(&arg(2, "ST"), &arg(3, ""))),
        "--gg" => done(tournament::gg(&arg(2, "ST"))),
        "--games" => done(played::games_cli()),
        "--decks" => done(played::decks_cli(&arg(2, "ST"))),
        "--parse" => {
            played::parse(rest);
            0
        }
        "--replay" => {
            let limit = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(60);
            done(played::replay(&arg(2, ""), limit))
        }
        "--sim" => done(played::sim()),
        "--style" => done(played::style_cli()),
        _ => return None,
    };
    Some(code)
}
