# Hinweise für die Arbeit an diesem Projekt

Überblick über Funktionen, Bauen und Daten: [README.md](README.md).
Offene Punkte: [ToDo.md](ToDo.md).

## Sprache und Stil

- Kommentare, Doku und Commit-Nachrichten auf **Deutsch**.
- Oberfläche zweisprachig (Deutsch, Englisch): Anzeigetexte stehen **nur** in
  `locales/app.yml` und kommen per `tr!("modul.schluessel", name = wert)` in
  den Code – keine Anzeigetexte als Zeichenketten im Code. Neue Texte immer in
  beiden Sprachen; die Tests `every_key_has_all_languages_and_same_placeholders`
  und `every_used_key_exists` prüfen das. Tests laufen auf Deutsch.
- Texte kurz und für Gelegenheitsspieler verständlich; Arena-Begriffe so,
  wie die jeweilige Arena-Version sie nennt.
- Kartennamen: intern immer englisch (`Card::name`, Schlüssel, Arena-Import),
  angezeigt mit `Card::display_name()` bzw. `CardDb::display_name()` in der
  eingestellten Sprache. Kartenbilder je Sprache über `images::key`.
- Werte aus dem Arena-Log nie als lose Zeichenketten: `games::arena`
  (`Phase`, Zonen, Nachrichten, Events).
- Die eingebaute egui-Schrift kennt keine Pfeile (`→`, `≥`): in der Oberfläche
  „, dann“, „>“ oder Worte verwenden.

## Aufbau

Bibliothek (`src/lib.rs`) und Programm (`src/main.rs` startet nur; die
Diagnose-Kommandozeile liegt in `src/cli/`). `src/app/` enthält nur Oberfläche
und Verdrahtung – Logik gehört in die Fachmodule, damit sie testbar bleibt.
Nichts Teures pro Bild berechnen: Auswertungen über `app/compute.rs`
zwischenspeichern (Generationszähler `data_gen`), lange Rechnungen
(Probehände, Deckbau) als Hintergrund-Thread mit `Msg` zurück. Zustand der
Oberfläche nach Bereich in `app/parts.rs` (Turnierdecks, Partien, magic.gg,
Bilder, Probehände, gebaute Decks). egui nur in `src/app/` und `main.rs` – Fachmodule
liefern eigene Typen (z. B. `images::Picture`) und nehmen Callbacks.
Netzabruf über den Trait `http::Http` (in Tests `testutil::FakeHttp`), Fehler
mit `anyhow`;
Deck-Kennungen nur über `app::deck_ids`.

Große Module sind Ordner mit Teilmodulen nach Aufgabe; `mod.rs` beschreibt
oben den Aufbau und exportiert die öffentlichen Teile, Pfade wie
`crate::games::report` bleiben dadurch gleich. Faustregel: Dateien unter
etwa 400 Zeilen, Funktionen unter etwa 100 Zeilen halten, sonst nach Aufgabe
teilen. Fachbegriffe (Farben, Schlüsselwörter, Lebenspunkte, Fehlerarten)
einmal definieren und nicht über Anzeigetexte vergleichen – dafür gibt es
Aufzählungen wie `Mistake` oder `LandVerdict`.

| Modul | Aufgabe |
|---|---|
| `carddb/` | Arena-Kartendatenbank: `load` (SQLite), `text` (Rollen, Brand, Mana aus Kartentext); Fachbegriffe für alle: `color` (Farben, Standardländer), `keyword` (Schlüsselwörter wie Fliegend), `Rarity`, `GrpId`, `CardKey` |
| `collection` | Sammlung aus dem Arena-Speicher (mtga-reader, Adminrechte) |
| `meta`, `magicgg` | MTGTop8 und magic.gg (Abruf mit Cache, Parser), Scryfall-Legalität |
| `deck`, `similar`, `budget` | Listen prüfen, Ersatzkarten, Budget-Version, Wildcard-Plan |
| `sim/` | Probehände (Deck 1000-mal durchspielen): `game` (eine Partie, Summen), `pressure` (Spielfeld, Kampf, Rennen), `profile` (Gegner und Gewohnheiten aus den eigenen Partien), `advice` |
| `style` | Spielstil (Aggro/Midrange/Control) aus Decks und Partien, Deckempfehlung |
| `builder` | Decks nur aus eigenen Karten |
| `games/` | Mitschreiben: `arena` (Protokollwerte), `object`, `tracker`, `live_state`, `parser`; Speichern: `record` (Datentypen, Rang), `store` (`games.json`); Auswerten: `moments` (einzelne Züge), `mistake` (Fehlerarten), `report` |
| `deckcheck/`, `opponents`, `progress` | Deck verbessern (`groups`, `mana`, `analysis`), Gegner und Archetyp-Erkennung, Verlauf |
| `live/`, `turnplan`, `combat` | Begleitfenster-Logik (`view`, `hints`, `damage`, `follow`; gezeichnet in `app/companion`), Zugvorschlag, Blocks/Tricks/ungeblockter Schaden |
| `settings`, `present`, `paths`, `images`, `http` | Einstellungen (Format als Kürzel gespeichert), Anzeigetexte, Pfade, Bild-Cache (Kartenbilder je Sprache), Netzabruf |
| `i18n` | Sprache (Betriebssystem oder Einstellung), Makro `tr!`; Texte in `locales/app.yml` |
| `testutil/` | Testhilfen: `cards`, `log` (künstliche Lognachrichten), `pages` (Webseiten) |

