<#
.SYNOPSIS
    Prüft den Code, baut die optimierte Release-Version nach dist\ und
    aktualisiert die Desktop-Verknüpfung.

.DESCRIPTION
    Schritte: Formatierung prüfen, Clippy (Warnungen = Fehler), Tests,
    Release-Build, .exe + Prüfsumme nach dist\ kopieren, Verknüpfung
    "MTG Arena Builder" auf dem Desktop anlegen bzw. aktualisieren.

    Die Verknüpfung zeigt immer auf dist\MTG Arena Builder.exe – nach jedem
    Lauf dieses Skripts startet sie also die aktuelle Version. Eine noch
    laufende App aus dist\ wird dafür beendet (ihr Zustand ist gespeichert).

.PARAMETER SkipChecks
    Nur bauen, ohne fmt/clippy/Tests.

.PARAMETER NoShortcut
    Keine Desktop-Verknüpfung anlegen.

.EXAMPLE
    .\build.ps1
#>
param([switch]$SkipChecks, [switch]$NoShortcut)

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

# Laufende App aus dist\ beenden, sonst ist die Datei gesperrt.
Get-Process | Where-Object { $_.Path -and $_.Path.StartsWith($dist, [StringComparison]::OrdinalIgnoreCase) } | ForEach-Object {
    Write-Host "Beende laufende App (PID $($_.Id))" -ForegroundColor Yellow
    if (-not $_.CloseMainWindow() -or -not $_.WaitForExit(5000)) { $_ | Stop-Process -Force }
    $_.WaitForExit(5000) | Out-Null
}

$built = "target\release\mtg_arena_builder.exe"
$versioned = Join-Path $dist "mtg_arena_builder-$version-windows-x64.exe"
$current = Join-Path $dist "MTG Arena Builder.exe"
Copy-Item $built $versioned -Force
Copy-Item $built $current -Force
$hash = (Get-FileHash $versioned -Algorithm SHA256).Hash.ToLower()
"$hash  $(Split-Path $versioned -Leaf)" | Out-File "$versioned.sha256" -Encoding ascii

if (-not $NoShortcut) {
    $link = Join-Path ([Environment]::GetFolderPath("Desktop")) "MTG Arena Builder.lnk"
    $shortcut = (New-Object -ComObject WScript.Shell).CreateShortcut($link)
    $shortcut.TargetPath = $current
    $shortcut.WorkingDirectory = $dist
    $shortcut.IconLocation = "$current,0"
    $shortcut.Description = "Turnierdecks mit der eigenen MTG-Arena-Sammlung abgleichen"
    $shortcut.Save()
    Write-Host "Verknüpfung aktualisiert: $link" -ForegroundColor Green
}

$sizeMb = [math]::Round((Get-Item $current).Length / 1MB, 1)
Write-Host "Fertig: $current (Version $version, $sizeMb MB)" -ForegroundColor Green
