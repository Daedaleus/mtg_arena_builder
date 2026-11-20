<#
.SYNOPSIS
    Windows part of `mise run install`: release into dist\, desktop shortcut,
    restart of the running app.

.DESCRIPTION
    Called by mise after `mise run build` (see mise.toml), not directly.
    Copies target\release\mtg_arena_builder.exe as a versioned file with
    SHA-256 and as "MTG Arena Builder.exe" into dist\ and creates the desktop
    shortcut "MTG Arena Builder". A running app from dist\ is stopped for
    this (its state is saved) and started again afterwards.
#>
$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
Set-Location $root

$version = (Select-String -Path Cargo.toml -Pattern '^version = "(.*)"$' | Select-Object -First 1).Matches[0].Groups[1].Value
$dist = Join-Path $root "dist"
New-Item -ItemType Directory -Force $dist | Out-Null

# Stop a running app from dist\, otherwise the file is locked (restart it afterwards).
$wasRunning = $false
Get-Process | Where-Object { $_.Path -and $_.Path.StartsWith($dist, [StringComparison]::OrdinalIgnoreCase) } | ForEach-Object {
    Write-Host "Stopping running app (PID $($_.Id))" -ForegroundColor Yellow
    $script:wasRunning = $true
    # Close only the main window normally (saves the state). If Windows treats
    # the companion window as the main window, only that one would close and
    # stay switched off – then rather stop the process directly.
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
$shortcut.Description = "Compare tournament decks with your MTG Arena collection"
$shortcut.Save()
Write-Host "Shortcut updated: $link" -ForegroundColor Green

$sizeMb = [math]::Round((Get-Item $current).Length / 1MB, 1)
Write-Host "Done: $current (version $version, $sizeMb MB)" -ForegroundColor Green
if ($wasRunning) {
    Start-Process $current -WorkingDirectory $dist
    Write-Host "App restarted" -ForegroundColor Green
}
