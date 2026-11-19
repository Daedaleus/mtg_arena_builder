# Notes for working on this project

Features and download: [README.md](README.md). Building, tests, command line
and releases: [CONTRIBUTING.md](CONTRIBUTING.md). Open items: [TODO.md](TODO.md).

## Language and style

- Documentation (README, TODO, CONTRIBUTING, SECURITY, this file) and commit
  messages in **English**. Code comments and doc comments stay **German**
  (the existing code base is German throughout).
- Commit messages and pull request titles: Conventional Commits with the
  matching gitmoji icon after the colon, e.g. `feat(sim): ✨ add prowess` or
  `fix: 🐛 keep settings` (types and icons in CONTRIBUTING.md, checked by
  `.githooks/commit-msg` and `pr-title.yml`).
- The interface is bilingual (German, English): display texts live **only** in
  `locales/app.yml` and reach the code via `tr!("module.key", name = value)` –
  no display texts as string literals in code. New texts always in both
  languages; the tests `every_key_has_all_languages_and_same_placeholders`
  and `every_used_key_exists` check this. Tests run in German.
- Texts short and understandable for casual players; Arena terms as the
  respective Arena language version calls them.
- Card names: internally always English (`Card::name`, keys, Arena import),
  displayed with `Card::display_name()` or `CardDb::display_name()` in the
  selected language. Card images per language via `images::key`.
- Values from the Arena log never as loose strings: `games::arena`
  (`Phase`, zones, messages, events).
- The built-in egui font has no arrows (`→`, `≥`): use ", dann", ">" or words
  in the interface.

## Structure

Library (`src/lib.rs`) and program (`src/main.rs` only starts; the diagnostic
command line lives in `src/cli/`). `src/app/` contains only interface and
wiring – logic belongs in the domain modules so it stays testable.
Compute nothing expensive per frame: cache evaluations in `app/compute.rs`
(generation counter `data_gen`); long computations (sample hands, deck
building) run on the task thread (`app/tasks.rs`) and report back as `Msg`.
Interface state per area in `app/parts.rs` (tournament decks, games, magic.gg,
images, sample hands, built decks). egui only in `src/app/` and `main.rs` –
domain modules return their own types (e.g. `images::Picture`) and take
callbacks. Network access via the trait `http::Http` (in tests
`testutil::FakeHttp`).

Errors: `anyhow` for one-off errors and the command line; own enums with
`thiserror` where the caller distinguishes kinds (`CollectionError`,
`http::FetchError`, `paths::LoadError`). Their `#[error]` text stays technical
and English; translation happens only when displayed (`user_text()`), so a
language switch also covers error messages.

Domain types instead of loose numbers: deck ids as `app::DeckId` (pasted,
built, MTGTop8, magic.gg; stored in `settings.json` as a number via
`to_raw`/`from_raw`), formats as `meta::Format`, Arena card numbers always as
`carddb::GrpId` (in tests the constants from `testutil::cards` or `GrpId(…)`).
`LiveCreature::grp_id` stays `u32` because the simulation numbers creatures
sequentially there. `CardKey` (normalised card name) is deliberately an alias.

Large modules are folders with submodules per task; `mod.rs` describes the
layout at the top and re-exports the public parts, so paths like
`crate::games::report` stay the same. Rule of thumb: files below about 400
lines, functions below about 100 lines, otherwise split by task. Define domain
terms (colours, keywords, life points, mistake kinds) once and never compare
display texts – there are enums like `Mistake` or `LandVerdict` for that.

