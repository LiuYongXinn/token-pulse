param([switch]$PhysicalInput, [Parameter(Mandatory)][string]$BaselineExecutable)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'project-env.ps1')
if (-not $IsWindows -or [IntPtr]::Size -ne 8) { throw 'This tray acceptance requires a 64-bit Windows process.' }
. (Join-Path $PSScriptRoot 'installer-window-probe.ps1')
$application = @(Get-Process token-pulse-desktop -ErrorAction Stop)
$installedExecutable = Join-Path $tokenPulseLocal 'app\token-pulse-desktop.exe'
if ($application.Count -ne 1 -or $application[0].Path -ne $installedExecutable) { throw 'Unexpected installed application identity.' }
if (-not [InstallerWindowProbe]::NsIsBinaryMatches((Resolve-Path -LiteralPath $BaselineExecutable).Path, $installedExecutable)) { throw 'Installed executable does not match the explicit release baseline.' }
$probeRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$lockText = Get-Content -LiteralPath (Join-Path $probeRoot 'Cargo.lock') -Raw
$traySource = Get-Content -LiteralPath (Join-Path $probeRoot 'src-tauri\src\lib.rs') -Raw
# The reviewed one-builder path consumes ID 1 in Builder::new and ID 2 in native TrayIcon::new.
# Refuse source/dependency changes instead of guessing arbitrary notification icon IDs.
if ($lockText -notmatch 'name = "tray-icon"\s+version = "0\.25\.1"' -or
    [regex]::Matches($traySource, 'TrayIconBuilder::with_id\("main-tray"\)').Count -ne 1 -or
    [regex]::Matches($traySource, 'TrayIconBuilder::').Count -ne 1 -or $traySource.Contains('.with_guid(')) { throw 'Reviewed tray identity mapping changed.' }
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class InstalledTrayInputProbe {
 [StructLayout(LayoutKind.Sequential)] public struct Rect {public int Left,Top,Right,Bottom;}
 [StructLayout(LayoutKind.Sequential)] struct Icon {public uint Size;public IntPtr Window;public uint Id;public Guid Guid;}
 [StructLayout(LayoutKind.Sequential)] public struct Point {public int X,Y;}
 [StructLayout(LayoutKind.Sequential)] struct Mouse {public int X,Y;public uint Data,Flags,Time;public UIntPtr Extra;}
 [StructLayout(LayoutKind.Explicit,Size=40)] struct Input {[FieldOffset(0)] public uint Type;[FieldOffset(8)] public Mouse Mouse;}
 [DllImport("shell32.dll")] static extern int Shell_NotifyIconGetRect(ref Icon icon,out Rect rect);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern IntPtr FindWindow(string cls,string title);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern IntPtr FindWindowEx(IntPtr parent,IntPtr after,string cls,string title);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr window,StringBuilder name,int length);
 [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window,out uint pid);
 [DllImport("user32.dll")] static extern IntPtr GetAncestor(IntPtr window,uint flags);
 [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr window,out Rect rect);
 [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
 [DllImport("user32.dll")] static extern bool IsWindowEnabled(IntPtr window);
 [DllImport("user32.dll")] static extern IntPtr WindowFromPoint(Point point);
 [DllImport("user32.dll")] static extern bool GetCursorPos(out Point point);
 [DllImport("user32.dll")] static extern bool SetCursorPos(int x,int y);
 [DllImport("user32.dll")] static extern short GetAsyncKeyState(int key);
 [DllImport("user32.dll")] static extern IntPtr GetForegroundWindow();
 [DllImport("user32.dll")] static extern IntPtr OpenInputDesktop(uint flags,bool inherit,uint access);
 [DllImport("user32.dll")] static extern bool CloseDesktop(IntPtr desktop);
 [DllImport("user32.dll")] static extern IntPtr GetThreadDesktop(uint thread);
 [DllImport("kernel32.dll")] static extern uint GetCurrentThreadId();
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern bool GetUserObjectInformation(IntPtr handle,int index,StringBuilder buffer,uint length,out uint needed);
 [DllImport("user32.dll")] static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
 [DllImport("user32.dll")] static extern uint SendInput(uint count,Input[] inputs,int size);
 [DllImport("wtsapi32.dll")] static extern bool WTSQuerySessionInformationW(IntPtr server,int session,int info,out IntPtr data,out uint bytes);
 [DllImport("wtsapi32.dll")] static extern void WTSFreeMemory(IntPtr data);
 static string Class(IntPtr window){var text=new StringBuilder(128);GetClassName(window,text,text.Capacity);return text.ToString();}
 static uint Owner(IntPtr window){uint pid;GetWindowThreadProcessId(window,out pid);return pid;}
 static string DesktopName(IntPtr desktop){var name=new StringBuilder(128);uint needed;if(!GetUserObjectInformation(desktop,2,name,256,out needed))throw new Exception("Desktop name unavailable.");return name.ToString();}
 public static void InputReady(){
  IntPtr info;uint size;if(!WTSQuerySessionInformationW(IntPtr.Zero,-1,25,out info,out size))throw new Exception("Session header unavailable.");
  try{if(size<20||Marshal.ReadInt32(info)!=1||Marshal.ReadInt32(info,12)!=0||Marshal.ReadInt32(info,16)!=1)throw new Exception("Interactive session is locked, inactive or unknown.");}finally{WTSFreeMemory(info);}
  var input=OpenInputDesktop(0,false,1);if(input==IntPtr.Zero)throw new Exception("Input desktop unavailable.");
  try{if(DesktopName(input)!=DesktopName(GetThreadDesktop(GetCurrentThreadId())))throw new Exception("Input desktop mismatch.");}finally{CloseDesktop(input);}
  if(GetForegroundWindow()==IntPtr.Zero)throw new Exception("Foreground window unavailable.");
  foreach(var key in new[]{0x10,0x11,0x12,0x5b,0x5c,1,2,4})if(GetAsyncKeyState(key)<0)throw new Exception("Input key is already held.");
 }
 public static IntPtr Shell(){var root=FindWindow("Shell_TrayWnd",null);if(root==IntPtr.Zero)throw new Exception("Primary taskbar unavailable.");return root;}
 public static uint ShellOwner(IntPtr root){if(Class(root)!="Shell_TrayWnd")throw new Exception("Taskbar changed.");return Owner(root);}
 public static IntPtr Chevron(IntPtr root,uint pid){
  var notify=FindWindowEx(root,IntPtr.Zero,"TrayNotifyWnd",null);
  var button=FindWindowEx(notify,IntPtr.Zero,"Button",null);
  if(notify==IntPtr.Zero||button==IntPtr.Zero||Owner(root)!=pid||Owner(notify)!=pid||Owner(button)!=pid||GetAncestor(button,2)!=root||FindWindowEx(notify,button,"Button",null)!=IntPtr.Zero)throw new Exception("Notification chevron is not unique or owned by this shell.");
  return button;
 }
 public static IntPtr Overflow(uint pid){var window=FindWindow("NotifyIconOverflowWindow",null);return window!=IntPtr.Zero&&Owner(window)==pid?window:IntPtr.Zero;}
 public static int[] Location(IntPtr tray,uint pid){if(Class(tray)!="tray_icon_app"||Owner(tray)!=pid)throw new Exception("Owned tray changed.");var icon=new Icon{Size=(uint)Marshal.SizeOf(typeof(Icon)),Window=tray,Id=2};Rect rect;int status=Shell_NotifyIconGetRect(ref icon,out rect);return new[]{status,rect.Left,rect.Top,rect.Right,rect.Bottom};}
 public static IntPtr EnterCoordinates(){var prior=SetThreadDpiAwarenessContext(new IntPtr(-4));if(prior==IntPtr.Zero)throw new Exception("Physical coordinate context unavailable.");return prior;}
 public static void LeaveCoordinates(IntPtr prior){SetThreadDpiAwarenessContext(prior);}
 static Point saved,last;static bool moved;
 public static void SavePointer(){if(!GetCursorPos(out saved))throw new Exception("Pointer unavailable.");moved=false;}
 static void Click(IntPtr root,uint pid,Point point,string hitClass,bool right){
  InputReady();var hit=WindowFromPoint(point);
  if(hit==IntPtr.Zero||Owner(root)!=pid||Owner(hit)!=pid||GetAncestor(hit,2)!=root||Class(hit)!=hitClass||!IsWindowVisible(hit)||!IsWindowEnabled(hit))throw new Exception("Physical target ownership or hit check failed.");
  if(!SetCursorPos(point.X,point.Y))throw new Exception("Pointer movement failed.");last=point;moved=true;
  InputReady();if(WindowFromPoint(point)!=hit)throw new Exception("Physical target changed before input.");
  var inputs=new[]{new Input{Mouse=new Mouse{Flags=right?8u:2u}},new Input{Mouse=new Mouse{Flags=right?16u:4u}}};
  uint sent=SendInput(2,inputs,Marshal.SizeOf(typeof(Input)));
  if(sent!=2){if(sent==1)SendInput(1,new[]{inputs[1]},Marshal.SizeOf(typeof(Input)));throw new Exception("Physical mouse input incomplete.");}
 }
 public static void ClickChevron(IntPtr root,uint pid,IntPtr button){
  if(Chevron(root,pid)!=button)throw new Exception("Chevron changed.");Rect rect;
  if(!GetWindowRect(button,out rect)||rect.Right<=rect.Left||rect.Bottom<=rect.Top)throw new Exception("Chevron rectangle unavailable.");
  Click(root,pid,new Point{X=rect.Left+(rect.Right-rect.Left)/2,Y=rect.Top+(rect.Bottom-rect.Top)/2},"Button",false);
 }
 public static void RightClickIcon(IntPtr root,uint shellPid,IntPtr tray,uint appPid){
  var location=Location(tray,appPid);
  if(location[0]!=0||location[3]<=location[1]||location[4]<=location[2])throw new Exception("Owned icon has no S_OK rectangle; physical input refused.");
  Click(root,shellPid,new Point{X=location[1]+(location[3]-location[1])/2,Y=location[2]+(location[4]-location[2])/2},"ToolbarWindow32",true);
 }
 public static void RestorePointer(){Point now;if(moved&&GetCursorPos(out now)&&now.X==last.X&&now.Y==last.Y){InputReady();SetCursorPos(saved.X,saved.Y);}}
}
'@
$appPid = [uint32]$application[0].Id
$tray = [InstallerWindowProbe]::Find($appPid, 'tray')
$shell = [InstalledTrayInputProbe]::Shell()
$shellPid = [InstalledTrayInputProbe]::ShellOwner($shell)
if ((Get-Process -Id $shellPid).Path -ne (Join-Path $env:WINDIR 'explorer.exe')) { throw 'Unexpected Explorer identity.' }
$priorDpi = [InstalledTrayInputProbe]::EnterCoordinates()
$openedOverflow = $false
$pointerSaved = $false
$menuAttempted = $false
$evidence = $null
try {
    $location = [InstalledTrayInputProbe]::Location($tray, $appPid)
    if (-not $PhysicalInput) {
        $rect = if ($location[0] -in @(0,1) -and $location[3] -gt $location[1] -and $location[4] -gt $location[2]) { $location[1..4] } else { $null }
        [pscustomobject]@{TrayHRESULT=$location[0];Rect=$rect;PhysicalInput=$false}
        return
    }
    if ([InstallerWindowProbe]::Find($appPid, 'menu') -ne [IntPtr]::Zero) { throw 'An application menu is already open.' }
    [InstalledTrayInputProbe]::InputReady()
    [InstalledTrayInputProbe]::SavePointer()
    $pointerSaved = $true
    $targetRoot = $shell
    $chevron = [IntPtr]::Zero
    if ($location[0] -eq 1) {
        $overflow = [InstalledTrayInputProbe]::Overflow($shellPid)
        if ($overflow -ne [IntPtr]::Zero -and [InstalledTrayInputProbe]::IsWindowVisible($overflow)) { throw 'Notification overflow is already open.' }
        $chevron = [InstalledTrayInputProbe]::Chevron($shell, $shellPid)
        [InstalledTrayInputProbe]::ClickChevron($shell, $shellPid, $chevron)
        $openedOverflow = $true
        Wait-InstallerCondition { $window=[InstalledTrayInputProbe]::Overflow($shellPid); $window -ne [IntPtr]::Zero -and [InstalledTrayInputProbe]::IsWindowVisible($window) } 'Notification overflow did not open.' 5
        $targetRoot = [InstalledTrayInputProbe]::Overflow($shellPid)
    }
    $menuAttempted = $true
    [InstalledTrayInputProbe]::RightClickIcon($targetRoot, $shellPid, $tray, $appPid)
    Wait-InstallerCondition { [InstallerWindowProbe]::Find($appPid, 'menu') -ne [IntPtr]::Zero } 'Physical right-click did not open the installed tray menu.' 5
    $ids = [InstallerWindowProbe]::MenuIds($appPid, [InstallerWindowProbe]::Find($appPid, 'menu'))
    $evidence = [pscustomobject]@{Result='INSTALLED_TRAY_PHYSICAL_MENU_OK';AppPid=$appPid;InitialHRESULT=$location[0];ExpandedOverflow=$openedOverflow;MenuIds=$ids;QuitInvoked=$false;CleanupVerified=$true}
} finally {
    try {
        if ($menuAttempted) {
            [InstallerWindowProbe]::Post($appPid, $tray, 0x001f, 0, 0)
            Wait-InstallerCondition { [InstallerWindowProbe]::Find($appPid, 'menu') -eq [IntPtr]::Zero } 'Owned menu did not close.' 5
        }
        if ($openedOverflow) {
            $overflow = [InstalledTrayInputProbe]::Overflow($shellPid)
            if ($overflow -ne [IntPtr]::Zero -and [InstalledTrayInputProbe]::IsWindowVisible($overflow)) {
                [InstalledTrayInputProbe]::ClickChevron($shell, $shellPid, $chevron)
                Wait-InstallerCondition { -not [InstalledTrayInputProbe]::IsWindowVisible($overflow) } 'Owned overflow attempt did not close.' 5
            }
        }
        if ($pointerSaved) { [InstalledTrayInputProbe]::RestorePointer() }
    } finally { [InstalledTrayInputProbe]::LeaveCoordinates($priorDpi) }
}
if ($null -ne $evidence) { $evidence }
