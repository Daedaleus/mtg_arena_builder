# MTG Arena Builder

Desktop-App (Rust, egui) für Spieler von **Magic: The Gathering Arena**, die
nicht gern selbst Decks bauen. Sie gleicht aktuelle Turnierdecks mit der
eigenen Sammlung ab, baut Decks aus vorhandenen Karten, wertet die eigenen
Partien aus und gibt während einer Partie Hinweise in einem kleinen
Begleitfenster.

Die Oberfläche ist deutsch. Im Alltag genutzt wird die Windows-Version; Linux
und macOS werden in der CI gebaut und getestet, sind aber noch nicht im
echten Einsatz ausprobiert (siehe [ToDo.md](ToDo.md)).

## Funktionen

| Reiter | Was er tut |
|---|---|
| **Turnierdecks** | Archetypen und Decklisten von MTGTop8 je Format. Zeigt, was mit der eigenen Sammlung fehlt und ob die Wildcards reichen; Ersatzkarten aus der Sammlung, „Budget-Version bauen“, **Wildcard-Plan** (was jetzt herstellbar ist und in welcher Reihenfolge der Rest folgt), **Probehände** und Export für den Arena-Import. |
| **Offizielle Events** | Decklisten von magic.gg: World Championship, Arena Championship, Regional Championships und jede Woche Ranglisten-Decks (Traditional). Archetyp wird aus den MTGTop8-Listen erkannt; gleiche Deckansicht wie bei den Turnierdecks. |
| **Aus meiner Sammlung** | Baut 60-Karten-Decks je Farbe/Farbpaar nur aus eigenen Karten (Manakurve, Kreaturen, Removal, passende Länder). |
| **Eigene Liste** | Eine beliebige Deckliste einfügen und gegen die Sammlung prüfen. |
| **Meine Partien** | Auswertung aus dem Arena-Log: typische Fehler mit Beispielen („Land nicht gespielt“, „Besserer Zug möglich“, „Gefahrlosen Angriff ausgelassen“ …), **Spielstil** mit passenden Turnierdecks (Aggro, Midrange oder Control – je nachdem, womit du gewinnst), Gegner-Analyse, Partienliste (10 je Seite) und **Verlauf** (Constructed- und Limited-Rang, Siegquote je Deck als Diagramm). |
| **Deck verbessern** | Kartennutzung je Deck, Länder-Check, Probehände, Vergleich mit dem passenden Turnier-Archetyp und Tauschvorschläge. |

**Probehände:** spielt ein Deck 1000-mal bis Zug 6 durch (Mulligan, Land legen,
Zauber nach dem Zugvorschlag) und zeigt, wie oft Länder oder Farben fehlen,
wie oft Länder zu viel sind und wie viel Mana genutzt wird – mit Empfehlungen.

**Begleitfenster** (oben rechts einschalten): liest das Arena-Log live mit und
zeigt Mulligan-Rat, Zugvorschlag (Land und Zauber mit Farbprüfung, auch Mana
aus Kreaturen), gefahrlose Angriffe, tödlichen Schaden, Blockvorschläge (auch
gegen Bedrohlich) und Warnungen vor typischen Tricks des Gegners. Es greift
nicht ins Spiel ein.

## Voraussetzungen

- MTG Arena, in den Arena-Einstellungen **Konto > „Detaillierte Logs
  (Plugin-Unterstützung)“** eingeschaltet (danach Arena neu starten).
- Zum Lesen der Sammlung muss Arena laufen (Hauptmenü). Windows fragt dafür
  nach Administratorrechten.
- Zum Bauen: Rust (Edition 2024, getestet mit 1.100) und unter Windows die
  MSVC-Build-Tools.

## Bauen und starten

```powershell
.\build.ps1             # fmt, Clippy, Tests, Release-Build, Desktop-Verknüpfung
.\build.ps1 -Coverage   # zusätzlich Testabdeckung (mindestens 90 %)
.\build.ps1 -SkipChecks # nur bauen
```

