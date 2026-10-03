param([string]$Installer, [switch]$OwnTrayCommands, [switch]$UseExistingLocalState)
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'Installer acceptance requires Windows.' }
$installerRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$releaseVersion = (Get-Content -LiteralPath (Join-Path $installerRoot 'src-tauri\tauri.conf.json') -Raw | ConvertFrom-Json).version
if (-not $Installer) { $Installer = Join-Path $installerRoot "target\release\bundle\nsis\TokenPulse_${releaseVersion}_x64-setup.exe" }
$Installer = (Resolve-Path -LiteralPath $Installer).Path
$uninstallKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\TokenPulse'
$productKey = 'HKCU:\Software\tokenpulse\TokenPulse'
$dataDirectory = Join-Path $env:LOCALAPPDATA 'com.tokenpulse.desktop'
$roamingDirectory = Join-Path $env:APPDATA 'com.tokenpulse.desktop'
# An existing installation is always refused. Existing data/retained installer state
# require explicit authorization; the normal clean-machine entry remains unchanged.
foreach ($existing in @($uninstallKey,
    'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\TokenPulse',
    'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\TokenPulse',
    (Join-Path ([Environment]::GetFolderPath('DesktopDirectory')) 'TokenPulse.lnk'),
    (Join-Path ([Environment]::GetFolderPath('Programs')) 'TokenPulse.lnk'))) {
    if (Test-Path -LiteralPath $existing) { throw 'Existing TokenPulse installation: acceptance refused.' }
}
foreach ($existing in @($productKey, $dataDirectory, $roamingDirectory)) {
    if ((Test-Path -LiteralPath $existing) -and -not $UseExistingLocalState) { throw 'Existing TokenPulse local state: explicit authorization is required.' }
}
if (Test-Path -LiteralPath $productKey) {
    $retainedLocation = (Get-ItemProperty -LiteralPath $productKey).'(default)'
    if ($retainedLocation -and (Test-Path -LiteralPath (Join-Path $retainedLocation 'token-pulse-desktop.exe'))) { throw 'Retained installation executable exists: acceptance refused.' }
}
if (Get-Process -Name token-pulse-desktop,token-pulse-taskbar-host -ErrorAction SilentlyContinue) { throw 'An application or native host process is already running.' }
$acceptanceDirectory = Join-Path ([IO.Path]::GetTempPath()) ('tokenpulse-install-' + [guid]::NewGuid().ToString('N'))
$acceptanceDirectory = [IO.Path]::GetFullPath($acceptanceDirectory)
$temporaryRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
if (-not $acceptanceDirectory.StartsWith($temporaryRoot, [StringComparison]::OrdinalIgnoreCase) -or -not [IO.Path]::GetFileName($acceptanceDirectory).StartsWith('tokenpulse-install-')) { throw 'Invalid owned acceptance directory.' }
New-Item -ItemType Directory -Path $acceptanceDirectory | Out-Null
$existingFileEvidence = @()
if ($UseExistingLocalState) {
    # Test safeguard only: exact database files, no WebView cookies, credentials,
    # source logs or recursive data copies. Never restore over a changed live database.
    $safeguardDirectory = Join-Path $acceptanceDirectory 'before-test'
    New-Item -ItemType Directory -Path $safeguardDirectory | Out-Null
    foreach ($name in @('token-pulse.db','token-pulse.db-wal','token-pulse.db-shm')) {
        $original = Join-Path $dataDirectory $name
        if (Test-Path -LiteralPath $original -PathType Leaf) {
            if ((Get-Item -LiteralPath $original).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Refuse redirected database safeguard input.' }
            $beforeHash = (Get-FileHash -LiteralPath $original -Algorithm SHA256).Hash
            $copy = Join-Path $safeguardDirectory $name
            Copy-Item -LiteralPath $original -Destination $copy
            if ((Get-FileHash -LiteralPath $original -Algorithm SHA256).Hash -ne $beforeHash -or (Get-FileHash -LiteralPath $copy -Algorithm SHA256).Hash -ne $beforeHash) { throw 'Database changed while preparing the test safeguard.' }
            $existingFileEvidence += [pscustomobject]@{file=$name;sha256=$beforeHash;length=(Get-Item -LiteralPath $copy).Length}
        }
    }
    ConvertTo-Json -InputObject $existingFileEvidence -Depth 3 | Set-Content -LiteralPath (Join-Path $safeguardDirectory 'file-evidence.json') -Encoding utf8
    if (Test-Path -LiteralPath $productKey) {
        & reg.exe export 'HKCU\Software\tokenpulse\TokenPulse' (Join-Path $safeguardDirectory 'retained-installer-state.reg') /y | Out-Null
        if ($LASTEXITCODE -ne 0) { throw 'Retained installer state safeguard failed.' }
    }
    # The actual compiled public key/version verifier runs without initializing the app.
    $signature = $Installer + '.sig'
    if (-not (Test-Path -LiteralPath $signature -PathType Leaf)) { throw 'Signed local-state acceptance requires the installer signature.' }
    $null = & (Join-Path $PSScriptRoot 'verify-release-artifact.ps1') -Installer $Installer -Signature $signature -Report (Join-Path $acceptanceDirectory 'release-verification.json')
    Write-Host ('INSTALLER_EXISTING_STATE_OK: explicit local-state acceptance; database safeguard and release signature verified; evidence=' + $acceptanceDirectory)
}
$installDirectory = Join-Path $acceptanceDirectory 'app'
$appFile = Join-Path $installDirectory 'token-pulse-desktop.exe'
$hostFile = Join-Path $installDirectory 'token-pulse-taskbar-host.exe'
$uninstallerFile = Join-Path $installDirectory 'uninstall.exe'
$noticesFile = Join-Path $installDirectory 'THIRD_PARTY_NOTICES.txt'

. (Join-Path $PSScriptRoot 'installer-window-probe.ps1')

$installation = Start-Process -FilePath $Installer -ArgumentList ('/S /D=' + $installDirectory) -WindowStyle Hidden -PassThru
if (-not $installation.WaitForExit(60000)) { throw 'Installer did not finish within the acceptance deadline.' }
if ($installation.ExitCode -ne 0) { throw 'NSIS installation failed.' }
foreach ($artifact in @($appFile, $hostFile, $uninstallerFile, $noticesFile)) {
    if (-not (Test-Path -LiteralPath $artifact -PathType Leaf)) { throw 'Required installed artifact missing.' }
}
if (-not [InstallerWindowProbe]::NsIsBinaryMatches((Join-Path $installerRoot 'target\release\token-pulse-desktop.exe'), $appFile)) { throw 'Installed application differs beyond the exact NSIS bundle marker.' }
if ((Get-FileHash -LiteralPath $hostFile).Hash -ne (Get-FileHash -LiteralPath (Join-Path $installerRoot 'target\release\token-pulse-taskbar-host.exe')).Hash) { throw 'Installed native host differs from the release artifact.' }
if ((Get-FileHash -LiteralPath $noticesFile).Hash -ne (Get-FileHash -LiteralPath (Join-Path $installerRoot 'src-tauri\resources\third-party-notices.txt')).Hash) { throw 'Installed third-party notices differ from the verified build resource.' }
$registration = Get-ItemProperty -LiteralPath $uninstallKey
if ($registration.InstallLocation.Trim('"') -ne $installDirectory -or $registration.DisplayVersion -ne $releaseVersion) {
    throw 'Current-user installation registration mismatch.'
}
Write-Host 'INSTALLER_FILES_OK: production executable and independent native host match release artifacts; current-user registration'
$application = Start-Process -FilePath $appFile -WorkingDirectory $installDirectory -WindowStyle Hidden -PassThru
Wait-InstallerCondition { [InstallerWindowProbe]::Find($application.Id, 'main') -ne [IntPtr]::Zero } 'Installed main window did not start.'
$main = [InstallerWindowProbe]::Find($application.Id, 'main')
Wait-InstallerCondition {
    $element = [System.Windows.Automation.AutomationElement]::FromHandle($main)
    $condition = [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty, '总览')
    $element.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition) -ne $null
} 'Embedded production frontend did not expose its overview navigation.'
$databaseFile = Join-Path $dataDirectory 'token-pulse.db'
if (-not (Test-Path -LiteralPath $databaseFile -PathType Leaf)) { throw 'Independent production SQLite was not created.' }
$databaseStream = [IO.FileStream]::new($databaseFile, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite)
try {
    $header = [byte[]]::new(16)
    if ($databaseStream.Read($header, 0, 16) -ne 16 -or [Text.Encoding]::ASCII.GetString($header) -ne "SQLite format 3`0") { throw 'Production database is not SQLite.' }
} finally { $databaseStream.Dispose() }
# Closing the main window retains the process and tray; the second executable signals it.
[InstallerWindowProbe]::Post($application.Id, $main, 0x0010, 0, 0)
Wait-InstallerCondition { -not [InstallerWindowProbe]::IsWindowVisible($main) } 'Close did not hide the installed main window.'
if ($application.HasExited) { throw 'Closing statistics unexpectedly exited the application.' }
$second = Start-Process -FilePath $appFile -WorkingDirectory $installDirectory -WindowStyle Hidden -PassThru
if (-not $second.WaitForExit(20000)) { throw 'Installed single-instance invocation did not exit.' }
Wait-InstallerCondition { [InstallerWindowProbe]::IsWindowVisible($main) } 'Second invocation did not reopen existing statistics.'
$actions = Read-ProductionMenu $application
[InstallerWindowProbe]::Post($application.Id, [InstallerWindowProbe]::Find($application.Id, 'tray'), 0x0111, $actions[1], 0)
Wait-InstallerCondition { $mini = [InstallerWindowProbe]::Find($application.Id, 'mini'); $mini -ne [IntPtr]::Zero -and [InstallerWindowProbe]::IsWindowVisible($mini) } 'Installed tray action did not create the mini window.'
Exit-ProductionApplication $application
Write-Host 'INSTALLER_RUNTIME_OK: embedded overview accessible, production SQLite, close-to-tray, single instance, own tray command mini action and normal exit'
if ($OwnTrayCommands) { Write-Host 'INSTALLER_TRAY_COMMAND_ONLY: explicitly reduced acceptance; no visible popup or physical mouse proof' }
# A second full process proves production cold-start persistence rather than one runtime.
$cold = Start-Process -FilePath $appFile -WorkingDirectory $installDirectory -WindowStyle Hidden -PassThru
Wait-InstallerCondition { [InstallerWindowProbe]::Find($cold.Id, 'tray') -ne [IntPtr]::Zero } 'Installed cold startup did not create its tray.'
Exit-ProductionApplication $cold
$dataHash = (Get-FileHash -LiteralPath $databaseFile).Hash
# Normal uninstall defaults to retaining application data. No recursive removal is scripted.
$resolvedInstallDirectory = (Resolve-Path -LiteralPath $installDirectory).Path
if ($resolvedInstallDirectory -ne $installDirectory -or -not $resolvedInstallDirectory.StartsWith($acceptanceDirectory + '\', [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Refuse uninstall outside the exact owned acceptance directory.'
}
$registeredUninstaller = (Get-ItemProperty -LiteralPath $uninstallKey).UninstallString.Trim('"')
if ($registeredUninstaller -ne $uninstallerFile) { throw 'Uninstaller ownership changed.' }
$uninstall = Start-Process -FilePath $uninstallerFile -ArgumentList '/S' -WindowStyle Hidden -PassThru
if (-not $uninstall.WaitForExit(60000) -or $uninstall.ExitCode -ne 0) { throw 'Uninstaller launch failed.' }
Wait-InstallerCondition { -not (Test-Path -LiteralPath $appFile) -and -not (Test-Path -LiteralPath $hostFile) -and -not (Test-Path -LiteralPath $noticesFile) -and -not (Test-Path -LiteralPath $uninstallKey) } 'Normal uninstall did not remove binaries, notices and registration.'
if (-not (Test-Path -LiteralPath $databaseFile) -or (Get-FileHash -LiteralPath $databaseFile).Hash -ne $dataHash) { throw 'Normal uninstall changed production data.' }
if (Get-Process -Name token-pulse-desktop -ErrorAction SilentlyContinue) { throw 'Installed application process remains after normal shutdown.' }
Write-Host 'INSTALLER_UNINSTALL_OK: installed files and registration removed; production SQLite retained unchanged'
Write-Host 'INSTALLER_SEQUENCE_OK'
