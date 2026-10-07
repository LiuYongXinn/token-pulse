# Explicit development acceptance only. Normal Restart Manager shutdown, never force/OS reboot.
param([Parameter(Mandatory)][uint32]$BaselineShellPid, [switch]$RestartShell)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'project-env.ps1')
$null = . (Join-Path $PSScriptRoot 'inspect-explorer-restart.ps1')
$before = [ExplorerRestartInspection]::Inspect()
if ($before.ShellPid -ne $BaselineShellPid) { throw 'Explorer differs from the explicit baseline PID.' }
if (-not $RestartShell) { $before | ConvertTo-Json -Depth 4 -Compress; return }
if (-not $before.Eligible) { throw ('Normal Explorer restart refused: ' + $before.Reason) }

$result = $null
try {
    $result = [ExplorerNormalRestart]::Run($BaselineShellPid)
} finally {
    # Recovery only if the explicitly held old process ended and no shell appeared.
    $old = Get-Process -Id $BaselineShellPid -ErrorAction SilentlyContinue
    if ($null -eq $old -and [ExplorerNormalRestart]::ShellPid() -eq 0) {
        $waitUntil = [DateTime]::UtcNow.AddSeconds(5)
        while ([ExplorerNormalRestart]::ShellPid() -eq 0 -and [DateTime]::UtcNow -lt $waitUntil) { Start-Sleep -Milliseconds 100 }
        if ([ExplorerNormalRestart]::ShellPid() -eq 0) {
            $shellExecutable = Join-Path $env:WINDIR 'explorer.exe'
            $null = Start-Process -FilePath $shellExecutable -WindowStyle Hidden -PassThru
        }
    }
}
$deadline = [DateTime]::UtcNow.AddSeconds(15)
while ([ExplorerNormalRestart]::ShellPid() -eq 0 -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 100 }
$after = [ExplorerRestartInspection]::Inspect()
if ($after.ShellPid -ne $BaselineShellPid) { $result.NewPid = $after.ShellPid }
$result | ConvertTo-Json -Depth 4 -Compress
if ($result.ShutdownStatus -ne 0 -or $result.RestartStatus -ne 0 -or -not $result.OldProcessExited -or $null -eq $result.NewPid) { throw 'Actual normal Explorer restart did not satisfy all acceptance conditions.' }
