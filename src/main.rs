#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod budget;
mod builder;
mod carddb;
mod collection;
mod deck;
mod deckcheck;
mod games;
mod images;
mod live;
mod meta;
mod opponents;
mod paths;
mod similar;

use std::path::Path;

/// `--check`: Diagnose auf der Konsole (Kartendaten, Sammlung, Meta-Abruf).
fn check() -> i32 {
    let db = match paths::find_card_db(None).and_then(|p| {
        println!("Kartendatenbank: {}", p.display());
        carddb::CardDb::load(&p)
    }) {
        Ok(db) => db,
        Err(e) => {
            println!("FEHLER Kartendaten: {e:#}");
            return 1;
        }
    };
    println!("{} Karten geladen (Arena {})", db.cards.len(), db.version);

    let owned = match collection::read_from_game() {
        Ok(c) => {
            println!(
                "Sammlung: {} Karten, Wildcards {:?}, Gold {}, Gems {}",
                c.total_cards(),
                c.wildcards,
                c.gold,
                c.gems
            );
            c.owned_by_name(&db)
        }
        Err(e) => {
            println!(
                "Sammlung nicht lesbar (Admin: {}): {e}",
                collection::is_admin()
            );
            Default::default()
        }
    };

    let mut fetcher = meta::Fetcher::new(paths::data_dir().join("cache"));
    let result = (|| -> anyhow::Result<()> {
        let archetypes = fetcher.archetypes("ST", false)?;
        println!(
            "{} Standard-Archetypen, z. B. {:?}",
            archetypes.len(),
            &archetypes[..archetypes.len().min(3)]
        );
        let decks = fetcher.decks("ST", &archetypes[0])?;
        println!(
            "{} Decks für {}, neuestes: {:?}",
            decks.len(),
            archetypes[0].name,
            decks.first()
        );
        let Some(first) = decks.first() else {
            return Ok(());
        };
        let list = deck::parse(&fetcher.decklist(first.id)?);
        let eval = deck::evaluate(&list, &db, &owned);
        for l in &eval.lines {
            println!(
                "  {} {:<34} {:<9} Besitz {} fehlt {}{}",
                l.qty,
                l.name,
                carddb::rarity_label(l.rarity),
                l.owned,
                l.missing,
                if l.key.is_none() {
                    "  [nicht in Arena]"
                } else {
                    ""
                }
            );
        }
        println!("Zusammenfassung: {:?}", eval.summary(true, |_| 0));
        println!(
            "\n{}",
            deck::export_arena(&eval, &db, None, true, true, |_| None)
        );
        let legal = fetcher.legal_names("standard")?;
        println!("{} Standard-legale Arena-Karten laut Scryfall", legal.len());
        Ok(())
    })();
    match result {
        Ok(()) => 0,
        Err(e) => {
            println!("FEHLER Meta-Abruf: {e:#}");
            1
        }
    }
}

