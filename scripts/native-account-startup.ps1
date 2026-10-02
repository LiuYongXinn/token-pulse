param([switch]$ExistingAccount)
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'Native account startup acceptance requires Windows.' }
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    $env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
}
if (-not $ExistingAccount) {
    & cargo build -p token-pulse-quota --features test-fixture --bin quota-fixture
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
& cargo build -p token-pulse-desktop --features custom-protocol
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$startupId = [guid]::NewGuid().ToString()
$startupExe = Join-Path $PSScriptRoot '..\target\debug\token-pulse-desktop.exe'
$ownedServicePid = $null
$phases = if ($ExistingAccount) { @('local_seed', 'local_ready') } else { @('seed', 'ready', 'disabled', 'changed') }
foreach ($phase in $phases) {
    $startupArgs = @('--native-smoke', "--native-account-startup-id=$startupId", "--native-account-phase=$phase")
    if ($ExistingAccount) { $startupArgs += '--native-existing-account' }
    $output = & $startupExe @startupArgs 2>&1
    $phaseExit = $LASTEXITCODE
    $output | ForEach-Object { Write-Host $_ }
    if ($phaseExit -ne 0) { exit $phaseExit }
    $phaseLabel = $phase.Replace('_', '')
    if (-not ($output | Select-String -SimpleMatch "NATIVE_ACCOUNT_COLD_OK: $phaseLabel")) {
        throw "Cold phase $phase did not run; another development instance may be active."
    }
    foreach ($line in $output) {
        if ("$line" -match '^NATIVE_ACCOUNT_COLD_OWNED_PID: (\d+)$') {
            $ownedServicePid = [int]$Matches[1]
        }
    }
    if ($null -ne $ownedServicePid -and (Get-Process -Id $ownedServicePid -ErrorAction SilentlyContinue)) {
        throw 'Owned synthetic quota process survived application shutdown.'
    }
}
if ($ExistingAccount) {
    Write-Host 'NATIVE_ACCOUNT_EXISTING_COLD_SEQUENCE_OK: two independent processes, same isolated configuration, actual main/mini quota'
} else {
    Write-Host 'NATIVE_ACCOUNT_COLD_SEQUENCE_OK: four independent processes, same isolated database, no surviving owned service'
}
