use super::*;

#[test]
fn locale_from_system_tag() {
    assert_eq!(Locale::from_tag("de-DE"), Locale::German);
    assert_eq!(Locale::from_tag("de_AT.UTF-8"), Locale::German);
    assert_eq!(Locale::from_tag("DE"), Locale::German);
    assert_eq!(Locale::from_tag("en-US"), Locale::English);
    assert_eq!(Locale::from_tag("fr-FR"), Locale::English, "sonst Englisch");
    assert_eq!(Locale::from_tag(""), Locale::English);
    // Die Systemsprache ist eine der beiden.
    assert!(Locale::ALL.contains(&Locale::system()));
    assert_eq!(Language::Fixed(Locale::English).resolve(), Locale::English);
    assert!(Locale::ALL.contains(&Language::System.resolve()));
}

#[test]
fn settings_roundtrip() {
    for lang in [
        Language::System,
        Language::Fixed(Locale::German),
        Language::Fixed(Locale::English),
    ] {
        let json = serde_json::to_string(&lang).unwrap();
        assert_eq!(serde_json::from_str::<Language>(&json).unwrap(), lang);
    }
    assert_eq!(
        serde_json::to_string(&Language::Fixed(Locale::English)).unwrap(),
        r#"{"Fixed":"en"}"#
    );
    assert_eq!(Locale::German.native_name(), "Deutsch");
    assert_eq!(Locale::English.native_name(), "English");
}

#[test]
fn texts_in_both_languages() {
    // Standard in Tests: Deutsch.
    assert_eq!(current(), Locale::German);
    assert_eq!(tr!("game_list.win"), "Sieg");
    assert_eq!(
        rust_i18n::t!("game_list.win", locale = "en"),
        "Win",
        "englischer Text"
    );
    assert_eq!(
        rust_i18n::t!(
            "games_overview.wins_losses",
            locale = "en",
            wins = 3,
            losses = 2
        ),
        "3 wins, 2 losses"
    );
    assert_eq!(
        tr!("games_overview.wins_losses", wins = 3, losses = 2),
        "3 Siege, 2 Niederlagen"
    );
    assert_eq!(
        tr!("deck_check.most_similar_loaded_tournament", archetype = "X", lists = 2, v = 12.6 : {:.0}),
        "Am ähnlichsten: X (2 geladene Turnierlisten, 13 % Übereinstimmung der Nichtland-Karten)"
    );
}

/// Schlüssel aus `locales/app.yml` mit ihren Sprachen.
fn locale_file() -> Vec<(String, Vec<(String, String)>)> {
    let text =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/locales/app.yml")).unwrap();
    let mut keys: Vec<(String, Vec<(String, String)>)> = Vec::new();
    for line in text.lines() {
        if line.starts_with('#') || line.starts_with("_version") || line.trim().is_empty() {
            continue;
        }
        if let Some(key) = line.strip_suffix(':').filter(|_| !line.starts_with(' ')) {
            keys.push((key.to_string(), Vec::new()));
        } else if let Some((lang, value)) = line.trim().split_once(": ") {
            keys.last_mut()
                .unwrap()
                .1
                .push((lang.to_string(), value.to_string()));
        }
    }
    keys
}

#[test]
fn every_key_has_all_languages_and_same_placeholders() {
    let keys = locale_file();
    assert!(keys.len() > 400, "{}", keys.len());
    fn placeholders(s: &str) -> Vec<&str> {
        let mut p: Vec<&str> = s
            .match_indices("%{")
            .map(|(i, _)| &s[i..i + s[i..].find('}').unwrap()])
            .collect();
        p.sort_unstable();
        p
    }
    for (key, langs) in &keys {
        let codes: Vec<&str> = langs.iter().map(|(l, _)| l.as_str()).collect();
        assert_eq!(codes, ["de", "en"], "{key}");
        assert_eq!(
            placeholders(&langs[0].1),
            placeholders(&langs[1].1),
            "{key}"
        );
    }
}

#[test]
fn every_used_key_exists() {
    let known: std::collections::HashSet<String> =
        locale_file().into_iter().map(|(k, _)| k).collect();
    let mut missing = Vec::new();
    let mut stack = vec![std::path::PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src"
    ))];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            for (i, _) in text.match_indices("tr!(\"") {
                // Nur das Makro `tr!`, nicht z. B. `include_str!`.
                if text[..i].ends_with(|c: char| c.is_alphanumeric() || c == '_') {
                    continue;
                }
                let rest = &text[i + 5..];
                let key = &rest[..rest.find('"').unwrap()];
                if !known.contains(key) {
                    missing.push(format!("{}: {key}", path.display()));
                }
            }
        }
    }
    assert!(missing.is_empty(), "{missing:#?}");
}