/// `--report [FORMAT]`: bewertet alle Archetypen gegen die gespeicherte Sammlung
/// und gibt eine Rangliste auf der Konsole aus.
fn report(format: &str) -> anyhow::Result<()> {
    use std::collections::HashSet;
    let db = carddb::CardDb::load(&paths::find_card_db(None)?)?;
    let col = collection::Collection::load(&paths::data_dir().join("collection.json"))
        .ok_or_else(|| anyhow::anyhow!("Keine gespeicherte Sammlung"))?;
    let owned = col.owned_by_name(&db);
    let wc = col.wildcards.as_array();
    let mut fetcher = meta::Fetcher::new(paths::data_dir().join("cache"));
    let scry = meta::FORMATS
        .iter()
        .find(|f| f.0 == format)
        .map(|f| f.2)
        .unwrap_or("standard");
    let legal = fetcher.legal_names(scry).ok();
    println!(
        "Sammlung {} Karten, Wildcards C/U/R/M {:?}",
        col.total_cards(),
        wc
    );

    let mut rows = Vec::new();
    for arch in fetcher.archetypes(format, false)? {
        let decks = fetcher.decks(format, &arch)?;
        let Some(first) = decks.first() else { continue };
        let eval = deck::evaluate(&deck::parse(&fetcher.decklist(first.id)?), &db, &owned);
        rows.push((arch, first.clone(), eval));
    }
    rows.sort_by_key(|(_, _, e)| {
        let s = e.summary(false, |_| 0);
        (
            s.unavailable,
            s.need[3] * 4 + s.need[2] * 2 + s.need[1] + s.need[0],
        )
    });
    println!(
        "\n{:<26} {:>4}  {:>9}  {:>15}  {:>15}  craftbar(Main/+Side)",
        "Archetyp", "Meta", "Main %", "Main C/U/R/M", "+Side C/U/R/M"
    );
    for (arch, _, eval) in &rows {
        let (m, a) = (eval.summary(false, |_| 0), eval.summary(true, |_| 0));
        println!(
            "{:<26} {:>3}%  {:>3.0}% {:>2}/{:<2}  {:>15}  {:>15}  {}/{}{}",
            arch.name,
            arch.share,
            m.percent().floor(),
            m.total - m.missing,
            m.total,
            format!("{:?}", m.need),
            format!("{:?}", a.need),
            if m.craftable(wc) { "ja" } else { "nein" },
            if a.craftable(wc) { "ja" } else { "nein" },
            if a.unavailable > 0 {
                format!("  [{} nicht in Arena]", a.unavailable)
            } else {
                String::new()
            },
        );
    }
    for (arch, deck_ref, eval) in rows.iter().take(3) {
        println!(
            "\n=== {} ({}, {}, Platz {}) ===",
            arch.name, deck_ref.player, deck_ref.event, deck_ref.rank
        );
        let in_deck: HashSet<String> = eval.lines.iter().filter_map(|l| l.key.clone()).collect();
        let colors = in_deck
            .iter()
            .filter_map(|k| db.representative(k))
            .filter(|c| !c.is_land())
            .fold(0u8, |m, c| m | c.colors);
        for l in eval.lines.iter().filter(|l| l.missing > 0) {
            let subs = l
                .key
                .as_deref()
                .map(|k| similar::suggest(&db, &owned, legal.as_ref(), colors, &in_deck, k, 3))
                .unwrap_or_default();
            println!(
                "  fehlt {}x {:<32} {:<8} {} Ersatz: {}",
                l.missing,
                l.name,
                carddb::rarity_label(l.rarity),
                if l.side { "(SB)" } else { "    " },
                subs.iter()
                    .map(|s| s.name.as_str())
                    .collect::<Vec<_>>()
                    .join(" | ")
            );
        }
    }
    Ok(())
}

/// `--variants FORMAT NAME`: bewertet alle Turnierlisten eines Archetyps.
fn variants(format: &str, name: &str) -> anyhow::Result<()> {
    let db = carddb::CardDb::load(&paths::find_card_db(None)?)?;
    let col = collection::Collection::load(&paths::data_dir().join("collection.json"))
        .ok_or_else(|| anyhow::anyhow!("Keine gespeicherte Sammlung"))?;
    let owned = col.owned_by_name(&db);
    let mut fetcher = meta::Fetcher::new(paths::data_dir().join("cache"));
    let arch = fetcher
        .archetypes(format, false)?
        .into_iter()
        .find(|a| a.name.eq_ignore_ascii_case(name))
        .ok_or_else(|| anyhow::anyhow!("Archetyp {name} nicht gefunden"))?;
    println!(
        "{:<30} {:<30} {:>6} {:>9}  Main C/U/R/M",
        "Spieler", "Turnier", "Platz", "Datum"
    );
    for d in fetcher.decks(format, &arch)? {
        let eval = deck::evaluate(&deck::parse(&fetcher.decklist(d.id)?), &db, &owned);
        let s = eval.summary(false, |_| 0);
        println!(
            "{:<30.30} {:<30.30} {:>6} {:>9}  {:?}{}",
            d.player,
            d.event,
            d.rank,
            d.date,
            s.need,
            if s.unavailable > 0 {
                format!("  [{} nicht in Arena]", s.unavailable)
            } else {
                String::new()
            }
        );
    }
    Ok(())
}

