# TODO

Open items and ideas. Finished work is in the Git history and the
[releases](https://github.com/Daedaleus/mtg_arena_builder/releases).

## Ideas

- [ ] Sample hands, race: prowess, the opponent's evasion (e.g. flying) and
      own combat tricks are still missing – spell-heavy decks (e.g. Slickshot)
      look worse than they are.
- [ ] Wildcard plan: estimated play time / packs until the next step.
- [ ] Turn suggestion: Treasures and other tokens (they are not in the card
      database).

## Technical

- [ ] Try the Linux build in real use (CI builds and tests it; reading the
      collection needs root there). macOS only when an opportunity arises.
- [ ] Code signing for the Windows executable (e.g. SignPath for open source)
      so SmartScreen stops warning.

## Recently done

- Screenshots in the README, taken by the app itself (`--screenshots`);
  colour names, "complete" and the card-data status now follow the
  interface language.
- Releases from version tags with malware scan (ClamAV, Defender), SHA-256
  checksums and signed build provenance; weekly dependency audit
  (cargo-deny) and Dependabot.
- Arena card ids as their own type `GrpId`; one background task thread.
- Polite network access: minimum gap per site, back-off after HTTP 429,
  recorded Scryfall responses for tests plus one manual contract test.
- Error types with `thiserror`; damaged `games.json` / `settings.json` are
  backed up instead of overwritten.
- English and German interface, card names and images per language.
- Sample hands against your own opponents up to turn 10, with removal and
  combat tricks as often as your opponents played them.
