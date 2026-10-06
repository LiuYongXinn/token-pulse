param(
    [Parameter(Mandatory)][uint32]$ApplicationId,
    [Parameter(Mandatory)][string]$BaselineExecutable,
    [Parameter(Mandatory)][string]$PythonExecutable,
    [switch]$Exercise
)
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'Windows native acceptance only.' }
$repository = Split-Path $PSScriptRoot -Parent
. (Join-Path $PSScriptRoot 'installer-window-probe.ps1')
$installedPath = Join-Path $env:LOCALAPPDATA 'TokenPulse/token-pulse-desktop.exe'
$application = Get-Process -Id $ApplicationId -ErrorAction Stop
$applicationStart = $application.StartTime.ToUniversalTime()
$baseline = (Resolve-Path -LiteralPath $BaselineExecutable).Path
if ($application.Path -ne $installedPath -or -not [InstallerWindowProbe]::NsIsBinaryMatches($baseline, $installedPath)) {
    throw 'Installed process does not match the explicit NSIS baseline.'
}
$python = (Resolve-Path -LiteralPath $PythonExecutable).Path
$databasePath = Join-Path $env:LOCALAPPDATA 'com.tokenpulse.desktop/token-pulse.db'
$readSettings = @'
import hashlib, json, pathlib, sqlite3, sys
db = sqlite3.connect(pathlib.Path(sys.argv[1]).resolve().as_uri() + '?mode=ro', uri=True)
db.execute('PRAGMA query_only=ON')
db.execute('BEGIN')
payload = json.loads(db.execute('SELECT payload_json FROM settings WHERE singleton=1').fetchone()[0])
revision = db.execute('SELECT settings_revision FROM app_state WHERE singleton=1').fetchone()[0]
windows = {key: json.loads(json.dumps(payload.get(key))) for key in ('main_window', 'mini_window')}
for key in windows:
    if isinstance(payload.get(key), dict): payload[key].pop('placement', None)