/// `--build [FORMAT]`: Deckvorschläge aus der gespeicherten Sammlung.
fn build(format: &str) -> anyhow::Result<()> {
    let db = carddb::CardDb::load(&paths::find_card_db(None)?)?;
    let col = collection::Collection::load(&paths::data_dir().join("collection.json"))
        .ok_or_else(|| anyhow::anyhow!("Keine gespeicherte Sammlung"))?;
    let owned = col.owned_by_name(&db);
    let mut fetcher = meta::Fetcher::new(paths::data_dir().join("cache"));
    let scry = meta::FORMATS
        .iter()
        .find(|f| f.0 == format)
        .map(|f| f.2)
        .unwrap_or("standard");
    let legal = fetcher.legal_names(scry).ok();
    // Turnier-Präsenz aus allen bereits heruntergeladenen Listen
    let mut meta_use = std::collections::HashMap::new();
    for arch in fetcher.archetypes(format, false)? {
        for d in fetcher.cached_decks(format, &arch).unwrap_or_default() {
            let Some(text) = fetcher.cached_decklist(d.id) else {
                continue;
            };
            let list = deck::parse(&text);
            let keys: std::collections::HashSet<String> = list
                .main
                .iter()
                .chain(&list.side)
                .filter_map(|e| db.resolve(&e.name))
                .collect();
            for k in keys {
                *meta_use.entry(k).or_insert(0u32) += 1;
            }
        }
    }
    let built = builder::build_all(&db, &owned, legal.as_ref(), &meta_use);
    for b in &built {
        println!(
            "{:<16} Stärke {:.2}  {} Kreaturen, {} Removal, Ø {:.1} Mana, {} Rares/Mythics, {} Turnierkarten",
            b.label, b.score, b.creatures, b.removal, b.avg_cmc, b.rares, b.meta_cards
        );
    }
    for b in built.iter().take(2) {
        println!(
            "
=== {} ===",
            b.label
        );
        for e in &b.deck.main {
            let c = db
                .resolve(&e.name)
                .and_then(|k| db.representative(&k).cloned());
            println!(
                "  {} {:<34} {:<10} {}",
                e.qty,
                e.name,
                c.as_ref().map(|c| c.mana_cost.clone()).unwrap_or_default(),
                c.as_ref()
                    .map(|c| carddb::rarity_label(c.rarity))
                    .unwrap_or("")
            );
        }
    }
    Ok(())
}

