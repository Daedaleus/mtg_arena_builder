//! Turnierdecks: Rangliste der Archetypen, Varianten, Deckbau, Budget-Version, magic.gg.

use super::*;

/// `--report [FORMAT]`: bewertet alle Archetypen gegen die gespeicherte Sammlung
/// und gibt eine Rangliste auf der Konsole aus.
pub fn report(format: &str) -> anyhow::Result<()> {
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
pub fn variants(format: &str, name: &str) -> anyhow::Result<()> {
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
pub fn build(format: &str) -> anyhow::Result<()> {
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
                    .unwrap_or_default()
            );
        }
    }
    Ok(())
}

/// `--budget FORMAT NAME`: Budget-Version und Wildcard-Plan des neuesten Decks eines Archetyps.
pub fn budget(format: &str, name: &str) -> anyhow::Result<()> {
    let db = carddb::CardDb::load(&paths::find_card_db(None)?)?;
    let col = collection::Collection::load(&paths::data_dir().join("collection.json"))
        .ok_or_else(|| anyhow::anyhow!("Keine gespeicherte Sammlung"))?;
    let owned = col.owned_by_name(&db);
    let mut fetcher = meta::Fetcher::new(paths::data_dir().join("cache"));
    let arch = fetcher
        .archetypes(format, false)?
        .into_iter()
        .find(|a| a.name.eq_ignore_ascii_case(name))
        .ok_or_else(|| anyhow::anyhow!("Archetyp nicht gefunden"))?;
    let d = fetcher
        .decks(format, &arch)?
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
    let wp = budget::wildcard_plan(&eval, &db, wc, false);
    println!("Wildcard-Plan ({:.0} % vorhanden):", wp.complete);
    for s in &wp.now {
        println!("   jetzt  {}× {} -> {:.0} %", s.qty, s.name, s.complete);
    }
    for s in &wp.later {
        println!(
            "   später {}× {} (fehlen dann {:?}) -> {:.0} %",
            s.qty, s.name, s.short, s.complete
        );
    }
    Ok(())
}

/// `--gg FORMAT`: Events und Decks von magic.gg.
pub fn gg(code: &str) -> anyhow::Result<()> {
    let fmt = meta::FORMATS
        .iter()
        .find(|f| f.0 == code)
        .ok_or_else(|| anyhow!("Unbekanntes Format {code}"))?;
    let db = carddb::CardDb::load(&paths::find_card_db(None)?)?;
    let mut fetcher = meta::Fetcher::new(paths::data_dir().join("cache"));
    let events = fetcher.gg_events(fmt.2, false)?;
    for e in events.iter().take(3) {
        let decks = fetcher.gg_decks(&e.slug)?;
        println!("##### {} ({}) – {} Decks", e.title, e.date, decks.len());
        for d in decks.iter().take(5) {
            let label = opponents::deck_label(&d.deck, &db, &[]);
            println!("   {:<40} {}", d.title, label);
        }
    }
    println!("{} Events insgesamt", events.len());
    Ok(())
}
