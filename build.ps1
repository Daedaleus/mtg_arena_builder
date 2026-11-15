<#
.SYNOPSIS
    Prüft den Code und baut die optimierte Release-Version nach dist\.

.DESCRIPTION
    Schritte: Formatierung prüfen, Clippy (Warnungen = Fehler), Tests,
    Release-Build, .exe + Prüfsumme nach dist\ kopieren.

.PARAMETER SkipChecks
    Nur bauen, ohne fmt/clippy/Tests.

.EXAMPLE
    .\build.ps1
#>
param([switch]$SkipChecks)

$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot

function Step($name, [scriptblock]$cmd) {
    Write-Host "==> $name" -ForegroundColor Cyan
    & $cmd
    if ($LASTEXITCODE -ne 0) { throw "$name fehlgeschlagen (Exit-Code $LASTEXITCODE)" }
}

if (-not $SkipChecks) {
    Step "Formatierung prüfen" { cargo fmt --all -- --check }
    Step "Clippy"              { cargo clippy --all-targets --locked -- -D warnings }
    Step "Tests"               { cargo test --locked }
}
Step "Release-Build" { cargo build --release --locked }

$version = (cargo metadata --no-deps --format-version 1 | ConvertFrom-Json).packages[0].version
$dist = Join-Path $PSScriptRoot "dist"
New-Item -ItemType Directory -Force $dist | Out-Null
$target = Join-Path $dist "mtg_arena_builder-$version-windows-x64.exe"
Copy-Item "target\release\mtg_arena_builder.exe" $target -Force
$hash = (Get-FileHash $target -Algorithm SHA256).Hash.ToLower()
"$hash  $(Split-Path $target -Leaf)" | Out-File "$target.sha256" -Encoding ascii

$sizeMb = [math]::Round((Get-Item $target).Length / 1MB, 1)
Write-Host "Fertig: $target ($sizeMb MB)" -ForegroundColor Green
