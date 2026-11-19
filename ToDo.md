# ToDo

Stand: 19.11.2026. Erledigtes steht in der Git-Historie.

## Ideen

- [x] Decklisten von magic.gg: Reiter „Offizielle Events“ (`magicgg.rs`).
- [x] Probehände mit Gegner-Druck: schneller Gegner bis Zug 6 (`sim/pressure.rs`).
- [x] Gegner-Druck mit Rennen: eigene Angriffe, Brand ins Gesicht, „vorne nach Zug 6“.
- [x] Persönliche Probehände: Gegner aus den eigenen Partien, Vergleich
      „mit Zugvorschlag“ und „so wie du spielst“ (`sim/profile.rs`).
- [x] Probehände gegen die eigenen Gegner bis Zug 10 (Stresstest weiter bis Zug 6).
- [x] Gegner in der Simulation mit Entfernung und Kampftricks – so oft, wie deine
      Gegner sie gespielt haben (Kirk jetzt 38 % vorne, real 5 von 12 gewonnen).
- [ ] Rennen: Prowess, Ausweichfähigkeiten des Gegners und Tricks fehlen noch –
      Spells-Decks (z. B. Slickshot) schneiden schlechter ab, als sie sind.
- [ ] Wildcard-Plan: geschätzte Spielzeit/Packs bis zum nächsten Schritt.

- [x] Probehände: Deck 1000-mal bis Zug 6 simulieren (`sim.rs`).
- [x] Wildcard-Plan in der Deckansicht (`budget::wildcard_plan`).
- [x] Schlüsselmomente erscheinen auch in der Detailansicht einer Partie.
- [x] Rangverlauf auch für Limited.
- [x] Zugvorschlag: Mana aus Kreaturen. Offen: Schätze und andere Token
      (stehen nicht in der Kartendatenbank).
- [x] Blockvorschlag: Bedrohlich mit zwei Blockern.
- [x] Spielstil auswerten und passende Turnierdecks empfehlen (`style.rs`).
- [x] „Meine Partien“: Partienliste mit 10 Partien je Seite.

## Technik

- [x] Sicherung: privates Repository `Daedaleus/mtg_checker` auf GitHub.
- [x] CI mit GitHub Actions: fmt, Clippy, Tests und Release-Build auf
      Windows, Linux und macOS, Testabdeckung ≥ 90 %.
- [ ] Linux-Version irgendwann starten und ausprobieren (bauen und Tests
      laufen in der CI; Speicherlesen braucht dort root). macOS nur bei
      Gelegenheit.
- [x] Releases: bei einem Versions-Tag die CI-Artefakte als GitHub-Release
      veröffentlichen.
- [x] Oberfläche in Module je Bereich aufgeteilt (`src/app/`).
- [x] Große Dateien in Teilmodule aufgeteilt (`games/`, `live/`, `carddb/`,
      `deckcheck/`, `cli/`, `testutil/`, Oberfläche je Bereich).
- [x] Refactoring: Fachbegriffe gebündelt (`carddb::color`, `carddb::keyword`,
      `Mistake`, `LandVerdict`), lange Funktionen in Schritte geteilt
      (Auswertung, Simulation, Deckbau, Deckprüfung, Begleiter, Parser,
      Spielstil, Oberfläche); Spielobjekte in fester Reihenfolge.
- [x] Zweisprachig (Deutsch/Englisch): Texte in `locales/app.yml`, Sprache aus
      dem Betriebssystem oder den Einstellungen, Kartennamen und -bilder je Sprache.
- [x] Architektur-Review (Note 2-): Status „Alle bewertet“ ohne Textvergleich,
      Auswertungen zwischengespeichert, Probehände und Deckbau im Hintergrund.
- [x] Review umgesetzt: egui nur noch in der Oberfläche, Logik aus `app/state.rs`
      in Fachmodule, `Rarity` als Aufzählung, Deck-Kennungen gekapselt,
      `GrpId`/`CardKey` benannt, Versionsnummer in `settings.json`,
      Parser-Version je Partie, Schreibfehler gemeldet, `App` in Teilzustände,
      Live-Thread beendbar, `anyhow` statt `String`-Fehlern, Netzabruf hinter
      `meta::Http`.
- [x] Review-Kleinigkeiten: Turnier-Präsenz in `deck::usage_counts`, Format in
      `settings.json` als Kürzel, Kartenbilder über `http::Http` (Netzfehler
      gelten nicht mehr als „Bild fehlt“), Aufgabegrund als `EndReason`,
      Turnierdeck-Felder als `TourState`.
- [x] Rest aus dem Review: `GrpId` als eigener Typ (Arena-Kartennummer kann
      nicht mehr mit Objekt-, Sitz- oder Zonennummern verwechselt werden;
      in `games.json` unverändert als Zahl); `CardKey` bleibt bewusst ein
      Alias (Begründung in `carddb`); ein Aufgaben-Thread statt eines
      Threads je Aktion (`app/tasks.rs`).
- [x] Zweites Review (Note 2), Schritte 1–2: beschädigte `games.json`/
      `settings.json` werden gesichert statt überschrieben (`LoadError`,
      thiserror), nicht lesbare nicht überschrieben; Probehände verfallen bei
      neuer Liste; Legalität und Kartenbilder werden nach Netzfehlern erneut
      angefragt; Fortschrittsmeldungen lösen keine Neuberechnung aus.
- [x] Zweites Review, Schritt 3: gemeinsamer Netzabruf `http::shared()` mit
      Mindestabständen je Seite (`Polite`, einschleusbare Uhr), echte
      Scryfall-Antworten als Testdaten, Vertragstest `#[ignore]`; Testsuite
      wieder ca. 0,2 s.
- [x] Zweites Review, Schritt 4: `CollectionError` (UAC-Abfrage nur bei
      verweigertem Zugriff oder unklarem Grund, Speicherfehler gemeldet,
      Übersetzung beim Anzeigen) und `http::FetchError` (404 / Status /
      Netz) mit thiserror.
- [x] Netz: nach HTTP 429 sperrt `http::Polite` die Seite für `Retry-After`
      (sonst 60 s) für alle Threads; Scryfall-Abstand 500 ms.
- [x] Zweites Review, Schritt 5: `meta::Format` statt Index, `app::DeckId` statt
      Zahlenraum, lange Funktionen geteilt (keine über 100 Zeilen mehr),
      „Format:“ übersetzt, Begleitfenster liest nach dem Neuladen der
      Kartendaten mit der neuen Datenbank.
