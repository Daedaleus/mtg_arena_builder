//! Setup and diagnostics: card data, collection, card images.

use super::*;

/// `--check`: diagnostics on the console (card data, collection, meta fetch).
pub fn check() -> i32 {
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
    println!("{} Karten geladen (Arena {})", db.card_count(), db.version);

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
                "Sammlung nicht lesbar (Admin: {}): {}",
                collection::is_admin(),
                e.user_text()
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
                l.rarity.label(),
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

/// `--sync-collection FILE`: helper mode with administrator rights.
pub fn sync_collection(out: &str) -> i32 {
    collection::run_sync_helper(Path::new(out.trim_matches('"')))
}

/// `--image NAME…`: load card images via Scryfall.
pub fn image(names: &[String]) {
    let (tx, rx) = std::sync::mpsc::channel();
    let req = images::spawn(
        paths::data_dir().join("cache").join("img"),
        mtg_arena_builder::http::shared(),
        move |name, img| {
            let size = match img {
                images::Fetched::Found(p) => Some(p.size),
                _ => None,
            };
            let _ = tx.send((name, size));
        },
    );
    for name in names {
        let _ = req.send(name.clone());
        match rx.recv_timeout(std::time::Duration::from_secs(30)) {
            Ok((n, size)) => println!("{n}: {size:?}"),
            Err(e) => println!("{name}: {e}"),
        }
    }
}
