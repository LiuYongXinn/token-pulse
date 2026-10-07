# Dot-source before any tool/child process. Environment changes are scoped to this process.
$tokenPulseRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if ([IO.Path]::GetPathRoot($tokenPulseRoot) -in @('C:\', ($env:SystemDrive + '\'))) {
    throw 'TokenPulse requires a project directory outside the system drive.'
}
$tokenPulseLocal = Join-Path $tokenPulseRoot '.local'
$tokenPulseEnvironment = @{
    TOKENPULSE_PROJECT_ROOT = $tokenPulseRoot
    CARGO_HOME = (Join-Path $tokenPulseLocal 'tools\cargo')
    RUSTUP_HOME = (Join-Path $tokenPulseLocal 'tools\rustup')
    npm_config_cache = (Join-Path $tokenPulseLocal 'cache\npm')
    PLAYWRIGHT_BROWSERS_PATH = (Join-Path $tokenPulseLocal 'cache\playwright')
    TEMP = (Join-Path $tokenPulseLocal 'tmp')
    TMP = (Join-Path $tokenPulseLocal 'tmp')
    TMPDIR = (Join-Path $tokenPulseLocal 'tmp')
    PYTHONPYCACHEPREFIX = (Join-Path $tokenPulseLocal 'cache\python')
}
foreach ($tokenPulseEntry in $tokenPulseEnvironment.GetEnumerator()) {
    [IO.Directory]::CreateDirectory($tokenPulseEntry.Value) | Out-Null
    [Environment]::SetEnvironmentVariable($tokenPulseEntry.Key, $tokenPulseEntry.Value, 'Process')
}
$env:PATH = (Join-Path $env:CARGO_HOME 'bin') + ';' + $env:PATH
