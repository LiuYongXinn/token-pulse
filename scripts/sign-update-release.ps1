param([string]$Installer)
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'Local release signing requires Windows.' }
$signingRepository = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$signingVersion = (Get-Content -LiteralPath (Join-Path $signingRepository 'package.json') -Raw | ConvertFrom-Json).version
if (-not $Installer) { $Installer = Join-Path $signingRepository "target\release\bundle\nsis\TokenPulse_${signingVersion}_x64-setup.exe" }
$Installer = (Resolve-Path -LiteralPath $Installer).Path
if ([IO.Path]::GetFileName($Installer) -notin @("TokenPulse_${signingVersion}_x64-setup.exe", "TokenPulse_${signingVersion}_arm64-setup.exe")) {
    throw 'Installer name must match the project version and supported target.'
}
if (Test-Path -LiteralPath ($Installer + '.sig')) { throw 'Signature already exists; replacement refused.' }
$signingRoot = Join-Path $env:USERPROFILE '.tokenpulse\release-signing'
$signingMetadata = Get-Content -LiteralPath (Join-Path $signingRoot 'active.json') -Raw | ConvertFrom-Json
if ($signingMetadata.schema -ne 1 -or $signingMetadata.key_directory -notmatch '^[a-f0-9]{32}$') { throw 'Local signing registration invalid.' }
$signingDirectory = Join-Path $signingRoot $signingMetadata.key_directory
$signingKey = Join-Path $signingDirectory 'tokenpulse-update.key'
$signingPasswordFile = Join-Path $signingDirectory 'password.dpapi'
foreach ($path in @($signingRoot, $signingDirectory, $signingKey, $signingPasswordFile, ($signingKey + '.pub'), $Installer)) {
    if ((Get-Item -LiteralPath $path).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Reparse points are not accepted for local signing inputs.' }
}
$signingUser = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$signingRootAcl = Get-Acl -LiteralPath $signingRoot
if (-not $signingRootAcl.AreAccessRulesProtected -or $signingRootAcl.Owner -ne [Security.Principal.WindowsIdentity]::GetCurrent().Name) {
    throw 'Local signing directory must have protected current-user ownership.'
}
foreach ($rule in $signingRootAcl.Access) {
    if ($rule.AccessControlType -eq 'Allow' -and $rule.IdentityReference.Translate([Security.Principal.SecurityIdentifier]).Value -notin @($signingUser, 'S-1-5-18')) {
        throw 'Local signing directory grants access beyond its owner and SYSTEM.'
    }
}
foreach ($path in @($signingKey, $signingPasswordFile, ($signingKey + '.pub'))) {
    $item = Get-Item -LiteralPath $path
    if ($item.PSIsContainer -or $item.Length -le 0 -or $item.Length -gt 16384) { throw 'Signing input is not a bounded regular file.' }
    $acl = Get-Acl -LiteralPath $path
    foreach ($rule in $acl.Access) {
        if ($rule.AccessControlType -eq 'Allow' -and $rule.IdentityReference.Translate([Security.Principal.SecurityIdentifier]).Value -notin @($signingUser, 'S-1-5-18')) {
            throw 'Signing input grants access beyond its owner and SYSTEM.'
        }
    }
}
$signingPublic = [IO.File]::ReadAllText($signingKey + '.pub').Trim()
$trackedPublic = [IO.File]::ReadAllText((Join-Path $signingRepository 'src-tauri\resources\updater-public-key.txt')).Trim()
$signingFingerprint = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($signingPublic))).ToLowerInvariant()
if ($signingPublic -ne $trackedPublic -or $signingFingerprint -ne $signingMetadata.public_key_sha256) { throw 'Local signing key differs from the project public key.' }
$signingInstallerItem = Get-Item -LiteralPath $Installer
if ($signingInstallerItem.PSIsContainer -or $signingInstallerItem.Length -le 0 -or $signingInstallerItem.Length -gt 536870912) { throw 'Installer is not a bounded regular file.' }
$signingInstallerHash = (Get-FileHash -LiteralPath $Installer -Algorithm SHA256).Hash
$signingPassword = ConvertTo-SecureString ([IO.File]::ReadAllText($signingPasswordFile))
$signingProcess = $null
try {
    $signingStart = [Diagnostics.ProcessStartInfo]::new((Get-Command node).Source)
    $signingStart.UseShellExecute = $false
    $signingStart.CreateNoWindow = $true
    $signingStart.RedirectStandardOutput = $true
    $signingStart.RedirectStandardError = $true
    foreach ($argument in @((Join-Path $signingRepository 'node_modules\@tauri-apps\cli\tauri.js'), 'signer', 'sign', '--app-version', $signingVersion, $Installer)) {
        $signingStart.ArgumentList.Add($argument)
    }
    $signingStart.Environment['TAURI_SIGNING_PRIVATE_KEY_PATH'] = $signingKey
    $signingStart.Environment.Remove('TAURI_SIGNING_PRIVATE_KEY') | Out-Null
    $signingStart.Environment['TAURI_SIGNING_PRIVATE_KEY_PASSWORD'] = [Net.NetworkCredential]::new('', $signingPassword).Password
    $signingProcess = [Diagnostics.Process]::Start($signingStart)
    $signingOut = $signingProcess.StandardOutput.ReadToEndAsync()
    $signingErr = $signingProcess.StandardError.ReadToEndAsync()
    if (-not $signingProcess.WaitForExit(60000)) { $signingProcess.Kill($true); throw 'Signing timed out; CLI output withheld.' }
    $null = $signingOut.GetAwaiter().GetResult()
    $null = $signingErr.GetAwaiter().GetResult()
    if ($signingProcess.ExitCode -ne 0) { throw 'Signing failed; CLI output withheld.' }
    if ((Get-FileHash -LiteralPath $Installer -Algorithm SHA256).Hash -ne $signingInstallerHash) { throw 'Installer changed during signing; signature must not be published.' }
    $signatureFile = Get-Item -LiteralPath ($Installer + '.sig')
    if ($signatureFile.Length -le 0 -or $signatureFile.Length -gt 16384) { throw 'Signature output is invalid.' }
    Write-Host "UPDATE_RELEASE_SIGNED: version=$signingVersion public_key_sha256=$signingFingerprint installer_sha256=$($signingInstallerHash.ToLowerInvariant())"
} finally {
    $signingPassword.Dispose()
    if ($null -ne $signingProcess) { $signingProcess.Dispose() }
    if ($null -ne $signingStart) { $signingStart.Environment.Remove('TAURI_SIGNING_PRIVATE_KEY_PASSWORD') | Out-Null }
}
