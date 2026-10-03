# Default preflight only. -ActualStandby pauses the whole computer; never restarts it.
param([switch]$ActualStandby, [switch]$Taskbar, [switch]$ApplicationRight, [string]$EvidenceDirectory)
$ErrorActionPreference = 'Stop'
if ($ApplicationRight -and -not $Taskbar) { throw 'ApplicationRight requires the explicit Taskbar scene.' }
$position = if ($Taskbar) { if ($ApplicationRight) { 'application_right' } else { 'notification_left' } } else { $null }
if (-not $IsWindows -or [IntPtr]::Size -ne 8 -or [Environment]::OSVersion.Version.Build -ne 19045) {
    throw 'This acceptance requires reviewed Windows 10 build 19045 x64.'
}
if (-not $EvidenceDirectory) { $EvidenceDirectory = Join-Path ([IO.Path]::GetTempPath()) ('tokenpulse-system-power-' + [guid]::NewGuid().ToString('N')) }
if (Test-Path -LiteralPath $EvidenceDirectory) { throw 'Use a new evidence directory to preserve prior runs.' }
$null = New-Item -ItemType Directory -Path $EvidenceDirectory
Add-Type -Path (Join-Path $PSScriptRoot 'power-resume-probe.cs')
Add-Type -Path (Join-Path $PSScriptRoot 'standby-resume-driver.cs')
function Test-FinalPower {
    $current = [TokenPulsePowerInspection]::Inspect($false)
    return $current.StandbyS3 -eq $true -and $current.StandbyAllowed -and $current.HibernationFilePresent -eq $false -and $current.AcLineStatus -eq 1 -and $current.WakePolicyIndex -eq 1
}
function Read-SelectedPowerEvents([long]$AfterRecord) {
    $query = "*[System[EventRecordID > $AfterRecord and ((Provider[@Name='Microsoft-Windows-Kernel-Power'] and (EventID=42 or EventID=107)) or (Provider[@Name='Microsoft-Windows-Power-Troubleshooter'] and EventID=1))]]"
    $records = @(Get-WinEvent -LogName System -FilterXPath $query -MaxEvents 32 -ErrorAction SilentlyContinue -ErrorVariable readErrors)
    if (@($readErrors | Where-Object { $_.FullyQualifiedErrorId -notlike 'NoMatchingEventsFound*' }).Count) { throw 'System power event query failed.' }
    foreach ($record in $records) {
        [xml]$eventXml = $record.ToXml()
        $state = @{}
        foreach ($datum in $eventXml.Event.EventData.Data) {
            if ($datum.Name -in @('TargetState','EffectiveState')) { $state[$datum.Name] = [int]$datum.InnerText }
        }
        [pscustomobject]@{ RecordId = $record.RecordId; Id = $record.Id; Provider = $record.ProviderName; TimeUtc = $record.TimeCreated.ToUniversalTime().ToString('o'); TargetState = $state['TargetState']; EffectiveState = $state['EffectiveState'] }
    }
}
$power = [TokenPulsePowerInspection]::Inspect($true)
$privilege = [TokenPulseStandbyDriver]::Inspect()
# Reading one latest record proves permission without exporting any event message or payload.
$baselineRecord = Get-WinEvent -LogName System -MaxEvents 1 -ErrorAction Stop
$preflight = [pscustomobject]@{
    Schema = 1; Power = $power; Privilege = $privilege; SystemRecordBaseline = $baselineRecord.RecordId
    Eligible = ($power.Eligible -eq $true -and $power.HibernationFilePresent -eq $false -and $privilege.PrivilegePresent -eq $true -and $privilege.TokenClosed -eq $true)
    ActualStandbyRequested = [bool]$ActualStandby; TaskbarRequested = [bool]$Taskbar; TaskbarPosition = $position; ComputerRestart = $false; PolicyChanged = $false
}
$preflight | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $EvidenceDirectory 'preflight.json')
if (-not $ActualStandby) {
    [pscustomobject]@{ EvidenceDirectory = $EvidenceDirectory; Preflight = $preflight; ActualSleepInvoked = $false } | ConvertTo-Json -Depth 6
    exit 0
}
if (-not $preflight.Eligible) { throw 'Actual standby refused: power, wake timer, own privilege or event-log preflight incomplete.' }
$executable = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\target\debug\token-pulse-desktop.exe')).Path
if (@(Get-Process token-pulse-desktop -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $executable }).Count) {
    throw 'Another debug instance exists; refusing to redirect the acceptance to it.'
}
$stdout = Join-Path $EvidenceDirectory 'observer.stdout.log'
$stderr = Join-Path $EvidenceDirectory 'observer.stderr.log'
$scene = if ($Taskbar) { if ($ApplicationRight) { 'SystemTaskbarRightObserver' } else { 'SystemTaskbarObserver' } } else { 'SystemObserver' }
$sceneArgument = if ($Taskbar) { '--native-power-taskbar-resume-smoke' } else { '--native-power-resume-smoke' }
$hostExecutable = [IO.Path]::Combine([IO.Path]::GetDirectoryName($executable),'token-pulse-taskbar-host.exe')
$hostSha256 = if ($Taskbar) { (Get-FileHash -LiteralPath $hostExecutable -Algorithm SHA256).Hash.ToLowerInvariant() } else { $null }
$observerArguments = @('--native-smoke',$sceneArgument)
if ($ApplicationRight) { $observerArguments += '--application-right' }
$observer = Start-Process -FilePath $executable -ArgumentList $observerArguments -WindowStyle Hidden -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr
$observerStart = $observer.StartTime.ToUniversalTime().Ticks
$readyDeadline = [DateTime]::UtcNow.AddSeconds(45)
$ready = $false
while (-not $observer.HasExited -and [DateTime]::UtcNow -lt $readyDeadline) {
    if (Test-Path -LiteralPath $stdout) {
        $readyLine = "NATIVE_POWER_READY: scene=$scene pid=$($observer.Id) fixture_parked=true baseline_tokens=3 pending_tokens=7 watcher=false poll_seconds=3600 computer_restart=false"
        if (@(Get-Content -LiteralPath $stdout | Where-Object { $_ -ceq $readyLine }).Count -eq 1) { $ready = $true; break }
    }
    Start-Sleep -Milliseconds 100
}
if (-not $ready) { throw 'Own observer readiness barrier failed; no standby requested.' }
$intentWriter = [Action[TokenPulseStandbyDriver+Evidence]]{
    param($intent)
    [pscustomobject]@{ IntentOnly = $true; ObserverPid = $observer.Id; ObserverStartUtcTicks = $observerStart; Driver = $intent; ComputerRestart = $false } |
        ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $EvidenceDirectory 'standby-intent.json')
}
$driver = [TokenPulseStandbyDriver]::Request($observer.Id, $observerStart, $executable, [Func[bool]]{ Test-FinalPower }, $intentWriter)
$driver | ConvertTo-Json -Depth 3 | Set-Content -LiteralPath (Join-Path $EvidenceDirectory 'driver.json')
# The isolated observer exits itself on success or its finite timeout. Do not terminate it.
if (-not $observer.WaitForExit(180000)) { throw 'Isolated observer did not exit within its bounded acceptance period; no process terminated.' }
$resultDeadline = [DateTime]::UtcNow.AddSeconds(15)
$records = @()
$suspend = @()
$resume = @()
while ($driver.SleepInvoked -and $driver.RequestUtc -and [DateTime]::UtcNow -lt $resultDeadline) {
    $records = @(Read-SelectedPowerEvents $baselineRecord.RecordId)
    $suspend = @($records | Where-Object { $_.Id -eq 42 -and $_.TargetState -eq 4 -and $_.EffectiveState -eq 4 -and [DateTime]$_.TimeUtc -ge $driver.RequestUtc.AddSeconds(-2) })
    $resume = @($records | Where-Object { $_.Id -eq 107 -and $suspend.Count -eq 1 -and $_.RecordId -gt $suspend[0].RecordId })
    if ($suspend.Count -eq 1 -and $resume.Count -eq 1) { break }
    Start-Sleep -Milliseconds 250
}
$records | ConvertTo-Json -Depth 3 | Set-Content -LiteralPath (Join-Path $EvidenceDirectory 'system-power-events.json')
$observerOutput = Get-Content -LiteralPath $stdout -Raw
$positionProof = if ($Taskbar) { '(?m)^NATIVE_POWER_TASKBAR_POSITION_OK: position="' + [regex]::Escape($position) + '" preferences_preserved=true\r?$' } else { '(?!)' }
$taskbarPassed = [bool]$Taskbar -and $observerOutput -match $positionProof -and $observerOutput -match '(?m)^NATIVE_POWER_TASKBAR_OK: .*tokens=3/10 fresh_snapshot=true old_details_hidden=true no_unexpected_actions=true geometry_restored=true physical_input=false computer_restart=false\r?$'
$passed = $driver.SleepInvoked -and $driver.RequestSucceeded -eq $true -and $driver.Reason -eq 'StandbyRequestReturnedNeedsOsEvidence' -and
    $driver.TimerCancelled -eq $true -and $driver.TimerClosed -eq $true -and $driver.PrivilegeRestored -eq $true -and $driver.TokenClosed -eq $true -and
    $driver.TickAfter -ge $driver.TickBefore -and $observer.ExitCode -eq 0 -and $suspend.Count -eq 1 -and $resume.Count -eq 1 -and
    $observerOutput -match '(?m)^NATIVE_POWER_RESUME_OBSERVER_OK: .*totals=3/10 source_readonly=true hidden_main=true authored_messages=false actual_system_transition_requires_os_evidence=true computer_restart=false\r?$' -and
    (-not $Taskbar -or $taskbarPassed)
$result = [pscustomobject]@{
    Schema = 1; Passed = [bool]$passed; Driver = $driver; SystemEvents = $records; ObserverPid = $observer.Id
    ObserverStartUtcTicks = $observerStart; ObserverExecutableSha256 = (Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash.ToLowerInvariant()
    ObserverExitCode = $observer.ExitCode; EvidenceDirectory = $EvidenceDirectory; PhysicalInput = $false; ComputerRestart = $false
    ProductionPackageAcceptance = $false; TaskbarSleepAcceptance = ($passed -and $taskbarPassed); TaskbarPosition = $position; TaskbarHostSha256 = $hostSha256; HardwareWakeSourceProven = $false
}
$result | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $EvidenceDirectory 'result.json')
$result | ConvertTo-Json -Depth 5
if (-not $passed) { exit 1 }
