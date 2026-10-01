$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'Native smoke requires Windows.' }
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    $env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
}
& cargo build -p token-pulse-desktop --features custom-protocol
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
& (Join-Path $PSScriptRoot '..\target\debug\token-pulse-desktop.exe') --native-smoke
exit $LASTEXITCODE
