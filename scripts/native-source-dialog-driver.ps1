param([Parameter(Mandatory)][uint32]$ApplicationId, [Parameter(Mandatory)][ValidateSet('cancel','select')][string]$Action, [string]$Folder, [ValidateSet('source','account_executable','account_home')][string]$Kind = 'source')
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class SourceDialogProbe {
    public delegate bool EnumProc(IntPtr window, IntPtr parameter);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc callback, IntPtr parameter);
    [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr root, EnumProc callback, IntPtr parameter);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr window, StringBuilder value, int capacity);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetWindowText(IntPtr window, StringBuilder value, int capacity);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] static extern bool PostMessage(IntPtr window, uint message, UIntPtr wp, IntPtr lp);
    [DllImport("user32.dll")] static extern int GetDlgCtrlID(IntPtr window);
    [DllImport("user32.dll", EntryPoint="SendMessageTimeoutW")] static extern IntPtr Send(IntPtr window, uint message, UIntPtr wp, IntPtr lp, uint flags, uint timeout, out UIntPtr result);
    [DllImport("user32.dll", EntryPoint="SendMessageTimeoutW", CharSet=CharSet.Unicode)] static extern IntPtr SendText(IntPtr window, uint message, UIntPtr wp, string text, uint flags, uint timeout, out UIntPtr result);
    [DllImport("user32.dll", EntryPoint="SendMessageTimeoutW", CharSet=CharSet.Unicode)] static extern IntPtr SendBuffer(IntPtr window, uint message, UIntPtr wp, StringBuilder text, uint flags, uint timeout, out UIntPtr result);
    static IntPtr Control(IntPtr root, uint expectedPid, string wantedClass, int id) {
        var matches = new List<IntPtr>();
        EnumChildWindows(root, (window, parameter) => {
            uint pid; GetWindowThreadProcessId(window, out pid);
            var value = new StringBuilder(128); GetClassName(window, value, value.Capacity);
            if (pid == expectedPid && IsWindowVisible(window) && value.ToString() == wantedClass && (id < 0 || GetDlgCtrlID(window) == id)) matches.Add(window);
            return true;
        }, IntPtr.Zero);
        if (matches.Count > 1) throw new Exception("DIALOG_AMBIGUOUS_CONTROL");
        return matches.Count == 1 ? matches[0] : IntPtr.Zero;
    }
    public static void Choose(uint expectedPid, IntPtr root, string action, string folder, int fieldId) {
        var button = Control(root, expectedPid, "Button", action == "cancel" ? 2 : 1);
        if (button == IntPtr.Zero) throw new Exception("DIALOG_ACTION_MISSING");
        UIntPtr result;
        if (action == "select") {
            var field = Control(root, expectedPid, "Edit", fieldId);
            if (field == IntPtr.Zero) {
                var combo = Control(root, expectedPid, "ComboBox", fieldId);
                if (combo == IntPtr.Zero) combo = Control(root, expectedPid, "ComboBoxEx32", fieldId);
                if (combo != IntPtr.Zero) field = Control(combo, expectedPid, "Edit", -1);
            }
            if (field == IntPtr.Zero) {
                EnumChildWindows(root, (window, parameter) => {
                    uint pid; GetWindowThreadProcessId(window, out pid);
                    var name = new StringBuilder(128); GetClassName(window, name, name.Capacity);
                    if(pid == expectedPid && IsWindowVisible(window) && (name.ToString() == "Edit" || name.ToString() == "ComboBox" || name.ToString() == "ComboBoxEx32")) Console.Error.WriteLine("DIALOG_FIELD_" + name.ToString().ToUpperInvariant() + "_" + GetDlgCtrlID(window));
                    return true;
                }, IntPtr.Zero);
                throw new Exception("DIALOG_PATH_FIELD_MISSING");
            }
            if (SendText(field, 0x000C, UIntPtr.Zero, folder, 2, 3000, out result) == IntPtr.Zero) throw new Exception("DIALOG_PATH_SET_FAILED");
            // GetWindowText cannot read another process's Edit value. WM_GETTEXT is a
            // system-marshalled message and uses a bounded buffer / deadline here.
            var actual = new StringBuilder(32768);
            if (SendBuffer(field, 0x000D, (UIntPtr)(uint)actual.Capacity, actual, 2, 3000, out result) == IntPtr.Zero) throw new Exception("DIALOG_PATH_VALUE_MISSING");
            if (actual.ToString() != folder) throw new Exception("DIALOG_PATH_VALUE_MISSING");
        }
        if (Send(button, 0x00F5, UIntPtr.Zero, IntPtr.Zero, 2, 3000, out result) == IntPtr.Zero) throw new Exception("DIALOG_ACTION_FAILED");
    }
    public static IntPtr Find(uint expectedPid, string title) {
        var matches = new List<IntPtr>();
        EnumWindows((window, parameter) => {
            uint pid; GetWindowThreadProcessId(window, out pid);
            if (pid != expectedPid || !IsWindowVisible(window)) return true;
            var value = new StringBuilder(256);
            GetClassName(window, value, value.Capacity);
            if (value.ToString() != "#32770") return true;
            GetWindowText(window, value, value.Capacity);
            if (value.ToString() == title) matches.Add(window);
            return true;
        }, IntPtr.Zero);
        if (matches.Count > 1) throw new Exception("DIALOG_AMBIGUOUS");
        return matches.Count == 1 ? matches[0] : IntPtr.Zero;
    }
    public static void CancelOwned(uint expectedPid, IntPtr window) {
        uint pid; GetWindowThreadProcessId(window, out pid);
        if (pid == expectedPid && window != IntPtr.Zero) PostMessage(window, 0x0010, UIntPtr.Zero, IntPtr.Zero);
    }
}
'@
$dialogTitle = switch ($Kind) {
    'source' { '选择 Codex Home（包含 sessions 的目录）' }
    'account_executable' { '选择 Codex 原生 codex.exe' }
    'account_home' { '选择此账户服务的 Codex Home' }
}
$dialogWindow = [IntPtr]::Zero
try {
    if ($Action -eq 'select') {
        $resolvedFolder = (Resolve-Path -LiteralPath $Folder).Path
        $fixtureHome = if ($Kind -eq 'account_executable') { [IO.Path]::GetDirectoryName($resolvedFolder) } else { $resolvedFolder }
        $expectedHomeName = if ($Kind -eq 'source') { 'synthetic-dialog-home' } else { 'synthetic-account-dialog-home' }
        if ([IO.Path]::GetFileName($fixtureHome) -ne $expectedHomeName -or -not ([IO.Path]::GetFileName([IO.Path]::GetDirectoryName($fixtureHome))).StartsWith('native-probe-')) { throw 'DIALOG_FIXTURE_PATH_REFUSED' }
        if ($Kind -eq 'account_executable' -and [IO.Path]::GetFileName($resolvedFolder) -ne 'synthetic-codex.exe') { throw 'DIALOG_FIXTURE_PATH_REFUSED' }
    }
    $dialogDeadline = [DateTime]::UtcNow.AddSeconds(20)
    while ($dialogWindow -eq [IntPtr]::Zero -and [DateTime]::UtcNow -lt $dialogDeadline) {
        $dialogWindow = [SourceDialogProbe]::Find($ApplicationId, $dialogTitle)
        if ($dialogWindow -eq [IntPtr]::Zero) { Start-Sleep -Milliseconds 100 }
    }
    if ($dialogWindow -eq [IntPtr]::Zero) { throw 'DIALOG_NOT_FOUND' }
    $pathFieldId = if ($Kind -eq 'account_executable') { 1148 } else { 1152 }
    [SourceDialogProbe]::Choose($ApplicationId, $dialogWindow, $Action, $resolvedFolder, $pathFieldId)
    $closeDeadline = [DateTime]::UtcNow.AddSeconds(8)
    while ([SourceDialogProbe]::Find($ApplicationId, $dialogTitle) -ne [IntPtr]::Zero -and [DateTime]::UtcNow -lt $closeDeadline) { Start-Sleep -Milliseconds 100 }
    if ([SourceDialogProbe]::Find($ApplicationId, $dialogTitle) -ne [IntPtr]::Zero) { throw 'DIALOG_NOT_CLOSED' }
    Write-Output 'NATIVE_SOURCE_DIALOG_DRIVER_OK'
    exit 0
} catch {
    [SourceDialogProbe]::CancelOwned($ApplicationId, $dialogWindow)
    $failureCode = 'DIALOG_API_FAILED'
    $cause = $_.Exception
    for ($level = 0; $level -lt 4 -and $null -ne $cause; $level++) {
        if ($cause.Message -match '^DIALOG_[A-Z_]+$') { $failureCode = $cause.Message; break }
        $cause = $cause.InnerException
    }
    Write-Error "NATIVE_SOURCE_DIALOG_DRIVER_FAILED: $failureCode" -ErrorAction Continue
    exit 1
}
