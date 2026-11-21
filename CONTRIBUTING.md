# Contributing

Thanks for your interest! Bug reports and ideas are welcome as
[issues](https://github.com/Daedaleus/mtg_arena_builder/issues). For larger
code changes please open an issue first so we can agree on the approach.

Project conventions (structure, domain types, network rules, pitfalls) are in
[CLAUDE.md](CLAUDE.md). In short: interface texts only in `locales/app.yml` in
both languages, logic outside `src/app/`, tests without network access.
Code comments, documentation and commit messages are English.

## Building

Tools and tasks come from [mise](https://mise.jdx.dev) (`mise.toml`), the same
on Windows, Linux and macOS and in CI. On Windows you also need the MSVC build
tools; Linux needs the usual egui packages (see `.github/workflows/ci.yml`).

```powershell
mise trust          # once per clone: allow the tasks in mise.toml
mise install        # Rust (with clippy, rustfmt, llvm-tools), cargo-llvm-cov, git-cliff
mise tasks          # list of tasks
```

| Task | What it does |
|---|---|
| `mise run check` | Formatting, Clippy (warnings as errors), all tests |
| `mise run fmt` | Format the code |
| `mise run coverage` | Test coverage, fails below 90 % (HTML report in `target/llvm-cov/html`) |
| `mise run build` | `check`, then the release build |
| `mise run install` | `build`, then a versioned copy with SHA-256 in `dist/`. On Windows also `dist\MTG Arena Builder.exe` (statically linked, no installation needed), a desktop shortcut, and a running instance is stopped and restarted |
| `mise run changelog` | Preview of the next release notes |

`cargo run` starts a debug build directly.

## Tests

Tests live per module in `src/tests/`. They need neither Arena nor the
internet: card databases, Arena log messages and web pages are generated in
`src/testutil/`, network replies come from recorded responses in
`src/testutil/fixtures/`. Coverage is measured without the interface
(`src/app/`) and program start (`main.rs`); `mise run coverage` fails below
90 % (filter and minimum in `mise.toml`).

One test talks to the real Scryfall API and is ignored by default; run it by
hand, and not several times in a row: `cargo test -- --ignored`.

Before every commit: `mise run check`.

## Pull requests

`main` is protected: every change goes through a pull request, also for the
maintainer. Create a branch, push it and open a pull request; it can be
merged once the CI checks are green. Keep pull requests focused on one topic.
Pull requests are squash-merged, so the **title** becomes the commit message
on `main` and must follow the same format (checked by
[pr-title.yml](.github/workflows/pr-title.yml)).

## Command line (diagnostics)

The release build has no console window; use `cargo run -- <args>`.

| Call | Purpose |
|---|---|
| `--check` | Check card data, collection and tournament pages |
| `--report [FORMAT]` | Rate all archetypes against the saved collection |
| `--variants FORMAT NAME` | Rate all lists of one archetype |
| `--build [FORMAT]` | Deck suggestions from the collection |
| `--budget FORMAT NAME` | Budget version and wildcard plan of an archetype |
| `--sim` | Sample hands for all played decks |
| `--gg [FORMAT]` | Events and decks from magic.gg |
| `--style` | Play style and classification of cached tournament lists |
| `--games` | Read and analyse games |
| `--decks [FORMAT]` | "Improve deck" and opponent analysis |
| `--parse LOG…` | Read a log file without changing saved games |
| `--replay LOG [N]` | Replay companion hints for a log |
| `--image NAME…` | Load card images |

`FORMAT` is the MTGTop8 code (`ST` Standard, `PI`, `EXP`, `HI`, `ALCH`).

### Screenshots

`cargo run -- --screenshots [DIR]` (default `docs/screenshots`) opens the
interface in English, goes through the tabs, selects an archetype and an
event, saves one PNG per tab and closes. It uses your own data and changes no
settings. Takes about a minute (MTGTop8 is fetched at most every 5 s).

## Data files

Everything lives in `%APPDATA%\mtg_arena_builder\data` (Linux/macOS: the
corresponding data directory):

| File | Content |
|---|---|
| `settings.json` | Settings and last selected view |
| `collection.json` | Last read collection and wildcards |
| `games.json` | All analysed games (Arena overwrites its log on every start) |
| `cache/` | MTGTop8 pages, decklists, Scryfall legality, card images |

## CI

Every push to `main` and every pull request runs
[ci.yml](.github/workflows/ci.yml) on Windows, Linux and macOS: formatting,
Clippy, tests, release build, test coverage and a malware scan of the built
programs (Microsoft Defender on Windows, ClamAV for all). If GitHub has no
free macOS runner, the run still passes; Windows and Linux must always pass.
[audit.yml](.github/workflows/audit.yml) checks the dependencies for known
vulnerabilities (cargo-deny) on changes and weekly; Dependabot proposes
updates.

## Releasing

1. Bump `version` in `Cargo.toml` (e.g. `0.2.0`) in a pull request and merge
   it.
2. Tag the merged commit on `main` and push the tag:

   ```powershell
   git switch main
   git pull
   git tag -a v0.2.0 -m "Version 0.2.0"
   git push origin v0.2.0
   ```

The tag triggers the full CI run. If everything passes and the tag matches the
version in `Cargo.toml`, the job "GitHub release" publishes the programs
(Windows, Linux, macOS if built) with `.sha256` files, a `SHA256SUMS.txt` and a
signed build provenance attestation.

The release notes start with **What's changed**: all commits since the
previous release, grouped by type with their icon (✨ Features, 🐛 Bug fixes,
…), generated by [git-cliff](https://git-cliff.org) from the commit messages
(configuration in [cliff.toml](cliff.toml)). Commits without the conventional
format end up under "Other". Below follow download and verification hints
from [.github/release-notes.md](.github/release-notes.md). Preview what the
next release will list:

```powershell
mise run changelog
```
