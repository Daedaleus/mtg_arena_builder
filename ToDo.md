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

- [ ] **Probehände testen:** tausende Starthände und erste Züge eines Decks
      simulieren und zeigen, wie oft Länder oder Farben fehlen.
- [ ] **Wildcard-Plan:** Ziel-Deck in Etappen – welche Karte zuerst herstellen.
- [ ] Schlüsselmomente auch in der Detailansicht einer Partie (je Zug) zeigen.
- [ ] Rangverlauf auch für Limited (`limitedLevel` steht schon im Log).
- [ ] Zugvorschlag: Mana aus Kreaturen und Schätzen berücksichtigen.
- [ ] Blockvorschlag: Bedrohlich mit zwei Blockern einplanen.

## Technik

- [x] Sicherung: privates Repository `Daedaleus/mtg_checker` auf GitHub.
- [x] CI mit GitHub Actions: fmt, Clippy, Tests und Release-Build auf
      Windows, Linux und macOS, Testabdeckung ≥ 90 %.
- [ ] Linux- und macOS-Version tatsächlich starten und ausprobieren (bauen und
      Tests laufen in der CI; Speicherlesen braucht dort root).
- [ ] Releases: bei einem Versions-Tag die CI-Artefakte als GitHub-Release
      veröffentlichen.
- [x] Oberfläche in Module je Bereich aufgeteilt (`src/app/`).