| Module | Purpose |
|---|---|
| `carddb/` | Arena card database: `load` (SQLite), `text` (roles, burn, mana from card text); shared domain terms: `color` (colours, basic lands), `keyword` (keywords like flying), `Rarity`, `GrpId`, `CardKey` |
| `collection` | Collection from Arena's memory (mtga-reader, admin rights) |
| `meta`, `magicgg` | MTGTop8 and magic.gg (fetch with cache, parser), Scryfall legality |
| `deck`, `similar`, `budget` | Check lists, replacement cards, budget version, wildcard plan |
| `sim/` | Sample hands (play a deck 1000 times): `game` (one game, totals), `pressure` (board, combat, race), `profile` (opponents and habits from your own games), `advice` |
| `style` | Play style (aggro/midrange/control) from decks and games, deck recommendation |
| `builder` | Decks from owned cards only |
| `games/` | Recording: `arena` (protocol values), `object`, `tracker`, `live_state`, `parser`; storage: `record` (data types, rank), `store` (`games.json`); analysis: `moments` (single turns), `mistake` (mistake kinds), `report` |
| `deckcheck/`, `opponents`, `progress` | Improve deck (`groups`, `mana`, `analysis`), opponents and archetype detection, progress |
| `live/`, `turnplan`, `combat` | Companion logic (`view`, `hints`, `damage`, `follow`; drawn in `app/companion`), turn suggestion, blocks/tricks/unblocked damage |
| `settings`, `present`, `paths`, `images`, `http` | Settings (format stored as code), display texts, paths, image cache (card images per language), network access |
| `i18n` | Language (operating system or setting), macro `tr!`; texts in `locales/app.yml` |
| `testutil/` | Test helpers: `cards`, `log` (synthetic log messages), `pages` (web pages) |

## Rules

- Before every commit: `cargo fmt`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test`. `build.ps1` does all of this and builds the release version.
- **Tests:** new logic gets tests in `src/tests/<module>.rs` (included via
  `#[cfg(test)] #[path = "tests/<module>.rs"] mod tests;`). Test data (cards,
  log messages, temporary folders) from `src/testutil/`. `coverage.ps1`
  requires at least 90 % line coverage excluding `src/app/` and `main.rs`.
- Tests must touch neither the network nor the real data directory. Network in
  tests via `testutil::FakeHttp` (with real, shortened responses from
  `testutil/fixtures/`), waiting via `testutil::FakeClock`. Only exception: the
  contract test `contract_with_real_scryfall` (`#[ignore]`, run by hand with
  `cargo test -- --ignored`).
- Network access in the app always through the shared `http::shared()` – it
  keeps the minimum gaps per site (`http::GAPS`) across all threads.
- The test `texts_have_no_broken_line_continuations` finds texts where the `\`
  at the end of a line got lost while editing (long runs of spaces).
- If what the log parser stores per game changes, increase `PARSER_VERSION` in
  `games/store.rs` – stored games are then recomputed from logs that still
  exist. Better recordings replace worse ones, never the other way round
  (`quality`).
- MTGTop8: at most one request every 5 seconds (robots.txt), cache everything;
  Scryfall 500 ms, magic.gg 2 s – configured in `http::GAPS`. After HTTP 429
  `http::Polite` blocks the site for the given time. Do not run the Scryfall
  contract test several times in a row.
- Commits and pushes only when the user asks. Remote `origin` is
  `Daedaleus/mtg_arena_builder` on GitHub. GitHub CLI:
  `C:\Program Files\GitHub CLI\gh.exe` (logged in).
- `main` is protected: never push to it directly. Work on a branch, push the
  branch, open a pull request with `gh pr create`; it is squash-merged once
  the required checks are green, so its title becomes the commit message.
- Releases: bump `version` in `Cargo.toml` via a pull request, then tag the
  merged commit on `main` with `vX.Y.Z` and push the tag (see
  CONTRIBUTING.md). Release notes: changelog from the commit messages via
  git-cliff (`cliff.toml`), then `.github/release-notes.md`.

## Pitfalls

- `build.ps1` stops the running app and restarts it. If an Arena game is in
  progress, ask first – the companion window disappears briefly.
- The release .exe has no console window and opens the interface: never start
  it from a script without a time limit. For console output use the debug
  build (`target\debug\...`) with `--games`, `--replay` etc.
- When changing Rust code (or YAML) via Python/shell scripts, `\n`, `\r`,
  `\1` and `\'` easily turn into real characters – check the result or use
  the Edit tool.
- Save PowerShell scripts (`build.ps1`, `coverage.ps1`) as UTF-8 with BOM and
  CRLF, otherwise Windows PowerShell 5.1 shows umlauts wrongly.
- Write `settings.json` by hand only without BOM (both are read).
- Load user data (`games.json`, `settings.json`) with `paths::load_json` and
  distinguish `LoadError`: damaged = back up (`backup_corrupt`), unreadable =
  do not overwrite. `read_json` only for caches.
- Arena log: the player's seat comes from `systemSeatIds`, not always seat 1.
  After the end of a game, messages without a game id still arrive (see test
  `next_game_after_leftover_message`).
- In Git Bash on the Windows CI runners there is no `shasum`; use
  `sha256sum` (macOS has only `shasum`).
