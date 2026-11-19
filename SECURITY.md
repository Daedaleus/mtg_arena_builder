# Security policy

## Supported versions

Only the [latest release](https://github.com/Daedaleus/mtg_arena_builder/releases/latest)
receives fixes.

## Reporting a vulnerability

Please **do not** open a public issue. Use GitHub's
[private vulnerability reporting](https://github.com/Daedaleus/mtg_arena_builder/security/advisories/new)
instead. You will get an answer within a few days; this is a hobby project,
so please allow some time for a fix.

## What the program does on your computer

- Reads Arena's memory **read-only** to get your collection (via
  [mtga-reader](https://github.com/mtgatool/mtga-reader)); on Windows this
  step asks for administrator rights. Nothing is written to Arena.
- Reads Arena's log file and local card database.
- Fetches public pages from MTGTop8, magic.gg and Scryfall. No personal data
  is sent.
- Stores its own data only in its data directory
  (`%APPDATA%\mtg_arena_builder\data` on Windows).

## Verifying downloads

Release files are built by GitHub Actions, scanned with ClamAV (and Microsoft
Defender for Windows) and carry a signed build provenance attestation:

```
gh attestation verify <file> --repo Daedaleus/mtg_arena_builder
```

Checksums are in `SHA256SUMS.txt` on each release.