/// `--games`: Partien aus den Arena-Logs einlesen und auswerten.
fn games_cli() -> anyhow::Result<()> {
    let db = carddb::CardDb::load(&paths::find_card_db(None)?)?;
    let games = games::sync(&paths::data_dir().join("games.json"), &paths::player_logs());
    for (i, g) in games.iter().enumerate() {
        let (me, opp) = g.final_life();
        println!(
            "#{} {} {:<8} {} {} Mull {} Züge {} Leben {}:{} Deck {} Gegner {} {}",
            i + 1,
            g.id.get(..8).unwrap_or(""),
            g.event,
            match g.won {
                Some(true) => "SIEG",
                Some(false) => "NIEDERLAGE",
                None => "?",
            },
            match g.on_play {
                Some(true) => "angefangen",
                Some(false) => "gezogen",
                None => "?",
            },
            g.mulligans,
            g.last_turn(),
            me,
            opp,
            games::deck_colors(&g.deck, &db),
            games::opp_colors(g, &db),
            if g.complete {
                String::new()
            } else {
                format!(
                    "(erst ab Zug {} mitgeschrieben)",
                    g.turns.first().map(|t| t.turn).unwrap_or(0)
                )
            }
        );
        for t in &g.turns {
            println!(
                "   Zug {:>2} {:<6} Leben {:>3}:{:<3} Länder {}:{} Kreaturen {}:{} frei {} Hand {:>2} Land {} Zauber {} Angriff {}",
                t.turn,
                if t.mine { "ich" } else { "Gegner" },
                t.life_me,
                t.life_opp,
                t.lands_me,
                t.lands_opp,
                t.creatures_me,
                t.creatures_opp,
                t.untapped_lands,
                t.hand.len(),
                if t.land_played { "ja" } else { "-" },
                t.spells,
                if t.attacked { "ja" } else { "-" }
            );
        }
    }
    let r = games::report(&games, &db);
    println!(
        "\n{} Siege, {} Niederlagen ({} aufgegeben), Ø {:.1} Züge, {} eigene Züge ausgewertet",
        r.wins, r.losses, r.conceded, r.avg_turns, r.my_turns
    );
    println!(
        "Starthand 2–4 Länder: {}/{} gewonnen, andere: {}/{}",
        r.good_keeps.0, r.good_keeps.1, r.bad_keeps.0, r.bad_keeps.1
    );
    for c in &r.categories {
        println!("\n== {} ({}×)", c.title, c.items.len());
        for f in &c.items {
            println!("   Partie #{}, Zug {}: {}", f.game, f.turn, f.text);
        }
    }
    println!("\nAm meisten Schaden: {:?}", r.damage_sources);
    Ok(())
}

