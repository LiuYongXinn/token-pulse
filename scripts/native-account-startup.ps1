param([switch]$ExistingAccount, [switch]$TaskbarAccount)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'project-env.ps1')
if (-not $IsWindows) { throw 'Native account startup acceptance requires Windows.' }
if ($TaskbarAccount -and -not $ExistingAccount) { throw 'TaskbarAccount requires the explicit ExistingAccount opt-in.' }
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    $env:PATH = "$env:CARGO_HOME\bin;$env:PATH"
}
if (-not $ExistingAccount) {
    & cargo build -p token-pulse-quota --features test-fixture --bin quota-fixture
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
& cargo build -p token-pulse-desktop --features custom-protocol
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
if ($TaskbarAccount) {
    & cargo build -p token-pulse-taskbar --bin token-pulse-taskbar-host
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
$startupId = [guid]::NewGuid().ToString()
$startupExe = Join-Path $PSScriptRoot '..\target\debug\token-pulse-desktop.exe'
$ownedServicePid = $null
$ownedTaskbarPid = $null
$phases = if ($TaskbarAccount) { @('local_seed', 'local_taskbar') } elseif ($ExistingAccount) { @('local_seed', 'local_ready') } else { @('seed', 'ready', 'disabled', 'changed') }
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
        if ("$line" -match '^NATIVE_TASKBAR_ACCOUNT_OWNED_PID: (\d+)$') {
            $ownedTaskbarPid = [int]$Matches[1]
        }
    }
    if ($null -ne $ownedServicePid -and (Get-Process -Id $ownedServicePid -ErrorAction SilentlyContinue)) {
        throw 'Owned synthetic quota process survived application shutdown.'
    }
    if ($null -ne $ownedTaskbarPid -and (Get-Process -Id $ownedTaskbarPid -ErrorAction SilentlyContinue)) {
        throw 'Owned taskbar process survived application shutdown.'
    }
    if ($TaskbarAccount -and $phase -eq 'local_taskbar' -and
        ($null -eq $ownedTaskbarPid -or -not ($output | Select-String -SimpleMatch 'NATIVE_TASKBAR_EXISTING_ACCOUNT_OK:'))) {
        throw 'Taskbar account phase did not prove the native host scenario.'
    }
}
if ($ExistingAccount) {
    Write-Host 'NATIVE_ACCOUNT_EXISTING_COLD_SEQUENCE_OK: two independent processes, same isolated configuration, actual main/mini quota'
} else {
    Write-Host 'NATIVE_ACCOUNT_COLD_SEQUENCE_OK: four independent processes, same isolated database, no surviving owned service'
}
