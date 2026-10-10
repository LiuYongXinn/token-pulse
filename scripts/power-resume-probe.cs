// Windows capability inspection only. No sleep, hibernation, restart or policy mutation API.
using System;
using System.Diagnostics;
using System.Runtime.InteropServices;

[assembly: DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
public static class TokenPulsePowerInspection {
    [StructLayout(LayoutKind.Sequential)]
    struct PowerStatus {
        public byte AcLine, BatteryFlag, BatteryPercent, SystemStatus;
        public uint BatteryLifeSeconds, BatteryFullLifeSeconds;
    }
    [DllImport("kernel32.dll", SetLastError=true)]
    static extern bool GetSystemPowerStatus(out PowerStatus status);
    [DllImport("powrprof.dll", SetLastError=true)]
    [return: MarshalAs(UnmanagedType.I1)]
    static extern bool GetPwrCapabilities(IntPtr capabilities);
    [DllImport("powrprof.dll")]
    [return: MarshalAs(UnmanagedType.I1)]
    static extern bool IsPwrSuspendAllowed();
    [DllImport("powrprof.dll")]
    static extern uint PowerGetActiveScheme(IntPtr key, out IntPtr scheme);
    [DllImport("powrprof.dll")]
    static extern uint PowerReadACValueIndex(IntPtr key, ref Guid scheme, ref Guid subgroup, ref Guid setting, out uint value);
    [DllImport("powrprof.dll")]
    static extern uint PowerReadDCValueIndex(IntPtr key, ref Guid scheme, ref Guid subgroup, ref Guid setting, out uint value);
    [DllImport("kernel32.dll")]
    static extern IntPtr LocalFree(IntPtr memory);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    static extern IntPtr CreateWaitableTimerW(IntPtr attributes, bool manualReset, string name);
    [DllImport("kernel32.dll", SetLastError=true)]
    static extern bool SetWaitableTimer(IntPtr timer, ref long dueTime, int period, IntPtr routine, IntPtr argument, bool resume);
    [DllImport("kernel32.dll", SetLastError=true)]
    static extern bool CancelWaitableTimer(IntPtr timer);
    [DllImport("kernel32.dll", SetLastError=true)]
    static extern bool CloseHandle(IntPtr handle);
    [DllImport("kernel32.dll")]
    static extern void SetLastError(uint error);

    public sealed class Evidence {
        public int Schema=1, Build=Environment.OSVersion.Version.Build, SessionId;
        public bool? StandbyS3, SystemS4Supported, HibernationFilePresent;
        public bool StandbyAllowed, CapabilitiesRead;
        public int? CapabilitiesError, PowerStatusError;
        public byte? AcLineStatus;
        public Guid? ActiveScheme;
        public uint? SchemeError, WakePolicyIndex, WakePolicyError;
        public bool WakeTimerChecked;
        public bool? TimerCreated, TimerArmed, TimerWakeSupported, TimerCancelled, TimerClosed;
        public int? TimerError;
        public bool? Eligible;
        public string Reason;
        public bool SleepInvoked=false, HibernateInvoked=false, ComputerRestart=false, PolicyChanged=false;
    }
    static int? LastError() {
        int error=Marshal.GetLastWin32Error();
        return error==0 ? (int?)null : error;
    }
    public static Evidence Inspect(bool checkWakeTimer) {
        if (!RuntimeInformation.IsOSPlatform(OSPlatform.Windows) || IntPtr.Size!=8)
            throw new InvalidOperationException("A 64-bit Windows process is required.");
        var result=new Evidence {SessionId=Process.GetCurrentProcess().SessionId, WakeTimerChecked=checkWakeTimer};
        // Native BOOLEAN fields begin with three button/lid flags then S1/S2/S3/S4/S5.
        // A zeroed oversized allocation contains the entire Win10 SYSTEM_POWER_CAPABILITIES;
        // only documented byte fields 5, 6 and 8 are projected after successful native read.
        IntPtr capabilities=Marshal.AllocHGlobal(256);
        try {
            Marshal.Copy(new byte[256],0,capabilities,256);
            result.CapabilitiesRead=GetPwrCapabilities(capabilities);
            if (result.CapabilitiesRead) {
                result.StandbyS3=Marshal.ReadByte(capabilities,5)!=0;
                result.SystemS4Supported=Marshal.ReadByte(capabilities,6)!=0;
                result.HibernationFilePresent=Marshal.ReadByte(capabilities,8)!=0;
            } else result.CapabilitiesError=LastError();
        } finally { Marshal.FreeHGlobal(capabilities); }
        result.StandbyAllowed=IsPwrSuspendAllowed();
        PowerStatus status;
        if (GetSystemPowerStatus(out status)) result.AcLineStatus=status.AcLine<=1 ? (byte?)status.AcLine : null;
        else result.PowerStatusError=LastError();
        IntPtr active;
        uint schemeStatus=PowerGetActiveScheme(IntPtr.Zero,out active);
        try {
            if (schemeStatus==0 && active!=IntPtr.Zero) {
                Guid scheme=Marshal.PtrToStructure<Guid>(active);
                result.ActiveScheme=scheme;
                Guid subgroup=new Guid("238c9fa8-0aad-41ed-83f4-97be242c8f20");
                Guid setting=new Guid("bd3b718a-0680-4d9d-8ab2-e1d2b4ac806d");
                uint value, policyStatus;
                if (result.AcLineStatus==1) policyStatus=PowerReadACValueIndex(IntPtr.Zero,ref scheme,ref subgroup,ref setting,out value);
                else if (result.AcLineStatus==0) policyStatus=PowerReadDCValueIndex(IntPtr.Zero,ref scheme,ref subgroup,ref setting,out value);
                else { result.Reason="PowerSourceUnknown"; return result; }
                if (policyStatus==0) result.WakePolicyIndex=value;
                else result.WakePolicyError=policyStatus;
            } else if (schemeStatus!=0) result.SchemeError=schemeStatus;
        } finally { if (active!=IntPtr.Zero) LocalFree(active); }
        if (result.StandbyS3==false || !result.StandbyAllowed) { result.Eligible=false; result.Reason="StandbyUnavailable"; return result; }
        if (result.AcLineStatus.HasValue && result.AcLineStatus!=1) { result.Eligible=false; result.Reason="AcPowerRequired"; return result; }
        if (result.WakePolicyIndex.HasValue && result.WakePolicyIndex!=1) { result.Eligible=false; result.Reason="WakeTimersNotFullyEnabled"; return result; }
        if (result.StandbyS3!=true || result.AcLineStatus!=1 || result.WakePolicyIndex!=1) { result.Reason="PowerInspectionIncomplete"; return result; }
        if (!checkWakeTimer) { result.Reason="WakeTimerNotChecked"; return result; }
        IntPtr timer=CreateWaitableTimerW(IntPtr.Zero,false,null);
        result.TimerCreated=timer!=IntPtr.Zero;
        if (timer==IntPtr.Zero) { result.TimerError=LastError(); result.Eligible=false; result.Reason="WakeTimerCreationFailed"; return result; }
        try {
            // Windows 8+ excludes sleep time from relative timers. Use an absolute UTC FILETIME.
            // This preflight immediately cancels the private unnamed noninheritable timer.
            long due=DateTime.UtcNow.AddSeconds(40).ToFileTimeUtc();
            SetLastError(0);
            bool armed=SetWaitableTimer(timer,ref due,0,IntPtr.Zero,IntPtr.Zero,true);
            int error=Marshal.GetLastWin32Error();
            result.TimerArmed=armed;
            if (error!=0) result.TimerError=error;
            if (armed) result.TimerWakeSupported=error!=50;
            result.Eligible=armed && result.TimerWakeSupported==true;
            result.Reason=result.Eligible==true ? "WakePreflightPassed" : "WakeTimerUnavailable";
        } finally {
            result.TimerCancelled=CancelWaitableTimer(timer);
            result.TimerClosed=CloseHandle(timer);
            if (result.TimerClosed!=true || (result.TimerArmed==true && result.TimerCancelled!=true)) {
                result.Eligible=false;
                result.Reason="WakeTimerCleanupUnconfirmed";
            }
        }
        return result;
    }
}
