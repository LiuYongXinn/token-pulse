param()
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'Native main-window acceptance requires Windows.' }
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { $env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH" }
& cargo build -p token-pulse-desktop --features custom-protocol
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$placementProbeId = [guid]::NewGuid().ToString()
$placementProbeExe = Join-Path $PSScriptRoot '..\target\debug\token-pulse-desktop.exe'
foreach ($phase in @('seed','restore','missing')) {
    $output = & $placementProbeExe --native-smoke "--native-main-window-id=$placementProbeId" "--native-main-window-phase=$phase" 2>&1
    $phaseExit = $LASTEXITCODE
    $output | ForEach-Object { Write-Host $_ }
    if ($phaseExit -ne 0) { exit $phaseExit }
    if (-not ($output | Select-String -SimpleMatch "NATIVE_MAIN_WINDOW_COLD_OK: $phase")) { throw 'Main-window phase did not run; a development instance may be active.' }
}
Write-Host 'NATIVE_MAIN_WINDOW_COLD_SEQUENCE_OK: three independent processes, ordinary native placement, maximize/minimize exclusion, close/reopen, synthetic small-work-area fit and missing-monitor fallback'
