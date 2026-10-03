param(
    [Parameter(Mandatory)][string]$Installer,
    [Parameter(Mandatory)][string]$Signature,
    [Parameter(Mandatory)][string]$Report
)
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'Release artifact verification requires Windows.' }
$verificationRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$verifier = (Resolve-Path -LiteralPath (Join-Path $verificationRoot 'target\release\token-pulse-desktop.exe')).Path
$Installer = (Resolve-Path -LiteralPath $Installer).Path
$Signature = (Resolve-Path -LiteralPath $Signature).Path
$Report = [IO.Path]::GetFullPath($Report)
if (Test-Path -LiteralPath $Report) { throw 'Release verification report already exists.' }
if (-not (Test-Path -LiteralPath ([IO.Path]::GetDirectoryName($Report)) -PathType Container)) { throw 'Release verification report directory does not exist.' }
# Windows GUI-subsystem executables can return control before exiting. Never use the
# caller's LASTEXITCODE as evidence of this maintenance process's result.
$arguments = @('--verify-update-release', ('"' + $Installer + '"'), ('"' + $Signature + '"'), ('"' + $Report + '"'))
$verificationProcess = Start-Process -FilePath $verifier -ArgumentList $arguments -WindowStyle Hidden -PassThru
try {
    if (-not $verificationProcess.WaitForExit(20000)) {
        # This handle belongs only to the maintenance process started above.
        $verificationProcess.Kill()
        $null = $verificationProcess.WaitForExit(5000)
        throw 'Release artifact verification exceeded its deadline.'
    }
    $verificationProcess.Refresh()
    if ($verificationProcess.ExitCode -ne 0) { throw 'Actual release signature/version verification failed.' }
    if (-not (Test-Path -LiteralPath $Report -PathType Leaf)) { throw 'Release verification did not create its report.' }
    Get-Content -LiteralPath $Report -Raw | ConvertFrom-Json
} finally {
    $verificationProcess.Dispose()
}
