# Default read-only. Optional private wake-timer arm/cancel preflight never sleeps the computer.
param([switch]$CheckWakeTimer)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'project-env.ps1')
if (-not $IsWindows -or [IntPtr]::Size -ne 8) {
    throw 'This inspection requires a 64-bit Windows process.'
}
Add-Type -Path (Join-Path $PSScriptRoot 'power-resume-probe.cs')
[TokenPulsePowerInspection]::Inspect([bool]$CheckWakeTimer) | ConvertTo-Json -Depth 3