digest = hashlib.sha256(json.dumps(payload, sort_keys=True, separators=(',', ':')).encode()).hexdigest()
print(json.dumps({'revision': revision, 'windows': windows, 'non_placement_sha256': digest}))
db.rollback()
db.close()
'@
function Read-PlacementSettings {
    $raw = & $python -c $readSettings $databasePath
    if ($LASTEXITCODE -ne 0) { throw 'Read-only settings snapshot failed.' }
    return ($raw | ConvertFrom-Json -Depth 12)
}
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public static class InstalledMonitorProbe {
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
    [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)] public struct Monitor {
        public uint Size; public Rect Bounds, Work; public uint Flags;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst=32)] public string Device;
    }
    public class Window {
        public Rect Outer; public int ClientWidth, ClientHeight; public uint Dpi;
        public string Monitor; public bool Visible, Minimized, Maximized;
    }
    delegate bool MonitorProc(IntPtr monitor, IntPtr dc, ref Rect rect, IntPtr data);
    [DllImport("user32.dll")] static extern bool EnumDisplayMonitors(IntPtr dc, IntPtr clip, MonitorProc callback, IntPtr data);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern bool GetMonitorInfo(IntPtr monitor, ref Monitor info);
    [DllImport("user32.dll")] static extern IntPtr MonitorFromWindow(IntPtr window, uint flags);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
    [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr window, out Rect rect);
    [DllImport("user32.dll")] static extern bool GetClientRect(IntPtr window, out Rect rect);
    [DllImport("user32.dll")] static extern uint GetDpiForWindow(IntPtr window);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] static extern bool IsIconic(IntPtr window);
    [DllImport("user32.dll")] static extern bool IsZoomed(IntPtr window);
    [DllImport("user32.dll")] static extern bool ShowWindow(IntPtr window, int command);
    [DllImport("user32.dll", SetLastError=true)] static extern bool SetWindowPos(IntPtr window, IntPtr after, int x, int y, int width, int height, uint flags);
    [DllImport("user32.dll", SetLastError=true)] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    static void Own(IntPtr window, uint expected) {
        uint pid; GetWindowThreadProcessId(window, out pid);
        if (window==IntPtr.Zero || pid!=expected) throw new Exception("Owned window identity changed.");
    }
    static Monitor Info(IntPtr handle) {
        var info = new Monitor { Size=(uint)Marshal.SizeOf(typeof(Monitor)) };
        if(handle==IntPtr.Zero || !GetMonitorInfo(handle, ref info)) throw new Exception("Monitor information unavailable.");
        return info;
    }
    public static Monitor[] Monitors() {
        var result=new List<Monitor>();
        MonitorProc callback=(IntPtr handle,IntPtr dc,ref Rect rect,IntPtr data)=>{result.Add(Info(handle));return true;};
        if(!EnumDisplayMonitors(IntPtr.Zero,IntPtr.Zero,callback,IntPtr.Zero)) throw new Exception("Monitor enumeration failed.");
        return result.ToArray();
    }
    public static Window Read(IntPtr window, uint expected) {
        Own(window, expected);
        Rect outer, client;
        if(!GetWindowRect(window,out outer)||!GetClientRect(window,out client)) throw new Exception("Window measurement failed.");
        var dpi=GetDpiForWindow(window); if(dpi==0) throw new Exception("Unknown window DPI.");
        return new Window { Outer=outer,ClientWidth=client.Right-client.Left,ClientHeight=client.Bottom-client.Top,
            Dpi=dpi,Monitor=Info(MonitorFromWindow(window,0)).Device,Visible=IsWindowVisible(window),
            Minimized=IsIconic(window),Maximized=IsZoomed(window) };
    }
    public static void Move(IntPtr window,uint expected,int x,int y) {
        Own(window,expected);
        if(!SetWindowPos(window,IntPtr.Zero,x,y,0,0,0x0015)) throw new Exception("Owned move failed.");
    }
    public static void Restore(IntPtr window,uint expected,Rect original) {
        Own(window,expected);
        if(!SetWindowPos(window,IntPtr.Zero,original.Left,original.Top,original.Right-original.Left,original.Bottom-original.Top,0x0014))
            throw new Exception("Owned geometry restoration failed.");
    }
    public static void ShowState(IntPtr window,uint expected,int command) { Own(window,expected); ShowWindow(window,command); }
}
'@
function Assert-ApplicationIdentity {
    $current = Get-Process -Id $ApplicationId -ErrorAction Stop
    if ($current.Path -ne $installedPath -or $current.StartTime.ToUniversalTime() -ne $applicationStart) { throw 'Application process changed.' }
}
function Invoke-OwnedButton([IntPtr]$Window, [string]$Name) {
    Assert-ApplicationIdentity
    $root = [System.Windows.Automation.AutomationElement]::FromHandle($Window)
    $condition = [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty, $Name)
    $buttons = @($root.FindAll([System.Windows.Automation.TreeScope]::Descendants, $condition) | Where-Object {
        $_.Current.IsEnabled -and $_.GetSupportedPatterns().Id -contains [System.Windows.Automation.InvokePattern]::Pattern.Id
    })
    if ($buttons.Count -ne 1) { throw "Owned button not unique: $Name" }
    $buttons[0].GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
}
$prior = [InstalledMonitorProbe]::SetThreadDpiAwarenessContext([IntPtr](-4))
if ($prior -eq [IntPtr]::Zero) { throw 'Inspector physical coordinate context unavailable.' }
try {
    $monitors = @([InstalledMonitorProbe]::Monitors())
    $main = [InstallerWindowProbe]::Find($ApplicationId, 'main')
    $mini = [InstallerWindowProbe]::Find($ApplicationId, 'mini')
    $originalMain = [InstalledMonitorProbe]::Read($main, $ApplicationId)
    $originalMini = [InstalledMonitorProbe]::Read($mini, $ApplicationId)
    $before = Read-PlacementSettings
    if (-not $Exercise) {
        [pscustomobject]@{ Scope='Read-only preflight; no window moves'; ApplicationId=$ApplicationId; StartUtc=$applicationStart.ToString('o');
            Monitors=$monitors; Main=$originalMain; Mini=$originalMini; SettingsRevision=$before.revision } | ConvertTo-Json -Depth 8
        return
    }
    if ($monitors.Count -lt 2 -or @($monitors.Device | Select-Object -Unique).Count -ne $monitors.Count) { throw 'Two distinct active monitor surfaces required.' }
    if (-not $originalMain.Visible -or $originalMain.Minimized) { throw 'Visible non-minimized main window required.' }
    if ($before.windows.mini_window.passthrough) { throw 'Do not alter an existing passthrough preference.' }
    $initialExpanded = [bool]$before.windows.mini_window.interaction.expanded
    $receiptDirectory = Join-Path $repository ('target/native-cross-monitor/' + [guid]::NewGuid().ToString())
    [void][IO.Directory]::CreateDirectory($receiptDirectory)
    $samples = [Collections.Generic.List[object]]::new()
    $failure = $null
    $cleanupFailure = $null
    $normalMain = $originalMain
    function Check-Placement([string]$Kind, [IntPtr]$Window, $Monitor, [bool]$Expanded, [bool]$RequireFit=$true) {
        $deadline = [DateTime]::UtcNow.AddSeconds(10)
        do {
            Assert-ApplicationIdentity
            $measurement = [InstalledMonitorProbe]::Read($Window, $ApplicationId)
            $settings = Read-PlacementSettings
            $saved = $settings.windows.($Kind + '_window').placement
            $scale = $measurement.Dpi / 96.0
            $fits = $measurement.Outer.Left -ge $Monitor.Work.Left -and $measurement.Outer.Top -ge $Monitor.Work.Top -and
                $measurement.Outer.Right -le $Monitor.Work.Right -and $measurement.Outer.Bottom -le $Monitor.Work.Bottom
            $savedMatches = $saved.monitor -eq $Monitor.Device -and
                [Math]::Abs($saved.offset_x_dip * $scale - ($measurement.Outer.Left - $Monitor.Work.Left)) -le 1 -and
                [Math]::Abs($saved.offset_y_dip * $scale - ($measurement.Outer.Top - $Monitor.Work.Top)) -le 1
            $sizeMatches = $Kind -eq 'main' -or ($measurement.ClientWidth -eq [Math]::Round($(if ($Expanded) {360} else {280}) * $scale) -and
                $measurement.ClientHeight -eq [Math]::Round($(if ($Expanded) {380} else {220}) * $scale))
            if ($measurement.Visible -and $measurement.Monitor -eq $Monitor.Device -and ($fits -or -not $RequireFit) -and $savedMatches -and $sizeMatches) {
                return [pscustomobject]@{Kind=$Kind;Expanded=$Expanded;CapturedUtc=[DateTime]::UtcNow.ToString('o');Window=$measurement;
                    Saved=$saved;SettingsRevision=$settings.revision;WorkArea=$Monitor.Work}
            }
            Start-Sleep -Milliseconds 150
        } while ([DateTime]::UtcNow -lt $deadline)
        throw "Actual monitor, full window fit, client size or persisted placement did not converge: $Kind / $($Monitor.Device)"
    }
    try {
        if ($originalMain.Maximized) {
            [InstalledMonitorProbe]::ShowState($main,$ApplicationId,9)
            Wait-InstallerCondition { -not [InstalledMonitorProbe]::Read($main,$ApplicationId).Maximized } 'Main did not restore to ordinary state.'
            $normalMain = [InstalledMonitorProbe]::Read($main,$ApplicationId)
        }
        if (-not $originalMini.Visible) {
            Invoke-OwnedButton $main '显示悬浮窗'
            Wait-InstallerCondition { [InstallerWindowProbe]::IsWindowVisible($mini) } 'Mini not shown.'
        }
        foreach ($monitor in ($monitors | Sort-Object Device -Descending)) {
            # Use actual current monitor surfaces; do not simulate DPI or display messages.
            [InstalledMonitorProbe]::Move($main, $ApplicationId, $monitor.Work.Left + 60, $monitor.Work.Top + 40)
            $samples.Add((Check-Placement 'main' $main $monitor $false))
        }
        foreach ($expanded in @($false, $true)) {
            $current = Read-PlacementSettings
            if ([bool]$current.windows.mini_window.interaction.expanded -ne $expanded) {
                Invoke-OwnedButton $mini $(if ($expanded) {'展开小窗'} else {'收起小窗'})
            }
            foreach ($monitor in ($monitors | Sort-Object Device -Descending)) {
                [InstalledMonitorProbe]::Move($mini, $ApplicationId, $monitor.Work.Left + 180, $monitor.Work.Top + 140)
                $samples.Add((Check-Placement 'mini' $mini $monitor $expanded))
            }
        }
    } catch { $failure = $_.Exception.Message }
    finally {
        $cleanupErrors = [Collections.Generic.List[string]]::new()
        try {
            Assert-ApplicationIdentity
            $current = Read-PlacementSettings
            if ([bool]$current.windows.mini_window.interaction.expanded -ne $initialExpanded) {
                Invoke-OwnedButton $mini $(if ($initialExpanded) {'展开小窗'} else {'收起小窗'})
            }
        } catch { $cleanupErrors.Add($_.Exception.Message) }
        try {
            Assert-ApplicationIdentity
            [InstalledMonitorProbe]::Restore($main, $ApplicationId, $normalMain.Outer)
            $mainMonitor = @($monitors | Where-Object Device -eq $normalMain.Monitor)[0]
            # Restore the user's original ordinary rectangle even when that original
            # rectangle crossed the work-area edge; all six exercised samples require fit.
            $restoredMain = Check-Placement 'main' $main $mainMonitor $false $false
            if (($normalMain.Outer | ConvertTo-Json -Compress) -ne ($restoredMain.Window.Outer | ConvertTo-Json -Compress)) { throw 'Original main geometry not restored.' }
        } catch { $cleanupErrors.Add($_.Exception.Message) }
        try {
            Assert-ApplicationIdentity
            [InstalledMonitorProbe]::Restore($mini, $ApplicationId, $originalMini.Outer)
            $miniMonitor = @($monitors | Where-Object Device -eq $originalMini.Monitor)[0]
            $restoredMini = Check-Placement 'mini' $mini $miniMonitor $initialExpanded
            if (($originalMini.Outer | ConvertTo-Json -Compress) -ne ($restoredMini.Window.Outer | ConvertTo-Json -Compress)) { throw 'Original mini geometry not restored.' }
        } catch { $cleanupErrors.Add($_.Exception.Message) }
        try {
            if (-not $originalMini.Visible) {
                Invoke-OwnedButton $mini '隐藏小窗'
                Wait-InstallerCondition { -not [InstallerWindowProbe]::IsWindowVisible($mini) } 'Mini visibility not restored.'
            }
        } catch { $cleanupErrors.Add($_.Exception.Message) }
        try {
            Assert-ApplicationIdentity
            if ($originalMain.Maximized) {
                [InstalledMonitorProbe]::ShowState($main,$ApplicationId,3)
                Wait-InstallerCondition { [InstalledMonitorProbe]::Read($main,$ApplicationId).Maximized } 'Main maximized state not restored.'
            }
        } catch { $cleanupErrors.Add($_.Exception.Message) }
        try {
            $after = Read-PlacementSettings
            if ($after.non_placement_sha256 -ne $before.non_placement_sha256) { throw 'A non-placement preference changed.' }
            if (-not [InstallerWindowProbe]::NsIsBinaryMatches($baseline, $installedPath)) { throw 'Installed bytes changed.' }
        } catch { $cleanupErrors.Add($_.Exception.Message) }
        if ($cleanupErrors.Count) { $cleanupFailure = $cleanupErrors -join '; ' }
    }
    $receipt = [pscustomobject]@{CapturedUtc=[DateTime]::UtcNow.ToString('o'); Scope='Installed actual cross-monitor SetWindowPos and application persistence; not physical drag, topology change or other DPI';
        OSVersion=[Environment]::OSVersion.Version.ToString();ApplicationId=$ApplicationId;StartUtc=$applicationStart.ToString('o');BaselineSHA256=(Get-FileHash -LiteralPath $baseline -Algorithm SHA256).Hash;
        Monitors=$monitors;OriginalMain=$originalMain;NormalMain=$normalMain;OriginalMini=$originalMini;BeforeSettings=$before;Samples=$samples;
        RestoredMain=$restoredMain;RestoredMini=$restoredMini;AfterSettings=$after;Failure=$failure;CleanupFailure=$cleanupFailure;
        Success=($null -eq $failure -and $null -eq $cleanupFailure)}
    $output = Join-Path $receiptDirectory 'receipt.json'
    $bytes = [Text.UTF8Encoding]::new($false).GetBytes(($receipt | ConvertTo-Json -Depth 12))
    $file = [IO.File]::Open($output, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::Read)
    try { $file.Write($bytes,0,$bytes.Length) } finally { $file.Dispose() }
    [pscustomobject]@{Receipt=$output;Success=$receipt.Success;Samples=$samples.Count;Failure=$failure;CleanupFailure=$cleanupFailure} | ConvertTo-Json
    if (-not $receipt.Success) { throw 'Cross-monitor acceptance failed; see retained receipt.' }
    Write-Output 'INSTALLED_CROSS_MONITOR_PLACEMENT_OK'
} finally { [void][InstalledMonitorProbe]::SetThreadDpiAwarenessContext($prior) }
