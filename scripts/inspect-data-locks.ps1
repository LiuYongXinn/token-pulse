param([string]$Database)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'project-env.ps1')
if (-not $Database) { $Database = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'com.tokenpulse.desktop\token-pulse.db' }
if (-not ('TokenPulseFileLocks' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class TokenPulseFileLocks {
    [StructLayout(LayoutKind.Sequential)] public struct ProcessIdentity { public int Id; public System.Runtime.InteropServices.ComTypes.FILETIME Start; }
    [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)] public struct ProcessInfo {
        public ProcessIdentity Process;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst=256)] public string Name;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst=64)] public string Service;
        public uint Type, Status, Session;
        [MarshalAs(UnmanagedType.Bool)] public bool Restartable;
    }
    [DllImport("rstrtmgr.dll", CharSet=CharSet.Unicode)] static extern int RmStartSession(out uint handle, int flags, string key);
    [DllImport("rstrtmgr.dll", CharSet=CharSet.Unicode)] static extern int RmRegisterResources(uint handle, uint count, string[] files, uint processes, IntPtr identities, uint services, IntPtr names);
    [DllImport("rstrtmgr.dll")] static extern int RmGetList(uint handle, out uint needed, ref uint count, [In,Out] ProcessInfo[] apps, ref uint reasons);
    [DllImport("rstrtmgr.dll")] static extern int RmEndSession(uint handle);
    public static ProcessInfo[] Find(string path) {
        uint handle;
        int result = RmStartSession(out handle, 0, Guid.NewGuid().ToString("N"));
        if(result != 0) throw new Exception("File-lock inspection failed: " + result);
        try {
            result = RmRegisterResources(handle, 1, new[]{path}, 0, IntPtr.Zero, 0, IntPtr.Zero);
            if(result != 0) throw new Exception("File registration failed: " + result);
            uint needed=0, count=0, reasons=0;
            result = RmGetList(handle, out needed, ref count, null, ref reasons);
            if(result == 0) return new ProcessInfo[0];
            if(result != 234) throw new Exception("File-lock listing failed: " + result);
            var apps = new ProcessInfo[needed]; count=needed;
            result = RmGetList(handle, out needed, ref count, apps, ref reasons);
            if(result != 0) throw new Exception("File-lock listing failed: " + result);
            return apps;
        } finally { RmEndSession(handle); }
    }
}
'@
}
[TokenPulseFileLocks]::Find($Database) | Select-Object Name, @{n='ProcessId';e={$_.Process.Id}}
