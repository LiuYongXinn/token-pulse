# Default read-only. Optional private wake-timer arm/cancel preflight never sleeps the computer.
param([switch]$CheckWakeTimer)
$ErrorActionPreference = 'Stop'
if (-not $IsWindows -or [IntPtr]::Size -ne 8 -or [Environment]::OSVersion.Version.Build -ne 19045) {
    throw 'This inspection requires reviewed Windows 10 build 19045 x64.'
}
Add-Type -Path (Join-Path $PSScriptRoot 'power-resume-probe.cs')
[TokenPulsePowerInspection]::Inspect([bool]$CheckWakeTimer) | ConvertTo-Json -Depth 3
