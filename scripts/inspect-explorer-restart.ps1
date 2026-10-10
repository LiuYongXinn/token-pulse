# Read-only eligibility inspection. Never shuts down, restarts or signals Explorer.
param()
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'project-env.ps1')
if (-not $IsWindows -or [IntPtr]::Size -ne 8) {
    throw 'This inspection requires a 64-bit Windows process.'
}
Add-Type -Path (Join-Path $PSScriptRoot 'explorer-restart-probe.cs')
$inspection = [ExplorerRestartInspection]::Inspect()
$inspection | ConvertTo-Json -Depth 4 -Compress
