param([switch]$Taskbar, [switch]$TaskbarActions, [switch]$TaskbarExplorerRestart, [switch]$PriceAliases, [switch]$OfflinePrices, [switch]$PriceRevalue, [switch]$Diagnostics, [switch]$RecoveryRoutes, [switch]$Notify, [switch]$Updates, [switch]$SourceDialogs, [switch]$AccountDialogs, [switch]$NotifyDialogs)
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'Native smoke requires Windows.' }
if ($TaskbarExplorerRestart -and $PSBoundParameters.Count -ne 1) { throw 'Actual Explorer restart acceptance must be the only selected scene.' }
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    $env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
}
if (-not ($Notify -or $Updates -or $SourceDialogs -or $NotifyDialogs -or $TaskbarExplorerRestart)) {
    & cargo build -p token-pulse-quota --features test-fixture --bin quota-fixture
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
& cargo build -p token-pulse-desktop --features custom-protocol
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
if (-not ($Notify -or $Updates -or $SourceDialogs -or $AccountDialogs -or $NotifyDialogs)) {
    & cargo build -p token-pulse-taskbar --bin token-pulse-taskbar-host
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
$probeArgs = @('--native-smoke')
if ($Taskbar) { $probeArgs += '--native-taskbar-smoke' }
if ($TaskbarActions) { $probeArgs += '--native-taskbar-actions-smoke' }
if ($TaskbarExplorerRestart) { $probeArgs += '--native-taskbar-explorer-restart-smoke' }
if ($PriceAliases) { $probeArgs += '--native-price-alias-smoke' }
if ($OfflinePrices) { $probeArgs += '--native-offline-prices-smoke' }
if ($PriceRevalue) { $probeArgs += '--native-price-revalue-smoke' }
if ($Diagnostics) { $probeArgs += '--native-diagnostics-smoke' }
if ($RecoveryRoutes) { $probeArgs += '--native-recovery-routes-smoke' }
if ($Notify) { $probeArgs += '--native-notify-smoke' }
if ($Updates) { $probeArgs += '--native-updates-smoke' }
if ($SourceDialogs) { $probeArgs += '--native-source-dialogs-smoke' }
if ($AccountDialogs) { $probeArgs += '--native-account-dialogs-smoke' }
if ($NotifyDialogs) { $probeArgs += '--native-notify-dialogs-smoke' }
& (Join-Path $PSScriptRoot '..\target\debug\token-pulse-desktop.exe') @probeArgs
exit $LASTEXITCODE
