## Download

| System | File |
|---|---|
| Windows (x64) | `mtg_arena_builder-windows-x64.exe` |
| Linux (x64) | `mtg_arena_builder-linux-x64` |
| macOS (Apple Silicon) | `mtg_arena_builder-macos-arm64` (only if the macOS build finished) |

Every file was built by GitHub Actions from the tagged commit, scanned with
ClamAV (and Microsoft Defender for Windows) and has a signed build
provenance attestation.

## Verify your download

Checksum (PowerShell):

```powershell
Get-FileHash .\mtg_arena_builder-windows-x64.exe -Algorithm SHA256
```

Compare the result with `SHA256SUMS.txt`.

Origin (needs the [GitHub CLI](https://cli.github.com)):

```
gh attestation verify mtg_arena_builder-windows-x64.exe --repo Daedaleus/mtg_arena_builder
```

The program is not code-signed, so Windows SmartScreen may warn on first
start ("More info" > "Run anyway").
