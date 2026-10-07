# Refresh an already migrated installation from compiled binaries; never builds an installer.
param()
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'project-env.ps1')
if (-not $IsWindows) { throw 'Windows local installation only.' }
if (Get-Process -Name token-pulse-desktop,token-pulse-taskbar-host -ErrorAction SilentlyContinue) { throw 'Close TokenPulse before refreshing its binaries.' }
$localApp = Join-Path $tokenPulseLocal 'app'
$localDatabase = Join-Path $tokenPulseLocal 'data\release\token-pulse.db'
if (-not (Test-Path -LiteralPath $localDatabase)) { throw 'Migrate the existing database before refreshing the local installation.' }
$builtApp = Join-Path $tokenPulseRoot 'target\release\token-pulse-desktop.exe'
$installedApp = Join-Path $localApp 'token-pulse-desktop.exe'
$builtHost = Join-Path $tokenPulseRoot 'target\release\token-pulse-taskbar-host.exe'
if (-not (Test-Path -LiteralPath (Join-Path $localApp 'uninstall.exe'))) { throw 'This script only refreshes an existing migrated NSIS installation.' }
$bytes = [IO.File]::ReadAllBytes($builtApp)
$token = [Text.Encoding]::ASCII.GetBytes('__TAURI_BUNDLE_TYPE_VAR_UNK')
$replacement = [Text.Encoding]::ASCII.GetBytes('__TAURI_BUNDLE_TYPE_VAR_NSS')
$matches = @()
for ($index = 0; $index -le $bytes.Length - $token.Length; $index++) {
    if ($bytes[$index] -ne $token[0]) { continue }
    $same = $true
    for ($offset = 1; $offset -lt $token.Length; $offset++) { if ($bytes[$index + $offset] -ne $token[$offset]) { $same = $false; break } }
    if ($same) { $matches += $index }
}
if ($matches.Count -ne 1) { throw 'Expected one Tauri bundle marker in the compiled desktop binary.' }
# Preserve the existing NSIS installation identity exactly as tauri-bundler does.
[Array]::Copy($replacement, 0, $bytes, $matches[0], $replacement.Length)
[IO.File]::WriteAllBytes($installedApp, $bytes)
if (Test-Path -LiteralPath $builtHost) { Copy-Item -LiteralPath $builtHost -Destination (Join-Path $localApp 'token-pulse-taskbar-host.exe') -Force }
. (Join-Path $PSScriptRoot 'installer-window-probe.ps1')
if (-not [InstallerWindowProbe]::NsIsBinaryMatches($builtApp, $installedApp)) { throw 'Local binary differs beyond the exact NSIS marker.' }

$oldApp = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'TokenPulse'
foreach ($key in @('HKCU:\Software\tokenpulse\TokenPulse', 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\TokenPulse')) {
    if (-not (Test-Path -LiteralPath $key)) { continue }
    $item = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($key.Substring('HKCU:\'.Length), $true)
    foreach ($name in $item.GetValueNames()) {
        $value = $item.GetValue($name)
        if ($value -is [string] -and $value.Contains($oldApp)) { $item.SetValue($name, $value.Replace($oldApp, $localApp), $item.GetValueKind($name)) }
    }
    $item.Dispose()
}
foreach ($directory in @([Environment]::GetFolderPath('DesktopDirectory'), [Environment]::GetFolderPath('Programs'))) {
    foreach ($shortcut in @(Get-ChildItem -LiteralPath $directory -Filter '*TokenPulse*.lnk' -File -Recurse -ErrorAction SilentlyContinue)) {
        $shell = New-Object -ComObject WScript.Shell
        $link = $shell.CreateShortcut($shortcut.FullName)
        if ($link.TargetPath.StartsWith($oldApp, [StringComparison]::OrdinalIgnoreCase)) {
            $link.TargetPath = $link.TargetPath.Replace($oldApp, $localApp)
            $link.WorkingDirectory = $localApp
            if ($link.IconLocation) { $link.IconLocation = $link.IconLocation.Replace($oldApp, $localApp) }
            $link.Save()
        }
        [Runtime.InteropServices.Marshal]::FinalReleaseComObject($link) | Out-Null
        [Runtime.InteropServices.Marshal]::FinalReleaseComObject($shell) | Out-Null
    }
}
# Only owned notify commands are edited. Source sessions and other Codex settings stay intact.
& python -B (Join-Path $PSScriptRoot 'relocate-notify.py') $oldApp $localApp (Join-Path $tokenPulseLocal 'data\release\notify')
if ($LASTEXITCODE -ne 0) { throw 'Owned notify command relocation failed.' }
Write-Host "LOCAL_APP_REFRESHED: $installedApp (no installer generated)"
