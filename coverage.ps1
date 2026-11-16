<#
.SYNOPSIS
    Misst die Testabdeckung und erzeugt einen HTML-Bericht.

.DESCRIPTION
    Nutzt cargo-llvm-cov. Die Oberfläche (app.rs), der Programmstart (main.rs)
    und die Testhilfen zählen nicht mit: Sie enthalten nur Bedienelemente und
    Verdrahtung, die eigentliche Logik steckt in den übrigen Modulen.

    Liegt die Zeilenabdeckung unter -Min Prozent, schlägt das Skript fehl.
    Bericht: target\llvm-cov\html\index.html

    Einmalig installieren:
        rustup component add llvm-tools-preview
        cargo install cargo-llvm-cov --locked

.PARAMETER Min
    Mindestabdeckung der Zeilen in Prozent (Standard 90).

.PARAMETER Open
    Bericht danach im Browser öffnen.

.EXAMPLE
    .\coverage.ps1 -Open
#>
param([int]$Min = 90, [switch]$Open)

$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot

cargo llvm-cov --version *> $null
if ($LASTEXITCODE -ne 0) {
    Write-Host "cargo-llvm-cov fehlt. Installieren mit:" -ForegroundColor Red
    Write-Host "  rustup component add llvm-tools-preview"
    Write-Host "  cargo install cargo-llvm-cov --locked"
    exit 1
}

$ignore = '(app|testutil|main)\.rs$|[\/]tests[\/]'
Write-Host "==> Tests mit Abdeckungsmessung" -ForegroundColor Cyan
cargo llvm-cov --lib --locked --html --ignore-filename-regex $ignore
if ($LASTEXITCODE -ne 0) { throw "Tests fehlgeschlagen (Exit-Code $LASTEXITCODE)" }

cargo llvm-cov report --summary-only --ignore-filename-regex $ignore --fail-under-lines $Min
$failed = $LASTEXITCODE -ne 0
$report = Join-Path $PSScriptRoot "target\llvm-cov\html\index.html"
Write-Host "Bericht: $report" -ForegroundColor Green
if ($Open) { Start-Process $report }
if ($failed) { throw "Zeilenabdeckung unter $Min %" }
