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

- [ ] **Decklisten von magic.gg** (https://magic.gg/decklists): offizielle
      Listen von Championships, Pro Tours, Regional Championships und Arena-
      Events als zweite Quelle neben MTGTop8 einbinden – erst Seitenaufbau,
      Nutzungsbedingungen und robots.txt prüfen.
- [ ] Probehände mit Gegner-Druck: wie viel Schaden bis Zug 6 (Aggro-Check).
- [ ] Wildcard-Plan: geschätzte Spielzeit/Packs bis zum nächsten Schritt.

- [x] Probehände: Deck 1000-mal bis Zug 6 simulieren (`sim.rs`).
- [x] Wildcard-Plan in der Deckansicht (`budget::wildcard_plan`).
- [x] Schlüsselmomente erscheinen auch in der Detailansicht einer Partie.
- [x] Rangverlauf auch für Limited.
- [x] Zugvorschlag: Mana aus Kreaturen. Offen: Schätze und andere Token
      (stehen nicht in der Kartendatenbank).
- [x] Blockvorschlag: Bedrohlich mit zwei Blockern.
- [ ] Spieler-Typ auswerten - Dahingehend Decks empfehlen
- [ ] UI Ändern: Meine Partien: Liste Paginieren (10)

## Technik

- [x] Sicherung: privates Repository `Daedaleus/mtg_checker` auf GitHub.
- [x] CI mit GitHub Actions: fmt, Clippy, Tests und Release-Build auf
      Windows, Linux und macOS, Testabdeckung ≥ 90 %.
- [ ] Linux- und macOS-Version tatsächlich starten und ausprobieren (bauen und
      Tests laufen in der CI; Speicherlesen braucht dort root).
- [ ] Releases: bei einem Versions-Tag die CI-Artefakte als GitHub-Release
      veröffentlichen.
- [x] Oberfläche in Module je Bereich aufgeteilt (`src/app/`).
- [ ] **Refaktorierung großer Dateien:** `games.rs` (~1500 Zeilen), `main.rs`
      (Kommandozeile ~700), `live.rs`, `carddb.rs`, `deckcheck.rs` und die großen
      Oberflächen-Module (`games_tab.rs`, `app/mod.rs`) in kleinere, nach
      Aufgabe benannte Module aufteilen, damit man sich zurechtfindet.
      Verhalten unverändert; Tests und Abdeckung ≥ 90 % bleiben.
