$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'TokenPulse installer preparation requires Windows.' }
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    $env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
}
$desktopRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
Push-Location -LiteralPath $desktopRoot
try {
    & npm run build
    if ($LASTEXITCODE -ne 0) { throw 'Frontend production build failed.' }
    $desktopHost = (& rustc --print host-tuple).Trim()
    if ($LASTEXITCODE -ne 0) { throw 'Unable to determine Rust host target.' }
    $desktopTarget = if ($env:TAURI_ENV_TARGET_TRIPLE) { $env:TAURI_ENV_TARGET_TRIPLE } else { $desktopHost }
    if ($desktopTarget -notin @('x86_64-pc-windows-msvc', 'aarch64-pc-windows-msvc')) {
        throw 'Installer target must be a supported Windows MSVC target.'
    }
    & node (Join-Path $PSScriptRoot 'generate-third-party-notices.mjs') --target $desktopTarget
    if ($LASTEXITCODE -ne 0) { throw 'Third-party notices could not be verified.' }
    $desktopProfile = if ($env:TAURI_ENV_DEBUG -eq 'true') { 'debug' } else { 'release' }
    $hostBuildArgs = @('build', '-p', 'token-pulse-taskbar', '--bin', 'token-pulse-taskbar-host')
    if ($desktopProfile -eq 'release') { $hostBuildArgs += '--release' }
    $desktopMetadata = & cargo metadata --no-deps --format-version 1
    if ($LASTEXITCODE -ne 0) { throw 'Unable to locate the actual Cargo artifact directory.' }
    $hostArtifactRoot = ($desktopMetadata | ConvertFrom-Json).target_directory
    if ($desktopTarget -ne $desktopHost) {
        $hostBuildArgs += @('--target', $desktopTarget)
        $hostArtifactRoot = Join-Path $hostArtifactRoot $desktopTarget
    }
    & cargo @hostBuildArgs
    if ($LASTEXITCODE -ne 0) { throw 'Native taskbar host build failed.' }
    $hostArtifact = Join-Path $hostArtifactRoot "$desktopProfile\token-pulse-taskbar-host.exe"
    if (-not (Test-Path -LiteralPath $hostArtifact -PathType Leaf)) { throw 'Native taskbar host artifact is missing.' }
    $sidecarDirectory = Join-Path $desktopRoot 'src-tauri\binaries'
    New-Item -ItemType Directory -Path $sidecarDirectory -Force | Out-Null
    $sidecarArtifact = Join-Path $sidecarDirectory "token-pulse-taskbar-host-$desktopTarget.exe"
    Copy-Item -LiteralPath $hostArtifact -Destination $sidecarArtifact -Force
    if ((Get-FileHash -LiteralPath $hostArtifact -Algorithm SHA256).Hash -ne (Get-FileHash -LiteralPath $sidecarArtifact -Algorithm SHA256).Hash) {
        throw 'Prepared native host does not match the built artifact.'
    }
    Write-Host "DESKTOP_HOST_PREPARED: $desktopTarget / $desktopProfile"
} finally {
    Pop-Location
}
