<#
.SYNOPSIS
    Windows-Teil von `mise run install`: Release nach dist\, Desktop-Verknüpfung,
    laufende App neu starten.

.DESCRIPTION
    Wird von mise nach `mise run build` aufgerufen (siehe mise.toml), nicht
    direkt. Kopiert target\release\mtg_arena_builder.exe als versionierte Datei
    mit SHA-256 und als "MTG Arena Builder.exe" nach dist\ und legt die
    Verknüpfung "MTG Arena Builder" auf dem Desktop an. Eine laufende App aus
    dist\ wird dafür beendet (ihr Zustand ist gespeichert) und danach neu
    gestartet.
#>
$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
Set-Location $root

$version = (Select-String -Path Cargo.toml -Pattern '^version = "(.*)"$' | Select-Object -First 1).Matches[0].Groups[1].Value
$dist = Join-Path $root "dist"
New-Item -ItemType Directory -Force $dist | Out-Null

# Laufende App aus dist\ beenden, sonst ist die Datei gesperrt (und danach neu starten).
$wasRunning = $false
Get-Process | Where-Object { $_.Path -and $_.Path.StartsWith($dist, [StringComparison]::OrdinalIgnoreCase) } | ForEach-Object {
    Write-Host "Beende laufende App (PID $($_.Id))" -ForegroundColor Yellow
    $script:wasRunning = $true
    # Nur das Hauptfenster regulär schließen (speichert den Zustand). Hält Windows das
    # Begleitfenster für das Hauptfenster, würde sonst nur dieses geschlossen und
    # dauerhaft abgeschaltet – dann lieber direkt beenden.
    $main = $_.MainWindowTitle -eq "MTG Arena Builder"
    if (-not $main -or -not $_.CloseMainWindow() -or -not $_.WaitForExit(5000)) { $_ | Stop-Process -Force }
    $_.WaitForExit(5000) | Out-Null
}

$built = "target\release\mtg_arena_builder.exe"
$versioned = Join-Path $dist "mtg_arena_builder-$version-windows-x64.exe"
$current = Join-Path $dist "MTG Arena Builder.exe"
Copy-Item $built $versioned -Force
Copy-Item $built $current -Force
$hash = (Get-FileHash $versioned -Algorithm SHA256).Hash.ToLower()
"$hash  $(Split-Path $versioned -Leaf)" | Out-File "$versioned.sha256" -Encoding ascii

$link = Join-Path ([Environment]::GetFolderPath("Desktop")) "MTG Arena Builder.lnk"
$shortcut = (New-Object -ComObject WScript.Shell).CreateShortcut($link)
$shortcut.TargetPath = $current
$shortcut.WorkingDirectory = $dist
$shortcut.IconLocation = "$current,0"
$shortcut.Description = "Turnierdecks mit der eigenen MTG-Arena-Sammlung abgleichen"
$shortcut.Save()
Write-Host "Verknüpfung aktualisiert: $link" -ForegroundColor Green

$sizeMb = [math]::Round((Get-Item $current).Length / 1MB, 1)
Write-Host "Fertig: $current (Version $version, $sizeMb MB)" -ForegroundColor Green
if ($wasRunning) {
    Start-Process $current -WorkingDirectory $dist
    Write-Host "App neu gestartet" -ForegroundColor Green
}