## Regeln

- Vor jedem Commit: `cargo fmt`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test`. `build.ps1` erledigt das und baut die Release-Version.
- **Tests:** neue Logik bekommt Tests in `src/tests/<modul>.rs` (eingebunden
  per `#[cfg(test)] #[path = "tests/<modul>.rs"] mod tests;`). Testdaten
  (Karten, Lognachrichten, temporäre Ordner) aus `src/testutil.rs`.
  `coverage.ps1` verlangt mindestens 90 % Zeilenabdeckung ohne `src/app/` und `main.rs`.
- Tests dürfen weder Netzwerk noch das echte Datenverzeichnis anfassen. Netz in
  Tests über `testutil::FakeHttp` (mit echten, gekürzten Antworten aus
  `testutil/fixtures/`), Wartezeiten über `testutil::FakeClock`. Einzige
  Ausnahme: der Vertragstest `contract_with_real_scryfall` (`#[ignore]`, von
  Hand mit `cargo test -- --ignored`).
- Netzabruf in der App immer über den gemeinsamen `http::shared()` – er hält
  die Mindestabstände je Seite (`http::GAPS`) über alle Threads ein.
- Der Test `texts_have_no_broken_line_continuations` findet Texte, bei denen
  beim Bearbeiten das `\` am Zeilenende verloren ging (lange Leerzeichenfolgen).
- Ändert sich, was der Log-Parser pro Partie speichert, `PARSER_VERSION` in
  `games/store.rs` erhöhen – dann werden gespeicherte Partien aus noch vorhandenen
  Logs neu berechnet. Bessere Aufzeichnungen ersetzen schlechtere, nie
  umgekehrt (`quality`).
- MTGTop8: höchstens eine Anfrage alle 5 Sekunden (robots.txt), alles cachen;
  Scryfall 100 ms, magic.gg 2 s – eingestellt in `http::GAPS`.
- Commits und Pushes nur auf Wunsch des Nutzers. Remote `origin` ist das
  private Repository `Daedaleus/mtg_checker` auf GitHub, Branch `main`.
  GitHub CLI: `C:\Program Files\GitHub CLI\gh.exe` (angemeldet).

## Fallstricke

- `build.ps1` beendet die laufende App und startet sie neu. Läuft gerade eine
  Arena-Partie, vorher fragen – das Begleitfenster ist dann kurz weg.
- Die Release-.exe hat kein Konsolenfenster und öffnet die Oberfläche: nie
  ohne Zeitlimit aus einem Skript starten. Für Konsolenausgaben die
  Debug-Version (`target\debug\...`) mit `--games`, `--replay` usw. nutzen.
- Beim Ändern von Rust-Code über Python-/Shell-Skripte werden `\n`, `\r` und
  `\'` in Byte- und Zeichenliteralen leicht zu echten Zeichen – Ergebnis prüfen
  oder das Edit-Werkzeug nehmen.
- PowerShell-Skripte (`build.ps1`, `coverage.ps1`) als UTF-8 mit BOM und CRLF
  speichern, sonst zeigt Windows PowerShell 5.1 Umlaute falsch an.
- `settings.json` von Hand nur ohne BOM schreiben (gelesen wird beides).
- Nutzerdaten (`games.json`, `settings.json`) mit `paths::load_json` laden und
  `LoadError` unterscheiden: beschädigt = sichern (`backup_corrupt`), nicht
  lesbar = nicht überschreiben. `read_json` nur für Caches.
- Arena-Log: Sitz des Spielers aus `systemSeatIds`, nicht fest Sitz 1. Nach
  Spielende kommen noch Nachrichten ohne Partie-Kennung (siehe Test
  `next_game_after_leftover_message`).
