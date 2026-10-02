param([switch]$Taskbar, [switch]$TaskbarActions, [switch]$PriceAliases)
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'Native smoke requires Windows.' }
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    $env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
}
& cargo build -p token-pulse-quota --features test-fixture --bin quota-fixture
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
& cargo build -p token-pulse-desktop --features custom-protocol
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
& cargo build -p token-pulse-taskbar --bin token-pulse-taskbar-host
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$probeArgs = @('--native-smoke')
if ($Taskbar) { $probeArgs += '--native-taskbar-smoke' }
if ($TaskbarActions) { $probeArgs += '--native-taskbar-actions-smoke' }
if ($PriceAliases) { $probeArgs += '--native-price-alias-smoke' }
& (Join-Path $PSScriptRoot '..\target\debug\token-pulse-desktop.exe') @probeArgs
exit $LASTEXITCODE
