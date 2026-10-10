param()
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'project-env.ps1')
if (-not $IsWindows) {
    throw 'This native floating-window placement acceptance requires Windows.'
}
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    $env:PATH = "$env:CARGO_HOME\bin;$env:PATH"
}
& cargo build -p token-pulse-desktop --features custom-protocol --locked --offline
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$executable = Join-Path $PSScriptRoot '..\target\debug\token-pulse-desktop.exe'
$output = & $executable --native-smoke --native-mini-placement-smoke 2>&1
$acceptanceExit = $LASTEXITCODE
$output | ForEach-Object { Write-Host $_ }
if ($acceptanceExit -ne 0) { exit $acceptanceExit }
if (-not ($output | Select-String -SimpleMatch 'NATIVE_MINI_OUTER_BOUNDS_OK')) {
    throw 'The isolated floating-window placement scene did not complete.'
}
