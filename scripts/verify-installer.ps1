param([string]$Installer, [switch]$OwnTrayCommands)
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
# Never replace an installation or launch with someone's existing production settings.
foreach ($existing in @($uninstallKey, $productKey, $dataDirectory, $roamingDirectory,
    'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\TokenPulse',
    'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\TokenPulse',
    (Join-Path ([Environment]::GetFolderPath('DesktopDirectory')) 'TokenPulse.lnk'),
    (Join-Path ([Environment]::GetFolderPath('Programs')) 'TokenPulse.lnk'))) {
    if (Test-Path -LiteralPath $existing) { throw 'Existing TokenPulse installation or production data: acceptance refused.' }
}
if (Get-Process -Name token-pulse-desktop -ErrorAction SilentlyContinue) { throw 'An application process is already running.' }
$acceptanceDirectory = Join-Path ([IO.Path]::GetTempPath()) ('tokenpulse-install-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $acceptanceDirectory | Out-Null
$installDirectory = Join-Path $acceptanceDirectory 'app'
$appFile = Join-Path $installDirectory 'token-pulse-desktop.exe'
$hostFile = Join-Path $installDirectory 'token-pulse-taskbar-host.exe'
$uninstallerFile = Join-Path $installDirectory 'uninstall.exe'
$noticesFile = Join-Path $installDirectory 'THIRD_PARTY_NOTICES.txt'

Add-Type -AssemblyName UIAutomationClient
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class InstallerWindowProbe {
    public static bool NsIsBinaryMatches(string releaseFile, string installedFile) {
        var expected = System.IO.File.ReadAllBytes(releaseFile);
        var actual = System.IO.File.ReadAllBytes(installedFile);
        if (expected.Length != actual.Length) return false;
        // tauri-bundler restores the unpatched build output after packaging. The installed
        // NSIS binary differs only at this documented, fixed-width bundle-kind marker.
        var token = Encoding.ASCII.GetBytes("__TAURI_BUNDLE_TYPE_VAR_UNK");
        var replacement = Encoding.ASCII.GetBytes("__TAURI_BUNDLE_TYPE_VAR_NSS");
        int marker = -1;
        for (int i = 0; i <= expected.Length - token.Length; i++) {
            if (expected[i] != token[0]) continue;
            bool match = true;
            for (int j = 1; j < token.Length; j++) if (expected[i+j] != token[j]) { match = false; break; }
            if (match) { if (marker != -1) return false; marker = i; }
        }
        if (marker < 0) return false;
        Array.Copy(replacement, 0, expected, marker, replacement.Length);
        for (int i = 0; i < expected.Length; i++) if (expected[i] != actual[i]) return false;
        return true;
    }
    public delegate bool EnumProc(IntPtr window, IntPtr parameter);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc callback, IntPtr parameter);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr window, StringBuilder name, int capacity);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetWindowText(IntPtr window, StringBuilder name, int capacity);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] static extern bool PostMessage(IntPtr window, uint message, UIntPtr wparam, IntPtr lparam);
    [DllImport("user32.dll")] static extern IntPtr SendMessageTimeout(IntPtr window, uint message, UIntPtr wparam, IntPtr lparam, uint flags, uint timeout, out UIntPtr result);
    [DllImport("user32.dll")] static extern int GetMenuItemCount(IntPtr menu);
    [DllImport("user32.dll")] static extern uint GetMenuItemID(IntPtr menu, int position);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetMenuString(IntPtr menu, uint item, StringBuilder text, int capacity, uint flags);
    public static IntPtr Find(uint pid, string kind) {
        var found = new List<IntPtr>();
        EnumWindows((window, unused) => {
            uint owner; GetWindowThreadProcessId(window, out owner);
            if (owner != pid) return true;
            var cls = new StringBuilder(128); GetClassName(window, cls, cls.Capacity);
            var title = new StringBuilder(128); GetWindowText(window, title, title.Capacity);
            if ((kind == "tray" && cls.ToString() == "tray_icon_app") ||
                (kind == "menu" && cls.ToString() == "#32768" && IsWindowVisible(window)) ||
                (kind == "main" && title.ToString() == "TokenPulse") ||
                (kind == "mini" && title.ToString() == "TokenPulse · 悬浮窗")) found.Add(window);
            return true;
        }, IntPtr.Zero);
        return found.Count == 1 ? found[0] : IntPtr.Zero;
    }
    public static void Post(uint pid, IntPtr window, uint message, uint wparam, long lparam) {
        uint owner; GetWindowThreadProcessId(window, out owner);
        if (window == IntPtr.Zero || owner != pid) throw new Exception("Refuse message outside owned application window.");
        if (!PostMessage(window, message, new UIntPtr(wparam), new IntPtr(lparam))) throw new Exception("Owned window message failed.");
    }
    public static uint[] MenuIds(uint pid, IntPtr window) {
        uint owner; GetWindowThreadProcessId(window, out owner);
        if (window == IntPtr.Zero || owner != pid) throw new Exception("Refuse menu outside owned application.");
        UIntPtr raw;
        if (SendMessageTimeout(window, 0x01e1, UIntPtr.Zero, IntPtr.Zero, 2, 1000, out raw) == IntPtr.Zero) throw new Exception("Owned menu unavailable.");
        var menu = new IntPtr(unchecked((long)raw.ToUInt64()));
        if (GetMenuItemCount(menu) != 3) throw new Exception("Unexpected production tray menu.");
        var expected = new [] { "打开统计", "显示悬浮窗 / 恢复交互", "退出 TokenPulse" };
        var ids = new uint[3];
        for (int i = 0; i < 3; i++) {
            var text = new StringBuilder(128); GetMenuString(menu, (uint)i, text, text.Capacity, 0x400);
            if (text.ToString() != expected[i]) throw new Exception("Unexpected tray action.");
            ids[i] = GetMenuItemID(menu, i);
            if (ids[i] == 0xffffffff) throw new Exception("Invalid menu action.");
        }
        return ids;
    }
}
'@
function Wait-InstallerCondition([scriptblock]$Condition, [string]$Failure, [int]$Seconds = 20) {
    $deadline = [DateTime]::UtcNow.AddSeconds($Seconds)
    while ([DateTime]::UtcNow -lt $deadline) {
        if (& $Condition) { return }
        Start-Sleep -Milliseconds 150
    }
    throw $Failure
}
function Read-ProductionMenu([System.Diagnostics.Process]$Application) {
    Wait-InstallerCondition { [InstallerWindowProbe]::Find($Application.Id, 'tray') -ne [IntPtr]::Zero } 'Production tray did not initialize.'
    if ($OwnTrayCommands) {
        # Explicit reduced acceptance when the interactive desktop cannot display a menu.
        # Pinned muda 0.20.0 starts IDs at 1000; these three items are created before Menu.
        # Require the reviewed source ordering and lockfile; never infer arbitrary IDs.
        $menuSource = Get-Content -LiteralPath (Join-Path $installerRoot 'src-tauri\src\lib.rs') -Raw
        $menuOrder = [regex]::Matches($menuSource, 'MenuItem::with_id\(app, "([^"]+)"') | ForEach-Object { $_.Groups[1].Value }
        $lockSource = Get-Content -LiteralPath (Join-Path $installerRoot 'Cargo.lock') -Raw
        if (($menuOrder -join ',') -ne 'open,mini,quit' -or [regex]::Matches($menuSource, 'Menu::').Count -ne 1 -or $lockSource -notmatch 'name = "muda"\s+version = "0\.20\.0"') { throw 'Production tray command mapping changed; reduced acceptance refused.' }
        return @(1000, 1001, 1002)
    }
    $tray = [InstallerWindowProbe]::Find($Application.Id, 'tray')
    # Version-pinned tray-icon 0.25.1 notification callback; authored own-window messages,
    # not physical mouse evidence. Menu contents are independently checked before activation.
    [InstallerWindowProbe]::Post($Application.Id, $tray, 6002, 0, 0x0205)
    Wait-InstallerCondition { [InstallerWindowProbe]::Find($Application.Id, 'menu') -ne [IntPtr]::Zero } 'Production tray menu did not open.'
    $menu = [InstallerWindowProbe]::Find($Application.Id, 'menu')
    $ids = [InstallerWindowProbe]::MenuIds($Application.Id, $menu)
    [InstallerWindowProbe]::Post($Application.Id, $tray, 0x001f, 0, 0)
    Wait-InstallerCondition { [InstallerWindowProbe]::Find($Application.Id, 'menu') -eq [IntPtr]::Zero } 'Production tray menu did not close.'
    return $ids
}
function Exit-ProductionApplication([System.Diagnostics.Process]$Application) {
    # Hold the native process handle before exit so an externally observed process also
    # retains its actual exit status. Missing status must never be accepted as zero.
    $null = $Application.Handle
    $ids = Read-ProductionMenu $Application
    [InstallerWindowProbe]::Post($Application.Id, [InstallerWindowProbe]::Find($Application.Id, 'tray'), 0x0111, $ids[2], 0)
    if (-not $Application.WaitForExit(20000)) { throw 'Production application did not complete normal tray exit.' }
    $Application.Refresh()
    if ($Application.ExitCode -ne 0) { throw 'Production application exit failed.' }
}

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
