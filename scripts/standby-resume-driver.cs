// Explicit acceptance driver: standby only; never reboot, shut down or hibernate.
using System;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;

[assembly: DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
public static class TokenPulseStandbyDriver {
    [StructLayout(LayoutKind.Sequential)] struct Luid { public uint Low; public int High; }
    [StructLayout(LayoutKind.Sequential)] struct Privileges { public uint Count; public Luid Id; public uint Attributes; }
    [DllImport("kernel32.dll")] static extern IntPtr GetCurrentProcess();
    [DllImport("advapi32.dll", SetLastError=true)] static extern bool OpenProcessToken(IntPtr process, uint access, out IntPtr token);
    [DllImport("advapi32.dll", CharSet=CharSet.Unicode, SetLastError=true)] static extern bool LookupPrivilegeValueW(string system, string name, out Luid id);
    [DllImport("advapi32.dll", SetLastError=true)] static extern bool GetTokenInformation(IntPtr token, int information, IntPtr buffer, uint length, out uint required);
    [DllImport("advapi32.dll", SetLastError=true)] static extern bool AdjustTokenPrivileges(IntPtr token, bool disableAll, ref Privileges next, uint length, out Privileges previous, out uint required);
    [DllImport("advapi32.dll", EntryPoint="AdjustTokenPrivileges", SetLastError=true)] static extern bool RestoreTokenPrivileges(IntPtr token, bool disableAll, ref Privileges next, uint length, IntPtr previous, IntPtr required);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)] static extern IntPtr CreateWaitableTimerW(IntPtr attributes, bool manualReset, string name);
    [DllImport("kernel32.dll", SetLastError=true)] static extern bool SetWaitableTimer(IntPtr timer, ref long due, int period, IntPtr routine, IntPtr argument, bool resume);
    [DllImport("kernel32.dll", SetLastError=true)] static extern bool CancelWaitableTimer(IntPtr timer);
    [DllImport("kernel32.dll", SetLastError=true)] static extern bool CloseHandle(IntPtr handle);
    [DllImport("kernel32.dll")] static extern void SetLastError(uint error);
    [DllImport("kernel32.dll")] static extern ulong GetTickCount64();
    [DllImport("powrprof.dll", SetLastError=true)] [return: MarshalAs(UnmanagedType.I1)]
    static extern bool SetSuspendState([MarshalAs(UnmanagedType.I1)] bool hibernate, [MarshalAs(UnmanagedType.I1)] bool force, [MarshalAs(UnmanagedType.I1)] bool disableWake);

    public sealed class Evidence {
        public bool? PrivilegePresent, PrivilegeEnabledBefore;
        public bool? PrivilegeEnabledForRequest, PrivilegeRestored, TokenClosed;
        public bool? TimerArmed, TimerCancelled, TimerClosed, ObserverStillRunning, RequestSucceeded;
        public DateTime? WakeDueUtc, RequestUtc, ReturnedUtc;
        public ulong? TickBefore, TickAfter;
        public int? Error;
        public string Reason;
        public bool SleepInvoked=false, HibernateInvoked=false, ComputerRestart=false, PolicyChanged=false;
    }
    static void RequirePlatform() {
        if (!RuntimeInformation.IsOSPlatform(OSPlatform.Windows) || IntPtr.Size!=8 || Marshal.SizeOf<Privileges>()!=16)
            throw new InvalidOperationException("A 64-bit Windows process with the expected privilege structure is required.");
    }
    static Luid ShutdownPrivilege() {
        Luid id;
        if (!LookupPrivilegeValueW(null,"SeShutdownPrivilege",out id)) throw new InvalidOperationException("Standby privilege identity unavailable.");
        return id;
    }
    static int? ErrorCode(int code) { return code==0 ? (int?)null : code; }
    static void InspectToken(IntPtr token, Luid expected, Evidence result) {
        uint required;
        GetTokenInformation(token,3,IntPtr.Zero,0,out required);
        if (Marshal.GetLastWin32Error()!=122 || required<4 || required>65536) throw new InvalidOperationException("Unexpected token privilege buffer.");
        IntPtr buffer=Marshal.AllocHGlobal(checked((int)required));
        try {
            if (!GetTokenInformation(token,3,buffer,required,out required)) throw new InvalidOperationException("Token privilege inspection failed.");
            uint count=unchecked((uint)Marshal.ReadInt32(buffer));
            if (count>(required-4)/12) throw new InvalidOperationException("Invalid token privilege count.");
            result.PrivilegePresent=false;
            for (uint index=0; index<count; index++) {
                IntPtr item=IntPtr.Add(buffer,checked(4+(int)index*12));
                if (unchecked((uint)Marshal.ReadInt32(item))==expected.Low && Marshal.ReadInt32(item,4)==expected.High) {
                    result.PrivilegePresent=true;
                    result.PrivilegeEnabledBefore=(Marshal.ReadInt32(item,8)&2)!=0;
                    break;
                }
            }
        } finally { Marshal.FreeHGlobal(buffer); }
    }
    public static Evidence Inspect() {
        RequirePlatform();
        var result=new Evidence {Reason="PrivilegeNotChecked"};
        IntPtr token;
        if (!OpenProcessToken(GetCurrentProcess(),8,out token)) { result.Error=ErrorCode(Marshal.GetLastWin32Error()); return result; }
        try {
            InspectToken(token,ShutdownPrivilege(),result);
            result.Reason=result.PrivilegePresent==true ? "StandbyPrivilegePresent" : "StandbyPrivilegeAbsent";
        } finally { result.TokenClosed=CloseHandle(token); }
        return result;
    }
    static bool SameObserver(Process observer, long startTicks, string path) {
        observer.Refresh();
        using (var caller=Process.GetCurrentProcess()) {
            return !observer.HasExited && observer.SessionId==caller.SessionId
                && observer.StartTime.ToUniversalTime().Ticks==startTicks
                && String.Equals(observer.MainModule.FileName,path,StringComparison.OrdinalIgnoreCase);
        }
    }
    // Caller must have fenced the exact own debug READY line and independently readable OS events.
    public static Evidence Request(int observerPid, long startTicks, string executable, Func<bool> finalPowerPreflight, Action<Evidence> recordIntent = null) {
        RequirePlatform();
        if (observerPid<=0 || !Path.IsPathRooted(executable) || finalPowerPreflight==null) throw new ArgumentException("Own observer identity required.");
        var result=new Evidence {Reason="StandbyNotInvoked"};
        IntPtr token=IntPtr.Zero, timer=IntPtr.Zero;
        Privileges previous=default(Privileges);
        bool adjustmentSucceeded=false;
        using (var observer=Process.GetProcessById(observerPid)) {
            try {
                if (!SameObserver(observer,startTicks,executable) || !finalPowerPreflight()) { result.Reason="FinalPreflightRefused"; return result; }
                if (recordIntent==null) { result.Reason="IntentRecorderRequired"; return result; }
                if (!OpenProcessToken(GetCurrentProcess(),8|32,out token)) { result.Error=ErrorCode(Marshal.GetLastWin32Error()); result.Reason="OwnTokenUnavailable"; return result; }
                Luid id=ShutdownPrivilege();
                InspectToken(token,id,result);
                if (result.PrivilegePresent!=true) { result.Reason="StandbyPrivilegeAbsent"; return result; }
                timer=CreateWaitableTimerW(IntPtr.Zero,false,null);
                if (timer==IntPtr.Zero) { result.Error=ErrorCode(Marshal.GetLastWin32Error()); result.Reason="WakeTimerCreationFailed"; return result; }
                result.WakeDueUtc=DateTime.UtcNow.AddSeconds(40);
                long due=result.WakeDueUtc.Value.ToFileTimeUtc();
                SetLastError(0);
                result.TimerArmed=SetWaitableTimer(timer,ref due,0,IntPtr.Zero,IntPtr.Zero,true);
                int timerError=Marshal.GetLastWin32Error();
                if (result.TimerArmed!=true || timerError!=0) { result.Error=ErrorCode(timerError); result.Reason="WakeTimerArmUnconfirmed"; return result; }
                var next=new Privileges {Count=1,Id=id,Attributes=2};
                uint required;
                SetLastError(0);
                adjustmentSucceeded=AdjustTokenPrivileges(token,false,ref next,16,out previous,out required);
                int privilegeError=Marshal.GetLastWin32Error();
                if (!adjustmentSucceeded || privilegeError!=0) { result.Error=ErrorCode(privilegeError); result.Reason="PrivilegeEnableUnconfirmed"; return result; }
                var enabled=new Evidence();
                InspectToken(token,id,enabled);
                result.PrivilegeEnabledForRequest=enabled.PrivilegePresent==true && enabled.PrivilegeEnabledBefore==true;
                if (result.PrivilegeEnabledForRequest!=true) { result.Reason="PrivilegeReadbackUnconfirmed"; return result; }
                if (!SameObserver(observer,startTicks,executable) || !finalPowerPreflight() || DateTime.UtcNow>result.WakeDueUtc.Value.AddSeconds(-30)) { result.Reason="FinalIdentityOrWakeDeadlineRefused"; return result; }
                result.RequestUtc=DateTime.UtcNow;
                result.TickBefore=GetTickCount64();
                recordIntent(result);
                if (DateTime.UtcNow>result.WakeDueUtc.Value.AddSeconds(-30)) { result.Reason="IntentWriteExceededWakeDeadline"; return result; }
                result.SleepInvoked=true;
                result.RequestSucceeded=SetSuspendState(false,false,false);
                if (result.RequestSucceeded!=true) result.Error=ErrorCode(Marshal.GetLastWin32Error());
                result.TickAfter=GetTickCount64();
                result.ReturnedUtc=DateTime.UtcNow;
                result.ObserverStillRunning=SameObserver(observer,startTicks,executable);
                result.Reason=result.RequestSucceeded==true ? "StandbyRequestReturnedNeedsOsEvidence" : "StandbyRequestFailed";
            } finally {
                if (adjustmentSucceeded) {
                    SetLastError(0);
                    result.PrivilegeRestored=RestoreTokenPrivileges(token,false,ref previous,0,IntPtr.Zero,IntPtr.Zero) && Marshal.GetLastWin32Error()==0;
                    // A readback failure must not prevent closing the timer and token.
                    try {
                        var restored=new Evidence();
                        InspectToken(token,ShutdownPrivilege(),restored);
                        result.PrivilegeRestored=result.PrivilegeRestored==true && restored.PrivilegePresent==true && restored.PrivilegeEnabledBefore==result.PrivilegeEnabledBefore;
                    } catch { result.PrivilegeRestored=false; }
                }
                if (timer!=IntPtr.Zero) { result.TimerCancelled=CancelWaitableTimer(timer); result.TimerClosed=CloseHandle(timer); }
                if (token!=IntPtr.Zero) result.TokenClosed=CloseHandle(token);
                if ((adjustmentSucceeded && result.PrivilegeRestored!=true) || (timer!=IntPtr.Zero && (result.TimerCancelled!=true || result.TimerClosed!=true)) || (token!=IntPtr.Zero && result.TokenClosed!=true))
                    result.Reason="NativeCleanupUnconfirmed";
            }
        }
        return result;
    }
}
