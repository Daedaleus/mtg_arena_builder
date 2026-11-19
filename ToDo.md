# ToDo

Stand: 16.11.2026. Erledigtes steht in der Git-Historie.

## Prüfen in echten Partien

- [ ] Blockvorschlag und Trick-Warnung im Begleitfenster während echter
      Partien beobachten (bisher nur mit Tests und alten Logs geprüft).
- [ ] Trick-Warnung mit geladenen Turnierlisten („Anteil in Turnierlisten“).
- [ ] Rangverlauf: steigt der Rang, ob die Klasse (Silber, Gold …) im Log
      wirklich als `constructedClass` steht – bisher nur Bronze gesehen, wo
      Arena das Feld weglässt.
- [ ] „Besserer Zug möglich“: Fehlalarme sammeln (z. B. Länder, die in dem Zug
      getappt ins Spiel kamen, oder Mana aus Kreaturen).

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
- [ ] Linux- und macOS-Version tatsächlich starten und ausprobieren (bauen und
      Tests laufen in der CI; Speicherlesen braucht dort root).
- [ ] Releases: bei einem Versions-Tag die CI-Artefakte als GitHub-Release
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
- [ ] Rest aus dem Review: `GrpId`/`CardKey` als echte Newtypes statt Aliasen,
      `DeckSource`-Enum statt gemeinsamem `u32`-Raum (braucht Umstellung von
      `settings.json`), lange Oberflächen-Funktionen (`handle_messages`,
      `top_bar`, `game_list`) weiter teilen.
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
- [ ] Netz: bei HTTP 429 (Scryfall drosselt) mit `Retry-After` warten statt
      gleich aufzugeben.
- [ ] Zweites Review, Schritt 5: `enum Format`, `DeckSource`, lange
      Oberflächen-Funktionen teilen; Kleinigkeiten (`"Format:"` übersetzen,
      Live-Thread mit neuer Kartendatenbank).