/// `--decks [FORMAT]`: Deck-Auswertung für alle gespielten Decks.
fn decks_cli(format: &str) -> anyhow::Result<()> {
    let db = carddb::CardDb::load(&paths::find_card_db(None)?)?;
    let col = collection::Collection::load(&paths::data_dir().join("collection.json"))
        .ok_or_else(|| anyhow::anyhow!("Keine gespeicherte Sammlung"))?;
    let owned = col.owned_by_name(&db);
    let games = games::sync(&paths::data_dir().join("games.json"), &paths::player_logs());
    let mut fetcher = meta::Fetcher::new(paths::data_dir().join("cache"));
    let scry = meta::FORMATS
        .iter()
        .find(|f| f.0 == format)
        .map(|f| f.2)
        .unwrap_or("standard");
    let legal = fetcher.legal_names(scry).ok();
    let mut lists: Vec<(u32, String, Vec<deck::Deck>)> = Vec::new();
    for arch in fetcher.archetypes(format, false)? {
        let id = arch.id;
        let decks: Vec<deck::Deck> = fetcher
            .cached_decks(format, &arch)
            .unwrap_or_default()
            .iter()
            .filter_map(|d| fetcher.cached_decklist(d.id))
            .map(|t| deck::parse(&t))
            .collect();
        lists.push((id, arch.name.clone(), decks));
    }
    let meta: Vec<deckcheck::MetaArchetype> = lists
        .iter()
        .map(|(id, name, decks)| deckcheck::MetaArchetype {
            id: *id,
            name: name.clone(),
            lists: decks.iter().collect(),
        })
        .collect();
    println!("##### Gegner");
    for o in opponents::analyze(&games, &db, &meta) {
        println!(
            "   {:<32} {} Siege, {} Niederlagen, Ø {:.0} Züge, Schaden: {:?}",
            o.name,
            o.wins,
            o.losses(),
            o.avg_turns,
            o.threats
        );
    }
    for group in deckcheck::group_decks(&games, &db) {
        let r = deckcheck::check(&group, &games, &db, &owned, legal.as_ref(), &meta);
        println!(
            "
##### {} {}",
            group.label,
            if group.fixed { "[vorgegeben]" } else { "" }
        );
        println!(
            "{} Siege aus {} Partien ({} vollständig)",
            r.wins, r.games, r.complete
        );
        println!(
            "Mana: {} Länder / {} Karten · zu wenig in {} · zu viel in {} von {} → {}",
            r.mana.lands, r.mana.size, r.mana.screw, r.mana.flood, r.mana.games, r.mana.advice
        );
        if let Some(m) = &r.meta {
            println!(
                "Turnier-Archetyp: {} ({} Listen, {:.0} % Übereinstimmung)",
                m.archetype,
                m.lists,
                m.overlap * 100.0
            );
            for (name, share, own) in m.missing.iter().take(8) {
                println!(
                    "   fehlt: {name} ({:.0} % der Listen, du hast {own})",
                    share * 100.0
                );
            }
        }
        for c in &r.cards {
            println!(
                "   {}x {:<32} gezogen {} gespielt {} zurückgehalten {} Schaden {:>3} Turnier {}",
                c.copies,
                c.name,
                c.seen,
                c.cast,
                c.held,
                c.damage,
                c.meta_share
                    .map(|m| format!("{:.0}%", m * 100.0))
                    .unwrap_or("-".into())
            );
        }
        for s in &r.swaps {
            println!("   RAUS {} – {} → {:?}", s.out, s.reason, s.replacement);
        }
    }
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("--sync-collection") => {
            let out = args
                .get(2)
                .map(|s| s.trim_matches('"').to_string())
                .unwrap_or_default();
            std::process::exit(collection::run_sync_helper(Path::new(&out)));
        }
        Some("--check") => std::process::exit(check()),
        Some("--variants") => {
            let format = args.get(2).map(String::as_str).unwrap_or("ST");
            let name = args.get(3).map(String::as_str).unwrap_or("");
            if let Err(e) = variants(format, name) {
                println!("FEHLER: {e:#}");
                std::process::exit(1);
            }
            return;
        }
        Some("--build") => {
            let format = args.get(2).map(String::as_str).unwrap_or("ST");
            if let Err(e) = build(format) {
                println!("FEHLER: {e:#}");
                std::process::exit(1);
            }
            return;
        }
        Some("--parse") => {
            // Diagnose: eine Logdatei einlesen, ohne gespeicherte Partien anzufassen.
            for path in &args[2..] {
                for g in games::parse_log(Path::new(path)) {
                    let cast: usize = g.turns.iter().map(|t| t.cast.len()).sum();
                    println!(
                        "{} {:<14} deck {:>2} züge {:>2} vollständig {} gewirkt {:>2} starthand {}",
                        g.id.get(..8).unwrap_or(""),
                        g.event.get(..14).unwrap_or(&g.event),
                        g.deck.len(),
                        g.turns.len(),
                        g.complete,
                        cast,
                        g.opening_hand.len()
                    );
                }
            }
            return;
        }
        Some("--budget") => {
            // Diagnose: Budget-Version des neuesten Turnierdecks eines Archetyps.
            let format = args.get(2).map(String::as_str).unwrap_or("ST").to_string();
            let name = args.get(3).cloned().unwrap_or_default();
            let run = || -> anyhow::Result<()> {
                let db = carddb::CardDb::load(&paths::find_card_db(None)?)?;
                let col = collection::Collection::load(&paths::data_dir().join("collection.json"))
                    .ok_or_else(|| anyhow::anyhow!("Keine gespeicherte Sammlung"))?;
                let owned = col.owned_by_name(&db);
                let mut fetcher = meta::Fetcher::new(paths::data_dir().join("cache"));
                let arch = fetcher
                    .archetypes(&format, false)?
                    .into_iter()
                    .find(|a| a.name.eq_ignore_ascii_case(&name))
                    .ok_or_else(|| anyhow::anyhow!("Archetyp nicht gefunden"))?;
                let d = fetcher
                    .decks(&format, &arch)?
                    .into_iter()
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("keine Decks"))?;
                let eval = deck::evaluate(&deck::parse(&fetcher.decklist(d.id)?), &db, &owned);
                let legal = fetcher.legal_names("standard").ok();
                let wc = col.wildcards.as_array();
                println!(
                    "Wildcards {wc:?}, ohne Ersatz nötig: {:?}",
                    eval.summary(false, |_| 0).need
                );
                let plan = budget::plan(&eval, &db, &owned, legal.as_ref(), wc, false);
                println!(
                    "hergestellt {:?}, ersetzt {}, ohne Ersatz {:?}",
                    plan.crafted, plan.replaced, plan.unresolved
                );
                for (k, r) in &plan.replacements {
                    let n = |k: &str| {
                        db.representative(k)
                            .map(|c| format!("{} ({})", c.name, carddb::rarity_label(c.rarity)))
                            .unwrap_or_default()
                    };
                    println!("   {} -> {}", n(k), n(r));
                }
                Ok(())
            };
            if let Err(e) = run() {
                println!("FEHLER: {e:#}");
            }
            return;
        }
        Some("--image") => {
            // Diagnose: ein Kartenbild über Scryfall laden.
            let (tx, rx) = std::sync::mpsc::channel();
            let req = images::spawn(
                paths::data_dir().join("cache").join("img"),
                move |name, img| {
                    let _ = tx.send((name, img.map(|i| i.size)));
                },
            );
            for name in &args[2..] {
                let _ = req.send(name.clone());
                match rx.recv_timeout(std::time::Duration::from_secs(30)) {
                    Ok((n, size)) => println!("{n}: {size:?}"),
                    Err(e) => println!("{name}: {e}"),
                }
            }
            return;
        }
        Some("--replay") => {
            // Diagnose: ein Log abspielen und die Hinweise des Begleitfensters ausgeben.
            let db = match paths::find_card_db(None).and_then(|p| carddb::CardDb::load(&p)) {
                Ok(db) => db,
                Err(e) => {
                    println!("FEHLER: {e:#}");
                    return;
                }
            };
            let Some(path) = args.get(2) else { return };
            let limit: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(60);
            let mut parser = games::LogParser::default();
            let mut last = String::new();
            let mut shown = 0;
            let text = std::fs::read(path).unwrap_or_default();
            for line in text.split(|b| *b == b'\n') {
                parser.feed(line.strip_suffix(b"\r").unwrap_or(line));
                let live = parser.live();
                let view = live::build_view(live.as_ref(), None, &db, &[]);
                let key = format!("{} | {:?}", view.status, view.hints);
                if live.is_some() && key != last {
                    println!("{key}");
                    if key.contains("Tödlich")
                        && let Some(ls) = &live
                    {
                        let show = |list: &[games::LiveCreature]| -> Vec<String> {
                            list.iter()
                                .map(|c| {
                                    format!(
                                        "{}{}{}",
                                        c.power,
                                        if c.flying { " fliegend" } else { "" },
                                        if c.menace { " bedrohlich" } else { "" }
                                    )
                                })
                                .collect()
                        };
                        println!(
                            "     Angreifer {:?} | Blocker {:?}",
                            show(&ls.attackers),
                            show(&ls.blockers)
                        );
                    }
                    last = key;
                    shown += 1;
                    if shown >= limit {
                        break;
                    }
                }
            }
            return;
        }
        Some("--decks") => {
            let format = args.get(2).map(String::as_str).unwrap_or("ST");
            if let Err(e) = decks_cli(format) {
                println!("FEHLER: {e:#}");
                std::process::exit(1);
            }
            return;
        }
        Some("--games") => {
            if let Err(e) = games_cli() {
                println!("FEHLER: {e:#}");
                std::process::exit(1);
            }
            return;
        }
        Some("--report") => {
            let format = args.get(2).map(String::as_str).unwrap_or("ST");
            if let Err(e) = report(format) {
                println!("FEHLER: {e:#}");
                std::process::exit(1);
            }
            return;
        }
        _ => {}
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
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    );
    if let Err(e) = result {
        eprintln!("Fenster konnte nicht geöffnet werden: {e}");
        std::process::exit(1);
    }
}
