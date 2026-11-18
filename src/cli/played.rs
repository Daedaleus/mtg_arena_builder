//! Gespielte Partien: Auswertung, Decks, Logdateien, Begleitfenster-Nachspielen, Probehände.

use super::*;

/// `--games`: Partien aus den Arena-Logs einlesen und auswerten.
pub fn games_cli() -> anyhow::Result<()> {
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
        println!("\n== {} ({}×)", c.title(), c.items.len());
        for f in &c.items {
            println!("   Partie #{}, Zug {}: {}", f.game, f.turn, f.text);
        }
    }
    println!("\nAm meisten Schaden: {:?}", r.damage_sources);
    Ok(())
}

/// `--decks [FORMAT]`: Deck-Auswertung für alle gespielten Decks.
pub fn decks_cli(format: &str) -> anyhow::Result<()> {
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
    for o in opponents::analyze(&games, &db, &opponents::arch_shares(&meta, &db)) {
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
            r.mana.lands,
            r.mana.size,
            r.mana.screw,
            r.mana.flood,
            r.mana.games,
            r.mana.verdict.advice()
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

/// `--parse LOG…`: Logdateien einlesen, ohne gespeicherte Partien anzufassen.
pub fn parse(paths: &[String]) {
    for path in paths {
        for g in games::parse_log(Path::new(path)) {
            let cast: usize = g.turns.iter().map(|t| t.cast.len()).sum();
            println!(
                "{} {:<14} deck {:>2} züge {:>2} vollständig {} gewirkt {:>2} starthand {} rang {}",
                g.id.get(..8).unwrap_or(""),
                g.event.get(..14).unwrap_or(&g.event),
                g.deck.len(),
                g.turns.len(),
                g.complete,
                cast,
                g.opening_hand.len(),
                g.rank_after
                    .as_ref()
                    .map(progress::rank_label)
                    .unwrap_or_else(|| "–".into())
            );
        }
    }
}

/// `--replay LOG [N]`: ein Log abspielen und die Hinweise des Begleitfensters ausgeben.
pub fn replay(path: &str, limit: usize) -> anyhow::Result<()> {
    let db = carddb::CardDb::load(&paths::find_card_db(None)?)?;
    let mut parser = games::LogParser::default();
    let mut last = String::new();
    let mut shown = 0;
    let text = std::fs::read(path).with_context(|| format!("{path} nicht lesbar"))?;
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
    Ok(())
}

/// `--sim`: Probehände für alle gespielten Decks.
pub fn sim() -> anyhow::Result<()> {
    let db = carddb::CardDb::load(&paths::find_card_db(None)?)?;
    let games = games::sync(&paths::data_dir().join("games.json"), &paths::player_logs());
    let opponent = sim::opponent_from(&games, &db);
    let habits = sim::habits_from(&games, &db);
    match &opponent {
        Some(o) => println!(
            "Gegnerprofil aus {} Partien: {} · neue Kraft je Zug {:.1?} · Entfernung {:.0} % der Züge · Tricks {:.0} %",
            o.games,
            o.label,
            o.power,
            o.removal * 100.0,
            o.tricks * 100.0
        ),
        None => println!("Zu wenige Partien für ein Gegnerprofil."),
    }
    if let Some(h) = &habits {
        println!(
            "Gewohnheiten aus {} Zügen: besserer Zug möglich in {:.0} % · gefahrlosen Angriff ausgelassen in {:.0} %",
            h.turns,
            h.weaker_turn * 100.0,
            h.skip_attack * 100.0
        );
    }
    for g in deckcheck::group_decks(&games, &db) {
        let start = std::time::Instant::now();
        let r = sim::simulate(&g.deck, &db, sim::DEFAULT_GAMES, 1);
        println!("##### {} ({} ms)", g.label, start.elapsed().as_millis());
        if let Some(o) = &opponent {
            let best = sim::simulate_with(&g.deck, &db, sim::DEFAULT_GAMES, 1, o, None);
            let yours = sim::simulate_with(&g.deck, &db, sim::DEFAULT_GAMES, 1, o, habits.as_ref());
            println!(
                "   gegen deine Gegner – mit Zugvorschlag: vorne {:.0} %, verloren {:.0} % · so wie du spielst: vorne {:.0} %, verloren {:.0} %",
                best.pressure.ahead * 100.0,
                best.pressure.dead * 100.0,
                yours.pressure.ahead * 100.0,
                yours.pressure.dead * 100.0
            );
        }
        println!(
            "   Land rechtzeitig Z1–4 {:?} · Mulligan {:.2} · Farbe fehlte {:.2} · Flut {:.2}",
            r.on_curve, r.mulligan, r.color_screw, r.flood
        );
        println!(
            "   Gegner-Druck: Leben {:.1?} · Gegner {:.1?} · vorne {:.2} · gewonnen {:.2} · verloren {:.2} · ≤5 Leben {:.2} · ausgeschaltet {:.1}",
            r.pressure.life,
            r.pressure.opp_life,
            r.pressure.ahead,
            r.pressure.won,
            r.pressure.dead,
            r.pressure.low,
            r.pressure.kills
        );
        for a in &r.advice {
            println!("   {a}");
        }
    }
    Ok(())
}

/// `--style`: Spielstil und Einteilung aller zwischengespeicherten Turnierlisten.
pub fn style_cli() -> anyhow::Result<()> {
    let db = carddb::CardDb::load(&paths::find_card_db(None)?)?;
    let games = games::sync(&paths::data_dir().join("games.json"), &paths::player_logs());
    let report = games::report(&games, &db);
    let s = mtg_arena_builder::style::analyze(&games, &db, &report);
    for r in &s.records {
        println!("{}: {} Partien, {} Siege", r.style.name(), r.games, r.wins);
    }
    for t in &s.traits {
        println!("  {t}");
    }
    println!("Empfehlung: {} – {}", s.recommended.name(), s.reason);
    // Zwischengespeicherte Listen einteilen
    let cache = paths::data_dir().join("cache");
    let mut counts = std::collections::HashMap::new();
    for entry in std::fs::read_dir(&cache)?.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let text = std::fs::read_to_string(entry.path()).unwrap_or_default();
        let decks: Vec<deck::Deck> = if name.starts_with("deck_") {
            vec![deck::parse(&text)]
        } else if name.starts_with("gg_") && name != "gg_index.html" {
            mtg_arena_builder::magicgg::parse_event(&text)
                .into_iter()
                .map(|d| d.deck)
                .collect()
        } else {
            continue;
        };
        for d in decks {
            let p = mtg_arena_builder::style::DeckProfile::of_deck(&d, &db);
            *counts.entry(p.style().name()).or_insert(0) += 1;
        }
    }
    println!("Zwischengespeicherte Turnierlisten: {counts:?}");
    Ok(())
}
