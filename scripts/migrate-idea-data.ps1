# Run only after the user has chosen to relocate this IDEA instance and closed it.
param([switch]$CopyOnly, [string]$ProductDirectory = 'IntelliJIdea2025.3')
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'project-env.ps1')
if (-not $IsWindows) { throw 'Windows IDEA migration only.' }
if ($ProductDirectory -notmatch '^(IntelliJIdea|IdeaIC)\d{4}\.\d+$') { throw 'Unexpected IDEA directory name.' }
if (Get-Process -Name idea64 -ErrorAction SilentlyContinue) { throw 'Close IDEA normally before migrating its data.' }
# Reuse the scoped copy, SHA-256 verification, reparse refusal and exact-source removal.
. (Join-Path $PSScriptRoot 'migrate-local-data.ps1') -ToolsOnly -SkipTools -CopyOnly:$CopyOnly
$ideaOldSystem = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) ('JetBrains\' + $ProductDirectory)
$ideaOldConfig = Join-Path ([Environment]::GetFolderPath('ApplicationData')) ('JetBrains\' + $ProductDirectory)
$ideaSystem = Join-Path $tokenPulseLocal ('cache\idea\' + $ProductDirectory + '\system')
$ideaConfig = Join-Path $tokenPulseLocal ('config\idea\' + $ProductDirectory)
$ideaProperties = Join-Path $ideaConfig 'idea.properties'
$migrationAllowedSources += @($ideaOldSystem, $ideaOldConfig)
Move-TokenPulseOwnedPath $ideaOldSystem $ideaSystem
# Settings include project task contexts, plugins and credential files. Preserve their ACLs.
Move-TokenPulseOwnedPath $ideaOldConfig $ideaConfig -Private
if ($CopyOnly) { return }
[IO.Directory]::CreateDirectory($ideaConfig) | Out-Null
$ideaPropertyText = if (Test-Path -LiteralPath $ideaProperties) { [IO.File]::ReadAllText($ideaProperties) } else { '' }
$ideaNewline = if ($ideaPropertyText.Contains("`r`n")) { "`r`n" } else { "`n" }
$ideaDirectories = [ordered]@{
    'idea.config.path' = $ideaConfig
    'idea.system.path' = $ideaSystem
    'idea.plugins.path' = (Join-Path $ideaConfig 'plugins')
    'idea.log.path' = (Join-Path $ideaSystem 'log')
    'java.io.tmpdir' = (Join-Path $tokenPulseLocal 'tmp\idea')
}
foreach ($property in $ideaDirectories.GetEnumerator()) {
    [IO.Directory]::CreateDirectory($property.Value) | Out-Null
    $propertyLine = $property.Key + '=' + $property.Value.Replace('\', '/')
    $propertyPattern = '(?m)^[ \t]*' + [regex]::Escape($property.Key) + '[ \t]*=.*$'
    if ([regex]::IsMatch($ideaPropertyText, $propertyPattern)) {
        $literalReplacement = $propertyLine.Replace('$', '$$')
        $ideaPropertyText = [regex]::Replace($ideaPropertyText, $propertyPattern, $literalReplacement)
    } else {
        if ($ideaPropertyText -and -not $ideaPropertyText.EndsWith("`n")) { $ideaPropertyText += $ideaNewline }
        $ideaPropertyText += $propertyLine + $ideaNewline
    }
}
[IO.File]::WriteAllText($ideaProperties, $ideaPropertyText, [Text.UTF8Encoding]::new($false))
[Environment]::SetEnvironmentVariable('IDEA_PROPERTIES', $ideaProperties, 'User')
[Environment]::SetEnvironmentVariable('IDEA_PROPERTIES', $ideaProperties, 'Process')
# Existing desktop/start-menu launchers receive the changed user environment too.
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class TokenPulseIdeaEnvironment {
    [DllImport("user32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    public static extern IntPtr SendMessageTimeout(IntPtr hwnd, uint message, UIntPtr wparam,
        string lparam, uint flags, uint timeout, out UIntPtr result);
}
'@
$broadcastResult = [UIntPtr]::Zero
[TokenPulseIdeaEnvironment]::SendMessageTimeout([IntPtr]0xffff, 0x001a, [UIntPtr]::Zero, 'Environment', 2, 2000, [ref]$broadcastResult) | Out-Null
Write-Host "IDEA_LOCAL_STORAGE_READY: $ideaProperties"