Ergebnis: `dist\MTG Arena Builder.exe` (statisch gelinkt, ohne Installation
lauffähig) plus versionierte Kopie mit SHA-256. Die Verknüpfung „MTG Arena
Builder“ auf dem Desktop zeigt immer auf die aktuelle Version. Eine laufende
App wird beim Bauen beendet und danach neu gestartet.

Für die Entwicklung: `cargo run`, `cargo test`.

## Tests

```powershell
cargo test              # alle Tests
.\coverage.ps1 -Open    # Abdeckung messen, HTML-Bericht öffnen
```

Die Tests liegen je Modul in `src/tests/`. Sie brauchen weder Arena noch
Internet: Kartendatenbanken, Arena-Lognachrichten und Turnierseiten werden in
`src/testutil.rs` künstlich erzeugt. Die Abdeckung wird ohne Oberfläche
(`src/app/`) und Programmstart (`main.rs`) gemessen; `coverage.ps1` schlägt unter
90 % fehl. Für `coverage.ps1` einmalig:

```powershell
rustup component add llvm-tools-preview
cargo install cargo-llvm-cov --locked
```

## CI

Bei jedem Push auf `main` und bei Pull Requests prüft GitHub Actions
([.github/workflows/ci.yml](.github/workflows/ci.yml)) auf Windows, Linux und
macOS: Formatierung, Clippy, Tests, Release-Build und Testabdeckung. Die
gebauten Programme liegen als Artefakte am jeweiligen Lauf.

## Kommandozeile (Diagnose)

| Aufruf | Zweck |
|---|---|
| `--check` | Kartendaten, Sammlung und Turnierseiten prüfen |
| `--report [FORMAT]` | alle Archetypen gegen die gespeicherte Sammlung bewerten |
| `--variants FORMAT NAME` | alle Listen eines Archetyps bewerten |
| `--build [FORMAT]` | Deckvorschläge aus der Sammlung |
| `--budget FORMAT NAME` | Budget-Version und Wildcard-Plan eines Archetyps |
| `--sim` | Probehände für alle gespielten Decks |
| `--gg [FORMAT]` | Events und Decks von magic.gg |
| `--style` | Spielstil und Einteilung der zwischengespeicherten Turnierlisten |
| `--games` | Partien einlesen und auswerten |
| `--decks [FORMAT]` | „Deck verbessern“ und Gegner-Analyse |
| `--parse LOG…` | Logdatei einlesen, ohne gespeicherte Partien zu ändern |
| `--replay LOG [N]` | Begleitfenster-Hinweise für ein Log nachspielen |
| `--image NAME…` | Kartenbilder laden |

`FORMAT` ist das MTGTop8-Kürzel (`ST` Standard, `PI`, `EXP`, `HI`, `ALCH`).
Die Release-Version hat kein Konsolenfenster; für die Ausgabe `cargo run --
--games` usw. verwenden.

## Daten

Alles liegt in `%APPDATA%\mtg_arena_builder\data` (Linux/macOS: der
entsprechende Datenordner):

| Datei | Inhalt |
|---|---|
| `settings.json` | Einstellungen und zuletzt gewählte Ansicht |
| `collection.json` | zuletzt gelesene Sammlung und Wildcards |
| `games.json` | alle ausgewerteten Partien (Arena überschreibt sein Log bei jedem Start) |
| `cache\` | MTGTop8-Seiten, Decklisten, Scryfall-Legalität, Kartenbilder |

## Datenquellen

- Kartendaten: die lokale Arena-Datenbank `Raw_CardDatabase_*.mtga` (SQLite).
- Sammlung: Arbeitsspeicher von Arena über
  [mtga-reader](https://github.com/mtgatool/mtga-reader) (GPL-3.0).
- Partien und Rang: `Player.log` von Arena („Detaillierte Logs“).
- Turnierdecks: [MTGTop8](https://www.mtgtop8.com) (5 s Abstand je Anfrage, Cache)
  und [magic.gg](https://magic.gg/decklists) (2 s Abstand, Cache).
- Legalität und Kartenbilder: [Scryfall](https://scryfall.com) (ohne API-Schlüssel).

## Lizenz

GPL-3.0-or-later (wegen mtga-reader). Privates Werkzeug, nicht mit Wizards of
the Coast verbunden.
