# Contributing

Thanks for your interest! Bug reports and ideas are welcome as
[issues](https://github.com/Daedaleus/mtg_arena_builder/issues). For larger
code changes please open an issue first so we can agree on the approach.

Project conventions (structure, domain types, network rules, pitfalls) are in
[CLAUDE.md](CLAUDE.md). In short: interface texts only in `locales/app.yml` in
both languages, logic outside `src/app/`, tests without network access.
Code comments are German, documentation and commit messages English.

## Building

Requires Rust (edition 2024) and, on Windows, the MSVC build tools. Linux
needs the usual egui packages (see `.github/workflows/ci.yml`).

```powershell
cargo run               # debug build
.\build.ps1             # fmt, Clippy, tests, release build, desktop shortcut
.\build.ps1 -Coverage   # additionally test coverage (at least 90 %)
.\build.ps1 -SkipChecks # build only
```

`build.ps1` puts `dist\MTG Arena Builder.exe` (statically linked, no
installation needed) plus a versioned copy with SHA-256 into `dist\`. A running
instance is stopped before building and restarted afterwards.

## Tests

```powershell
cargo test              # all tests (about 0.3 s)
.\coverage.ps1 -Open    # measure coverage, open HTML report
```

Tests live per module in `src/tests/`. They need neither Arena nor the
internet: card databases, Arena log messages and web pages are generated in
`src/testutil/`, network replies come from recorded responses in
`src/testutil/fixtures/`. Coverage is measured without the interface
(`src/app/`) and program start (`main.rs`); `coverage.ps1` fails below 90 %.
One-time setup:

```powershell
rustup component add llvm-tools-preview
cargo install cargo-llvm-cov --locked
```

One test talks to the real Scryfall API and is ignored by default; run it by
hand, and not several times in a row: `cargo test -- --ignored`.

Before every commit: `cargo fmt`, `cargo clippy --all-targets -- -D warnings`,
`cargo test` (or simply `.\build.ps1`).

## Commit messages

[Conventional Commits](https://www.conventionalcommits.org) with a
[gitmoji](https://gitmoji.dev) icon after the colon, in English:

```
<type>(<optional scope>): <icon> <description>

feat(sim): ✨ add prowess to the race
fix(collection): 🐛 ask for admin rights only when access is denied
docs: 📝 add screenshots to the README
```

| Type | Icon | For |
|---|---|---|
| `feat` | ✨ | a new feature |
| `fix` | 🐛 | a bug fix |
| `docs` | 📝 | documentation only |
| `style` | 🎨 | formatting, no code change |
| `refactor` | ♻️ | restructuring without changing behaviour |
| `perf` | ⚡️ | faster or leaner |
| `test` | ✅ | tests only |
| `build` | 📦️ | build scripts, dependencies |
| `ci` | 👷 | GitHub Actions |
| `chore` | 🔧 | everything else (config, housekeeping) |
| `revert` | ⏪️ | undo an earlier commit |

The scope is optional and names the module (`sim`, `games`, `live`, `app` …).
A breaking change gets a `!` before the colon (`feat!: ✨ …`). The description
starts lowercase and has no full stop.

The hook in `.githooks/` checks this when committing. Switch it on once per
clone:

```powershell
git config core.hooksPath .githooks
```

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
version in `Cargo.toml`, the job "GitHub-Release" publishes the programs
(Windows, Linux, macOS if built) with `.sha256` files, a `SHA256SUMS.txt` and a
signed build provenance attestation. The notes are
[.github/release-notes.md](.github/release-notes.md) plus a changelog
generated from the commits.
