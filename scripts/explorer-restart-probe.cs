using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
public static class ExplorerRestartInspection {
    [StructLayout(LayoutKind.Sequential)] public struct FileTime { public uint Low, High; }
    [StructLayout(LayoutKind.Sequential)] public struct UniqueProcess { public uint Pid; public FileTime Start; }
    [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)] public struct ProcessInfo {
        public UniqueProcess Process;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst=256)] public string AppName;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst=64)] public string ServiceName;
        public int Type;
        public uint Status, Session;
        [MarshalAs(UnmanagedType.Bool)] public bool Restartable;
    }
    public sealed class Evidence {
        public uint ShellPid;
        public int SessionId;
        public bool ShellIdentityVerified;
        public bool Eligible;
        public string Reason;
        public int FileWindowCount;
        public int DetachedCloakedFrameCount;
        public Dictionary<string,int> UnknownVisibleClasses = new Dictionary<string,int>();
        public uint? RegisteredProcesses, RebootReasons;
        public bool? Restartable;
        public int? ApplicationType;
        public uint? ListStatus;
        public bool ShutdownInvoked = false, RestartInvoked = false;
    }
    delegate bool EnumCallback(IntPtr window, IntPtr parameter);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern IntPtr FindWindow(string cls,string title);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumCallback callback,IntPtr parameter);
    [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr window,EnumCallback callback,IntPtr parameter);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window,out uint pid);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr window,StringBuilder text,int length);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr window);
    [DllImport("dwmapi.dll")] static extern int DwmGetWindowAttribute(IntPtr window,uint attribute,out uint value,uint size);
    [DllImport("kernel32.dll")] static extern bool GetProcessTimes(IntPtr process,out FileTime created,out FileTime exited,out FileTime kernel,out FileTime user);
    [DllImport("rstrtmgr.dll", CharSet=CharSet.Unicode)] static extern uint RmStartSession(out uint session,uint flags,StringBuilder key);
    [DllImport("rstrtmgr.dll", CharSet=CharSet.Unicode)] static extern uint RmRegisterResources(uint session,uint files,IntPtr paths,uint apps,[In] UniqueProcess[] processes,uint services,IntPtr names);
    [DllImport("rstrtmgr.dll", CharSet=CharSet.Unicode)] static extern uint RmGetList(uint session,out uint needed,ref uint count,[In,Out] ProcessInfo[] processes,out uint reasons);
    [DllImport("rstrtmgr.dll")] static extern uint RmEndSession(uint session);
    static uint Owner(IntPtr window){uint pid;GetWindowThreadProcessId(window,out pid);return pid;}
    static string Class(IntPtr window){var cls=new StringBuilder(128);if(GetClassName(window,cls,cls.Capacity)==0)throw new Exception("Window class unavailable.");return cls.ToString();}
    static bool Same(FileTime a,FileTime b){return a.Low==b.Low&&a.High==b.High;}
    static bool DetachedCloakedFrame(IntPtr window,uint pid) {
        uint cloaked;
        if(Class(window)!="ApplicationFrameWindow" || DwmGetWindowAttribute(window,14,out cloaked,4)!=0 || cloaked!=2)return false;
        int children=0;bool valid=true;Exception failure=null;
        bool enumerated=EnumChildWindows(window,(child,unused)=>{
            try {
                string cls=Class(child);
                if(++children>16 || Owner(child)!=pid || (cls!="ApplicationFrameTitleBarWindow" && cls!="ApplicationFrameInputSinkWindow")){valid=false;return false;}
                return true;
            } catch(Exception error){failure=error;return false;}
        },IntPtr.Zero);
        return enumerated && failure==null && valid && children>=1 && Class(window)=="ApplicationFrameWindow" && Owner(window)==pid && DwmGetWindowAttribute(window,14,out cloaked,4)==0 && cloaked==2;
    }
    public static Evidence Inspect() {
        if(Marshal.SizeOf(typeof(UniqueProcess))!=12 || Marshal.SizeOf(typeof(ProcessInfo))!=668)throw new Exception("Unexpected Restart Manager ABI.");
        var root=FindWindow("Shell_TrayWnd",null);
        if(root==IntPtr.Zero)throw new Exception("Primary taskbar unavailable.");
        uint pid=Owner(root);
        var evidence=new Evidence{ShellPid=pid};
        using(var shell=Process.GetProcessById(checked((int)pid))) using(var caller=Process.GetCurrentProcess()) {
            evidence.SessionId=shell.SessionId;
            var expected=Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.Windows),"explorer.exe");
            if(shell.SessionId!=caller.SessionId || !String.Equals(shell.MainModule.FileName,expected,StringComparison.OrdinalIgnoreCase) || Class(root)!="Shell_TrayWnd")throw new Exception("Unexpected primary Explorer identity.");
            FileTime start,end,kernel,user;
            if(!GetProcessTimes(shell.Handle,out start,out end,out kernel,out user))throw new Exception("Explorer creation time unavailable.");
            evidence.ShellIdentityVerified=true;
            var allowed=new HashSet<string>(StringComparer.Ordinal){"Shell_TrayWnd","Progman","WorkerW","DummyDWMListenerWindow","tooltips_class32"};
            Exception enumerationFailure=null;
            int primaryCount=0;
            bool enumerated=EnumWindows((window,unused)=>{
                try {
                    if(Owner(window)!=pid)return true;
                    string cls=Class(window);
                    if(cls=="Shell_TrayWnd")primaryCount++;
                    if(cls=="CabinetWClass"||cls=="ExploreWClass")evidence.FileWindowCount++;
                    if(IsWindowVisible(window)&&!allowed.Contains(cls)){
                        if(DetachedCloakedFrame(window,pid))evidence.DetachedCloakedFrameCount++;
                        else {int count;evidence.UnknownVisibleClasses.TryGetValue(cls,out count);evidence.UnknownVisibleClasses[cls]=count+1;}
                    }
                    return true;
                } catch(Exception failure) {enumerationFailure=failure;return false;}
            },IntPtr.Zero);
            if(!enumerated||enumerationFailure!=null||primaryCount!=1)throw new Exception("Explorer window inventory is incomplete or primary taskbar is ambiguous.");
            uint session;
            uint result=RmStartSession(out session,0,new StringBuilder(33));
            if(result!=0)throw new Exception("Restart Manager session failed: "+result);
            try {
                result=RmRegisterResources(session,0,IntPtr.Zero,1,new[]{new UniqueProcess{Pid=pid,Start=start}},0,IntPtr.Zero);
                if(result!=0)throw new Exception("Exact Explorer registration failed: "+result);
                uint needed,count=0,reasons;
                result=RmGetList(session,out needed,ref count,null,out reasons);
                if(result==234 && needed>=1 && needed<=8){
                    var processes=new ProcessInfo[needed];count=needed;
                    result=RmGetList(session,out needed,ref count,processes,out reasons);
                    evidence.ListStatus=result;
                    if(result==0){
                        evidence.RegisteredProcesses=count;evidence.RebootReasons=reasons;
                        if(count==1 && processes[0].Process.Pid==pid && Same(processes[0].Process.Start,start) && processes[0].Session==(uint)caller.SessionId){
                            evidence.Restartable=processes[0].Restartable;evidence.ApplicationType=processes[0].Type;
                        }
                    }
                } else {
                    evidence.ListStatus=result;
                    if(result==0){evidence.RegisteredProcesses=count;evidence.RebootReasons=reasons;}
                }
            } finally {uint ended=RmEndSession(session);if(ended!=0)throw new Exception("Restart Manager session release failed: "+ended);}
            shell.Refresh();
            if(shell.HasExited||FindWindow("Shell_TrayWnd",null)!=root||Owner(root)!=pid)throw new Exception("Explorer generation changed during inspection.");
            evidence.Eligible=evidence.FileWindowCount==0 && evidence.UnknownVisibleClasses.Count==0 && evidence.RegisteredProcesses==1 && evidence.RebootReasons==0 && evidence.Restartable==true && evidence.ApplicationType==4;
            evidence.Reason=evidence.Eligible?"EligibleForControlledRestart":evidence.FileWindowCount!=0?"ExplorerFileWindowsPresent":evidence.UnknownVisibleClasses.Count!=0?"UnknownVisibleExplorerWindowsPresent":"RestartManagerEligibilityNotProven";
            return evidence;
        }
    }
}
public static class ExplorerNormalRestart {
 [DllImport("kernel32.dll")] static extern bool GetProcessTimes(IntPtr process,out ExplorerRestartInspection.FileTime created,out ExplorerRestartInspection.FileTime exited,out ExplorerRestartInspection.FileTime kernel,out ExplorerRestartInspection.FileTime user);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern IntPtr FindWindow(string cls,string title);
 [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window,out uint pid);
 [DllImport("rstrtmgr.dll",CharSet=CharSet.Unicode)] static extern uint RmStartSession(out uint session,uint flags,StringBuilder key);
 [DllImport("rstrtmgr.dll",CharSet=CharSet.Unicode)] static extern uint RmRegisterResources(uint session,uint files,IntPtr paths,uint apps,[In] ExplorerRestartInspection.UniqueProcess[] processes,uint services,IntPtr names);
 [DllImport("rstrtmgr.dll",CharSet=CharSet.Unicode)] static extern uint RmGetList(uint session,out uint needed,ref uint count,[In,Out] ExplorerRestartInspection.ProcessInfo[] processes,out uint reasons);
 [DllImport("rstrtmgr.dll")] static extern uint RmShutdown(uint session,uint flags,IntPtr callback);
 [DllImport("rstrtmgr.dll")] static extern uint RmRestart(uint session,uint flags,IntPtr callback);
 [DllImport("rstrtmgr.dll")] static extern uint RmEndSession(uint session);
 public sealed class Result {public uint OldPid;public uint? NewPid,ShutdownStatus,RestartStatus;public bool ShutdownInvoked,RestartInvoked,OldProcessExited;public int? OldExitCode;public bool ForceShutdown=false,ComputerRestart=false;}
 public static uint ShellPid(){var root=FindWindow("Shell_TrayWnd",null);uint pid=0;if(root!=IntPtr.Zero)GetWindowThreadProcessId(root,out pid);return pid;}
 public static Result Run(uint expectedPid){
  var check=ExplorerRestartInspection.Inspect();if(!check.Eligible||check.ShellPid!=expectedPid)throw new Exception("Explorer eligibility changed.");
  var result=new Result{OldPid=expectedPid};
  using(var shell=Process.GetProcessById(checked((int)expectedPid))){
   ExplorerRestartInspection.FileTime start,end,kernel,user;
   if(!GetProcessTimes(shell.Handle,out start,out end,out kernel,out user))throw new Exception("Creation time unavailable.");
   uint session;uint status=RmStartSession(out session,0,new StringBuilder(33));if(status!=0)throw new Exception("Restart Manager start failed: "+status);
   try {
    status=RmRegisterResources(session,0,IntPtr.Zero,1,new[]{new ExplorerRestartInspection.UniqueProcess{Pid=expectedPid,Start=start}},0,IntPtr.Zero);
    if(status!=0)throw new Exception("Exact process registration failed: "+status);
    uint needed,count=2,reasons;var list=new ExplorerRestartInspection.ProcessInfo[2];status=RmGetList(session,out needed,ref count,list,out reasons);
    if(status!=0||count!=1||reasons!=0||list[0].Process.Pid!=expectedPid||list[0].Process.Start.Low!=start.Low||list[0].Process.Start.High!=start.High||!list[0].Restartable||list[0].Type!=4||list[0].Session!=(uint)shell.SessionId)throw new Exception("Restart Manager affected scope changed.");
    check=ExplorerRestartInspection.Inspect();if(!check.Eligible||check.ShellPid!=expectedPid||shell.HasExited)throw new Exception("Explorer changed before normal shutdown.");
    result.ShutdownInvoked=true;
    try {result.ShutdownStatus=RmShutdown(session,0x10,IntPtr.Zero);} // OnlyRegistered; no ForceShutdown.
    finally {result.RestartInvoked=true;result.RestartStatus=RmRestart(session,0,IntPtr.Zero);}
    result.OldProcessExited=shell.WaitForExit(15000);
    if(result.OldProcessExited){shell.Refresh();result.OldExitCode=shell.ExitCode;}
    uint current=ShellPid();if(current!=0&&current!=expectedPid)result.NewPid=current;
   } finally {uint ended=RmEndSession(session);if(ended!=0)throw new Exception("Restart Manager release failed: "+ended);}
  }
  return result;
 }
}
