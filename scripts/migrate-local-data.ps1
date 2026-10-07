param([switch]$CopyOnly, [switch]$ToolsOnly, [switch]$TempOnly, [switch]$SkipRelease, [switch]$SkipTools, [string]$LegacyNpmCache)
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'This migration applies to the existing Windows installation.' }
$migrationRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$migrationLocal = Join-Path $migrationRoot '.local'
$migrationOldLocal = [Environment]::GetFolderPath('LocalApplicationData')
$migrationProfile = [Environment]::GetFolderPath('UserProfile')
$migrationOldTemp = Join-Path $migrationOldLocal 'Temp'
$migrationAllowedSources = @(
    (Join-Path $migrationOldLocal 'com.tokenpulse.desktop'),
    (Join-Path $migrationOldLocal 'com.tokenpulse.desktop.dev'),
    (Join-Path $migrationOldLocal 'TokenPulse'),
    (Join-Path $migrationProfile '.cargo'),
    (Join-Path $migrationProfile '.rustup'),
    (Join-Path $migrationOldLocal 'ms-playwright'),
    (Join-Path $migrationOldLocal 'tauri'),
    (Join-Path $migrationProfile '.tokenpulse\release-signing')
)
if ($LegacyNpmCache) {
    if (-not [IO.Path]::IsPathFullyQualified($LegacyNpmCache)) { throw 'Legacy npm cache must be an explicit absolute path.' }
    $LegacyNpmCache = [IO.Path]::GetFullPath($LegacyNpmCache).TrimEnd('\')
    if ($LegacyNpmCache -eq [IO.Path]::GetPathRoot($LegacyNpmCache).TrimEnd('\') -or
        $LegacyNpmCache.StartsWith($migrationRoot, [StringComparison]::OrdinalIgnoreCase)) { throw 'Legacy npm source must be a separate cache directory.' }
    if ((Test-Path -LiteralPath $LegacyNpmCache) -and -not (Test-Path -LiteralPath (Join-Path $LegacyNpmCache '_cacache') -PathType Container)) { throw 'Legacy source is not an npm cache.' }
    $migrationAllowedSources += $LegacyNpmCache
}
if ([IO.Path]::GetPathRoot($migrationRoot) -in @('C:\', ($env:SystemDrive + '\'))) { throw 'Migration destination must be outside the system drive.' }
[IO.Directory]::CreateDirectory($migrationLocal) | Out-Null

function Get-MigrationHash([string]$Path) {
    $stream = [IO.File]::Open($Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete)
    try { [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($stream)) }
    finally { $stream.Dispose() }
}

function Move-TokenPulseOwnedPath([string]$Source, [string]$Destination, [switch]$Private) {
    if (-not (Test-Path -LiteralPath $Source)) { return }
    $sourcePath = (Get-Item -LiteralPath $Source -Force).FullName
    $destinationPath = [IO.Path]::GetFullPath($Destination)
    $localPrefix = [IO.Path]::GetFullPath($migrationLocal).TrimEnd('\') + '\'
    $toolsPrefix = [IO.Path]::GetFullPath((Join-Path $migrationRoot 'target\.tauri')).TrimEnd('\') + '\'
    $temporaryPrefix = [IO.Path]::GetFullPath($migrationOldTemp).TrimEnd('\') + '\'
    $sourceAllowed = $sourcePath -in $migrationAllowedSources -or
        ($sourcePath.StartsWith($temporaryPrefix, [StringComparison]::OrdinalIgnoreCase) -and
         [IO.Path]::GetDirectoryName($sourcePath).Equals($temporaryPrefix.TrimEnd('\'), [StringComparison]::OrdinalIgnoreCase) -and
         [IO.Path]::GetFileName($sourcePath) -match '^token[-_.]?pulse')
    if (-not $sourceAllowed -or -not ($destinationPath.StartsWith($localPrefix, [StringComparison]::OrdinalIgnoreCase) -or $destinationPath.Equals($toolsPrefix.TrimEnd('\'), [StringComparison]::OrdinalIgnoreCase) -or $destinationPath.StartsWith($toolsPrefix, [StringComparison]::OrdinalIgnoreCase))) {
        throw 'Migration path is outside its explicit source/destination scope.'
    }
    $sourceItem = Get-Item -LiteralPath $sourcePath -Force
    if ($sourceItem.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Redirected migration source refused: $sourcePath" }
    # Inspect the destination's existing ancestors before any copy or recursive removal.
    $ancestor = $destinationPath
    while (-not (Test-Path -LiteralPath $ancestor)) { $ancestor = Split-Path $ancestor -Parent }
    if ((Get-Item -LiteralPath $ancestor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Redirected migration destination refused.' }
    $sourceFiles = if ($sourceItem.PSIsContainer) { @(Get-ChildItem -LiteralPath $sourcePath -File -Recurse -Force) } else { @($sourceItem) }
    if ($sourcePath -eq (Join-Path $migrationProfile '.cargo')) {
        # Cargo's live lock/metadata files are regenerated at the new cache location.
        $sourceFiles = @($sourceFiles | Where-Object { $_.DirectoryName -ne $sourcePath -or $_.Name -notmatch '^\.(package-cache|global-cache)' })
    }
    if ($sourceItem.PSIsContainer -and (Get-ChildItem -LiteralPath $sourcePath -Recurse -Force | Where-Object { $_.Attributes -band [IO.FileAttributes]::ReparsePoint })) { throw 'Migration source contains a reparse point.' }
    [IO.Directory]::CreateDirectory($(if ($sourceItem.PSIsContainer) { $destinationPath } else { Split-Path $destinationPath -Parent })) | Out-Null
    if ($sourceItem.PSIsContainer) {
        foreach ($directory in Get-ChildItem -LiteralPath $sourcePath -Directory -Recurse -Force) {
            $targetDirectory = Join-Path $destinationPath ([IO.Path]::GetRelativePath($sourcePath, $directory.FullName))
            if ((Test-Path -LiteralPath $targetDirectory) -and ((Get-Item -LiteralPath $targetDirectory -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Redirected destination directory refused.' }
            [IO.Directory]::CreateDirectory($targetDirectory) | Out-Null
        }
    }
    foreach ($file in $sourceFiles) {
        $target = if ($sourceItem.PSIsContainer) { Join-Path $destinationPath ([IO.Path]::GetRelativePath($sourcePath, $file.FullName)) } else { $destinationPath }
        [IO.Directory]::CreateDirectory((Split-Path $target -Parent)) | Out-Null
        if (Test-Path -LiteralPath $target) {
            if ((Get-Item -LiteralPath $target -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Redirected destination file refused.' }
            if ((Get-Item -LiteralPath $target).Length -ne $file.Length -or (Get-MigrationHash $target) -ne (Get-MigrationHash $file.FullName)) {
                throw "Migration destination already contains different data: $target"
            }
        } else {
            Copy-Item -LiteralPath $file.FullName -Destination $target
            if ((Get-MigrationHash $target) -ne (Get-MigrationHash $file.FullName)) { throw 'Migration copy verification failed.' }
        }
    }
    if ($Private) {
        # Keep protected owner/DACL on signing keys and notify capabilities; never print contents.
        if ($sourceItem.PSIsContainer) {
            Set-Acl -LiteralPath $destinationPath -AclObject (Get-Acl -LiteralPath $sourcePath)
            foreach ($item in Get-ChildItem -LiteralPath $sourcePath -Recurse -Force) {
                Set-Acl -LiteralPath (Join-Path $destinationPath ([IO.Path]::GetRelativePath($sourcePath, $item.FullName))) -AclObject (Get-Acl -LiteralPath $item.FullName)
            }
        } else { Set-Acl -LiteralPath $destinationPath -AclObject (Get-Acl -LiteralPath $sourcePath) }
    }
    # The exact resolved path was checked against the allowlist above. Never delete a parent.
    if (-not $CopyOnly) { Remove-Item -LiteralPath $sourcePath -Recurse -Force }
    Write-Host ("LOCAL_DATA_" + $(if ($CopyOnly) { 'COPIED' } else { 'MOVED' }) + ": $sourcePath -> $destinationPath ($($sourceFiles.Count) files)")
}

if (-not $ToolsOnly -and -not $TempOnly) {
    if (Get-Process -Name token-pulse-desktop,token-pulse-taskbar-host -ErrorAction SilentlyContinue) { throw 'Close TokenPulse before migrating its database and installation.' }
    if (-not $SkipRelease) {
        $releaseDatabase = Join-Path $migrationOldLocal 'com.tokenpulse.desktop\token-pulse.db'
        if (Test-Path -LiteralPath $releaseDatabase) {
            # Also detects repair utilities and embedded SQLite readers, not just the GUI process.
            $databaseGuard = [IO.File]::Open($releaseDatabase, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::None)
            $databaseGuard.Dispose()
        }
    }
    $applicationEntries = @(@('com.tokenpulse.desktop.dev','data\dev'), @('TokenPulse','app'))
    if (-not $SkipRelease) { $applicationEntries = ,@('com.tokenpulse.desktop','data\release') + $applicationEntries }
    foreach ($entry in $applicationEntries) {
        Move-TokenPulseOwnedPath (Join-Path $migrationOldLocal $entry[0]) (Join-Path $migrationLocal $entry[1]) -Private
    }
    Move-TokenPulseOwnedPath (Join-Path $migrationProfile '.tokenpulse\release-signing') (Join-Path $migrationLocal 'secrets\release-signing') -Private
}
if (-not $ToolsOnly) {
    foreach ($entry in @(Get-ChildItem -LiteralPath $migrationOldTemp -Force | Where-Object Name -match '^token[-_.]?pulse')) {
        Move-TokenPulseOwnedPath $entry.FullName (Join-Path $migrationLocal ('tmp\migrated\' + $entry.Name))
    }
    foreach ($fixture in @(Get-ChildItem -LiteralPath $migrationOldTemp -Directory -Filter '.tmp*' -Force)) {
        if (-not (Test-Path -LiteralPath (Join-Path $fixture.FullName 'token-pulse.db') -PathType Leaf)) { continue }
        $children = @(Get-ChildItem -LiteralPath $fixture.FullName -Force)
        # Rust tempfile directories have random names. Only accept a direct child containing
        # exclusively this project's named SQLite database and its WAL/SHM companions.
        if ($children | Where-Object { $_.PSIsContainer -or $_.Name -notmatch '^token-pulse\.db(?:-wal|-shm)?$' }) { continue }
        if (-not $CopyOnly) {
            $fixtureGuard = [IO.File]::Open((Join-Path $fixture.FullName 'token-pulse.db'), [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::None)
            $fixtureGuard.Dispose()
        }
        $migrationAllowedSources += $fixture.FullName
        Move-TokenPulseOwnedPath $fixture.FullName (Join-Path $migrationLocal ('tmp\migrated\rust-fixtures\' + $fixture.Name))
    }
    $oldSigningParent = [IO.Path]::GetFullPath((Join-Path $migrationProfile '.tokenpulse'))
    if (-not $CopyOnly -and (Test-Path -LiteralPath $oldSigningParent)) {
        $parent = Get-Item -LiteralPath $oldSigningParent -Force
        if ($parent.FullName -ne (Join-Path $migrationProfile '.tokenpulse') -or
            ($parent.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Unexpected old signing parent.' }
        if (-not @(Get-ChildItem -LiteralPath $oldSigningParent -Force).Count) {
            # Exact legacy directory only; nonrecursive removal never deletes remaining keys.
            Remove-Item -LiteralPath $oldSigningParent -Force
            Write-Host "LOCAL_EMPTY_DIRECTORY_REMOVED: $oldSigningParent"
        }
    }
}
if ($SkipTools -or $TempOnly) { return }
if (-not $CopyOnly -and (Get-Process -Name cargo,rustc,rustup,rustfmt,clippy-driver -ErrorAction SilentlyContinue | Where-Object { $_.Path -and ($_.Path.StartsWith((Join-Path $migrationProfile '.cargo'), [StringComparison]::OrdinalIgnoreCase) -or $_.Path.StartsWith((Join-Path $migrationProfile '.rustup'), [StringComparison]::OrdinalIgnoreCase)) })) { throw 'Rust tools are still running from the old directories; their source directories have not been removed.' }
foreach ($entry in @(@((Join-Path $migrationProfile '.cargo'),'tools\cargo'), @((Join-Path $migrationProfile '.rustup'),'tools\rustup'), @((Join-Path $migrationOldLocal 'ms-playwright'),'cache\playwright'))) {
    Move-TokenPulseOwnedPath $entry[0] (Join-Path $migrationLocal $entry[1])
}
Move-TokenPulseOwnedPath (Join-Path $migrationOldLocal 'tauri') (Join-Path $migrationRoot 'target\.tauri')
if ($LegacyNpmCache) {
    # Keep old mutable index/log files separate from the active npm cache.
    Move-TokenPulseOwnedPath $LegacyNpmCache (Join-Path $migrationLocal 'cache\npm\legacy')
}
