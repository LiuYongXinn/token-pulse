param([string]$Compiler)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'project-env.ps1')
if (-not $Compiler) { $Compiler = Join-Path $tokenPulseRoot 'target\.tauri\NSIS\makensis.exe' }
if (-not $IsWindows) { throw 'Windows is required.' }
if (-not (Test-Path -LiteralPath $Compiler -PathType Leaf)) { throw 'Build the NSIS channel first to install its compiler.' }
$probeRoot = [IO.Path]::GetFullPath((Join-Path ([IO.Path]::GetTempPath()) ('tokenpulse-update-hook-' + [guid]::NewGuid().ToString())))
$tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
if (-not $probeRoot.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase) -or -not ([IO.Path]::GetFileName($probeRoot)).StartsWith('tokenpulse-update-hook-')) { throw 'Invalid owned fixture path.' }
[IO.Directory]::CreateDirectory($probeRoot) | Out-Null
$ownedParent = $null; $ownedInstaller = $null
try {
    $hook = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\src-tauri\windows\update-hooks.nsh'))
    $fixture = @'
Unicode true
RequestExecutionLevel user
Name "TokenPulse isolated hook fixture"
InstallDir "$EXEDIR"
OutFile "hook-fixture.exe"
!include "LogicLib.nsh"
!include "FileFunc.nsh"
!include "UPDATE_HOOK"
Section
  FileOpen $9 "$EXEDIR\entered" w
  FileWrite $9 "entered"
  FileClose $9
  !insertmacro NSIS_HOOK_PREINSTALL
  FileOpen $9 "$EXEDIR\finished" w
  FileWrite $9 "finished"
  FileClose $9
SectionEnd
'@
    $source = Join-Path $probeRoot 'fixture.nsi'
    [IO.File]::WriteAllText($source, $fixture.Replace('UPDATE_HOOK', $hook), [Text.UTF8Encoding]::new($false))
    & $Compiler '/V2' $source
    if ($LASTEXITCODE -ne 0) { throw 'Fixture NSIS compilation failed.' }
    $parentSource = Join-Path $probeRoot 'parent.ps1'
    [IO.File]::WriteAllText($parentSource, @'
param([string]$Root)
[IO.File]::WriteAllText((Join-Path $Root 'parent-ready'),'ready')
$until=[DateTime]::UtcNow.AddSeconds(40)
while(-not (Test-Path -LiteralPath (Join-Path $Root 'release'))) {
  if([DateTime]::UtcNow -gt $until) { exit 2 }
  Start-Sleep -Milliseconds 30
}
exit 0
'@, [Text.UTF8Encoding]::new($false))
    $ownedParent = Start-Process -FilePath (Get-Command pwsh).Source -ArgumentList @('-NoProfile', '-File', ('"' + $parentSource + '"'), '-Root', ('"' + $probeRoot + '"')) -WindowStyle Hidden -PassThru
    $null = $ownedParent.Handle
    $until = [DateTime]::UtcNow.AddSeconds(10)
    while (-not (Test-Path -LiteralPath (Join-Path $probeRoot 'parent-ready'))) { if ([DateTime]::UtcNow -gt $until) { throw 'Owned parent not ready.' }; Start-Sleep -Milliseconds 30 }
    $ownedInstaller = Start-Process -FilePath (Join-Path $probeRoot 'hook-fixture.exe') -ArgumentList @('/S', ("/TOKENPULSE_PARENT=" + $ownedParent.Id)) -WindowStyle Hidden -PassThru
    $null = $ownedInstaller.Handle
    $until = [DateTime]::UtcNow.AddSeconds(10)
    while (-not (Test-Path -LiteralPath (Join-Path $probeRoot 'entered'))) { if ([DateTime]::UtcNow -gt $until) { throw 'Owned hook not entered.' }; Start-Sleep -Milliseconds 30 }
    # Let the installer enter the wait. This is a lifecycle assertion, no benchmark.
    Start-Sleep -Milliseconds 300
    if ($ownedParent.HasExited -or $ownedInstaller.HasExited -or (Test-Path -LiteralPath (Join-Path $probeRoot 'finished'))) { throw 'Installer did not wait for the live parent.' }
    [IO.File]::WriteAllText((Join-Path $probeRoot 'release'), 'release')
    if (-not $ownedParent.WaitForExit(10000) -or $ownedParent.ExitCode -ne 0) { throw 'Owned parent did not exit normally.' }
    if (-not $ownedInstaller.WaitForExit(10000) -or $ownedInstaller.ExitCode -ne 0 -or -not (Test-Path -LiteralPath (Join-Path $probeRoot 'finished'))) { throw 'Hook did not continue after normal parent exit.' }
    # The production installer can reach the hook after ordinary application exit.
    # Keep the old parent handle first (OpenProcess must return a signalled object),
    # then release it (OpenProcess must report ERROR_INVALID_PARAMETER). Both races
    # have independent finished markers, so the live-parent check cannot mask them.
    $exitedParentId = $ownedParent.Id
    foreach ($retainedHandle in @($true, $false)) {
        if (-not $retainedHandle) { $ownedParent.Dispose(); $ownedParent = $null }
        Remove-Item -LiteralPath (Join-Path $probeRoot 'entered'), (Join-Path $probeRoot 'finished')
        $ownedInstaller.Dispose()
        $ownedInstaller = Start-Process -FilePath (Join-Path $probeRoot 'hook-fixture.exe') -ArgumentList @('/S', ("/TOKENPULSE_PARENT=" + $exitedParentId)) -WindowStyle Hidden -PassThru
        $null = $ownedInstaller.Handle
        if (-not $ownedInstaller.WaitForExit(10000) -or $ownedInstaller.ExitCode -ne 0 -or -not (Test-Path -LiteralPath (Join-Path $probeRoot 'finished'))) { throw ('Hook did not accept already-exited parent; retained handle=' + $retainedHandle) }
    }
    Write-Output 'NATIVE_UPDATE_HOOK_OK: real NSIS waited for live parent, accepted already-exited parent with/without retained handle; no product install, registry, data or taskbar changes'
} finally {
    foreach ($process in @($ownedInstaller, $ownedParent)) { if ($null -ne $process) { if (-not $process.HasExited) { Stop-Process -InputObject $process -Force }; $process.Dispose() } }
    # The resolved UUID directory was checked above against the explicit temporary root.
    Remove-Item -LiteralPath $probeRoot -Recurse -Force
}
