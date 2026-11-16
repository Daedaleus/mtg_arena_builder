# Hinweise für die Arbeit an diesem Projekt

Überblick über Funktionen, Bauen und Daten: [README.md](README.md).
Offene Punkte: [ToDo.md](ToDo.md).

## Sprache und Stil

- Oberfläche, Kommentare, Doku und Commit-Nachrichten auf **Deutsch**.
- Texte kurz und für Gelegenheitsspieler verständlich; Arena-Begriffe so,
  wie die deutsche Arena-Version sie nennt, Kartennamen englisch.
- Die eingebaute egui-Schrift kennt keine Pfeile (`→`, `≥`): in der Oberfläche
  „, dann“, „>“ oder Worte verwenden.

## Aufbau

Bibliothek (`src/lib.rs`) mit dünnem Programm (`src/main.rs`: Start und
Diagnose-Kommandozeile). `app.rs` enthält nur Oberfläche und Verdrahtung –
Logik gehört in die Fachmodule, damit sie testbar bleibt.

| Modul | Aufgabe |
|---|---|
| `carddb` | Arena-Kartendatenbank (SQLite), Rollen aus Kartentext |
| `collection` | Sammlung aus dem Arena-Speicher (mtga-reader, Adminrechte) |
| `meta` | MTGTop8-Abruf und -Parser, Scryfall-Legalität |
| `deck`, `similar`, `budget` | Listen prüfen, Ersatzkarten, Budget-Version |
| `builder` | Decks nur aus eigenen Karten |
| `games` | Log-Parser (`LogParser`, `Tracker`), Partie-Auswertung, Rang |
| `deckcheck`, `opponents`, `progress` | Deck verbessern, Gegner, Verlauf |
| `live`, `turnplan`, `combat` | Begleitfenster: Mitlesen, Zugvorschlag, Blocks/Tricks |
| `settings`, `present`, `paths`, `images` | Einstellungen, Anzeigetexte, Pfade, Bild-Cache |

## Regeln

- Vor jedem Commit: `cargo fmt`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test`. `build.ps1` erledigt das und baut die Release-Version.
- **Tests:** neue Logik bekommt Tests in `src/tests/<modul>.rs` (eingebunden
  per `#[cfg(test)] #[path = "tests/<modul>.rs"] mod tests;`). Testdaten
  (Karten, Lognachrichten, temporäre Ordner) aus `src/testutil.rs`.
  `coverage.ps1` verlangt mindestens 90 % Zeilenabdeckung ohne `app.rs`/`main.rs`.
- Tests dürfen weder Netzwerk noch das echte Datenverzeichnis anfassen.
- Ändert sich, was der Log-Parser pro Partie speichert, `PARSER_VERSION` in
  `games.rs` erhöhen – dann werden gespeicherte Partien aus noch vorhandenen
  Logs neu berechnet. Bessere Aufzeichnungen ersetzen schlechtere, nie
  umgekehrt (`quality`).
- MTGTop8: höchstens eine Anfrage alle 5 Sekunden (robots.txt), alles cachen.
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
- Arena-Log: Sitz des Spielers aus `systemSeatIds`, nicht fest Sitz 1. Nach
  Spielende kommen noch Nachrichten ohne Partie-Kennung (siehe Test
  `next_game_after_leftover_message`).
