# Read-only eligibility inspection. Never shuts down, restarts or signals Explorer.
param()
$ErrorActionPreference = 'Stop'
if (-not $IsWindows -or [IntPtr]::Size -ne 8 -or [Environment]::OSVersion.Version.Build -ne 19045) {
    throw 'This inspection requires the reviewed Windows 10 build 19045 x64.'
}
Add-Type -Path (Join-Path $PSScriptRoot 'explorer-restart-probe.cs')
$inspection = [ExplorerRestartInspection]::Inspect()
$inspection | ConvertTo-Json -Depth 4 -Compress
