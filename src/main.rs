#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

//! MTG Arena Builder: startet die Oberfläche oder, mit Argument, die
//! Diagnose-Kommandozeile (siehe README).

mod cli;

use mtg_arena_builder::app;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    // Fotomodus für das README: `--screenshots [ORDNER]`.
    let shots = (args.get(1).map(String::as_str) == Some("--screenshots"))
        .then(|| std::path::PathBuf::from(args.get(2).map_or("docs/screenshots", String::as_str)));
    if shots.is_none()
        && let Some(code) = cli::run(&args)
    {
        std::process::exit(code);
    }

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 840.0])
            .with_min_inner_size([900.0, 560.0])
            .with_title("MTG Arena Builder")
            .with_icon(
                eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png"))
                    .expect("eingebettetes Logo ist ein gültiges PNG"),
            ),
        ..Default::default()
    };
    let result = eframe::run_native(
        "MTG Arena Builder",
        options,
        Box::new(|cc| {
            Ok(Box::new(match shots {
                Some(dir) => app::App::for_screenshots(cc, dir),
                None => app::App::new(cc),
            }))
        }),
    );
    if let Err(e) = result {
        eprintln!("Fenster konnte nicht geöffnet werden: {e}");
        std::process::exit(1);
    }
}
