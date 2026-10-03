# Owned installed-application window checks shared by acceptance and bounded recovery.
Add-Type -AssemblyName UIAutomationClient
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class InstallerWindowProbe {
    public static bool NsIsBinaryMatches(string releaseFile, string installedFile) {
        var expected = System.IO.File.ReadAllBytes(releaseFile);
        var actual = System.IO.File.ReadAllBytes(installedFile);
        if (expected.Length != actual.Length) return false;
        // tauri-bundler restores the unpatched build output after packaging. The installed
        // NSIS binary differs only at this documented, fixed-width bundle-kind marker.
        var token = Encoding.ASCII.GetBytes("__TAURI_BUNDLE_TYPE_VAR_UNK");
        var replacement = Encoding.ASCII.GetBytes("__TAURI_BUNDLE_TYPE_VAR_NSS");
        int marker = -1;
        for (int i = 0; i <= expected.Length - token.Length; i++) {
            if (expected[i] != token[0]) continue;
            bool match = true;
            for (int j = 1; j < token.Length; j++) if (expected[i+j] != token[j]) { match = false; break; }
            if (match) { if (marker != -1) return false; marker = i; }
        }
        if (marker < 0) return false;
        Array.Copy(replacement, 0, expected, marker, replacement.Length);
        for (int i = 0; i < expected.Length; i++) if (expected[i] != actual[i]) return false;
        return true;
    }
    public delegate bool EnumProc(IntPtr window, IntPtr parameter);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc callback, IntPtr parameter);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr window, StringBuilder name, int capacity);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetWindowText(IntPtr window, StringBuilder name, int capacity);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] static extern bool PostMessage(IntPtr window, uint message, UIntPtr wparam, IntPtr lparam);
    [DllImport("user32.dll")] static extern IntPtr SendMessageTimeout(IntPtr window, uint message, UIntPtr wparam, IntPtr lparam, uint flags, uint timeout, out UIntPtr result);
    [DllImport("user32.dll")] static extern int GetMenuItemCount(IntPtr menu);
    [DllImport("user32.dll")] static extern uint GetMenuItemID(IntPtr menu, int position);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetMenuString(IntPtr menu, uint item, StringBuilder text, int capacity, uint flags);
    public static IntPtr Find(uint pid, string kind) {
        var found = new List<IntPtr>();
        EnumWindows((window, unused) => {
            uint owner; GetWindowThreadProcessId(window, out owner);
            if (owner != pid) return true;
            var cls = new StringBuilder(128); GetClassName(window, cls, cls.Capacity);
            var title = new StringBuilder(128); GetWindowText(window, title, title.Capacity);
            if ((kind == "tray" && cls.ToString() == "tray_icon_app") ||
                (kind == "menu" && cls.ToString() == "#32768" && IsWindowVisible(window)) ||
                (kind == "main" && title.ToString() == "TokenPulse") ||
                (kind == "mini" && title.ToString() == "TokenPulse · 悬浮窗")) found.Add(window);
            return true;
        }, IntPtr.Zero);
        return found.Count == 1 ? found[0] : IntPtr.Zero;
    }
    public static void Post(uint pid, IntPtr window, uint message, uint wparam, long lparam) {
        uint owner; GetWindowThreadProcessId(window, out owner);
        if (window == IntPtr.Zero || owner != pid) throw new Exception("Refuse message outside owned application window.");
        if (!PostMessage(window, message, new UIntPtr(wparam), new IntPtr(lparam))) throw new Exception("Owned window message failed.");
    }
    public static uint[] MenuIds(uint pid, IntPtr window) {
        uint owner; GetWindowThreadProcessId(window, out owner);
        if (window == IntPtr.Zero || owner != pid) throw new Exception("Refuse menu outside owned application.");
        UIntPtr raw;
        if (SendMessageTimeout(window, 0x01e1, UIntPtr.Zero, IntPtr.Zero, 2, 1000, out raw) == IntPtr.Zero) throw new Exception("Owned menu unavailable.");
        var menu = new IntPtr(unchecked((long)raw.ToUInt64()));
        if (GetMenuItemCount(menu) != 3) throw new Exception("Unexpected production tray menu.");
        var expected = new [] { "打开统计", "显示悬浮窗 / 恢复交互", "退出 TokenPulse" };
        var ids = new uint[3];
        for (int i = 0; i < 3; i++) {
            var text = new StringBuilder(128); GetMenuString(menu, (uint)i, text, text.Capacity, 0x400);
            if (text.ToString() != expected[i]) throw new Exception("Unexpected tray action.");
            ids[i] = GetMenuItemID(menu, i);
            if (ids[i] == 0xffffffff) throw new Exception("Invalid menu action.");
        }
        return ids;
    }
}
'@
function Wait-InstallerCondition([scriptblock]$Condition, [string]$Failure, [int]$Seconds = 20) {
    $deadline = [DateTime]::UtcNow.AddSeconds($Seconds)
    while ([DateTime]::UtcNow -lt $deadline) {
        if (& $Condition) { return }
        Start-Sleep -Milliseconds 150
    }
    throw $Failure
}
function Read-ProductionMenu([System.Diagnostics.Process]$Application) {
    Wait-InstallerCondition { [InstallerWindowProbe]::Find($Application.Id, 'tray') -ne [IntPtr]::Zero } 'Production tray did not initialize.'
    if ($OwnTrayCommands) {
        # Explicit reduced acceptance when the interactive desktop cannot display a menu.
        # Pinned muda 0.20.0 starts IDs at 1000; these three items are created before Menu.
        # Require the reviewed source ordering and lockfile; never infer arbitrary IDs.
        $menuSource = Get-Content -LiteralPath (Join-Path $installerRoot 'src-tauri\src\lib.rs') -Raw
        $menuOrder = [regex]::Matches($menuSource, 'MenuItem::with_id\(app, "([^"]+)"') | ForEach-Object { $_.Groups[1].Value }
        $lockSource = Get-Content -LiteralPath (Join-Path $installerRoot 'Cargo.lock') -Raw
        if (($menuOrder -join ',') -ne 'open,mini,quit' -or [regex]::Matches($menuSource, 'Menu::').Count -ne 1 -or $lockSource -notmatch 'name = "muda"\s+version = "0\.20\.0"') { throw 'Production tray command mapping changed; reduced acceptance refused.' }
        return @(1000, 1001, 1002)
    }
    $tray = [InstallerWindowProbe]::Find($Application.Id, 'tray')
    # Version-pinned tray-icon 0.25.1 notification callback; authored own-window messages,
    # not physical mouse evidence. Menu contents are independently checked before activation.
    try {
        [InstallerWindowProbe]::Post($Application.Id, $tray, 6002, 0, 0x0205)
        Wait-InstallerCondition { [InstallerWindowProbe]::Find($Application.Id, 'menu') -ne [IntPtr]::Zero } 'Production tray menu did not open.'
        $menu = [InstallerWindowProbe]::Find($Application.Id, 'menu')
        return [InstallerWindowProbe]::MenuIds($Application.Id, $menu)
    } finally {
        # Cancel only this application's menu even after a failed observation.
        [InstallerWindowProbe]::Post($Application.Id, $tray, 0x001f, 0, 0)
        Wait-InstallerCondition { [InstallerWindowProbe]::Find($Application.Id, 'menu') -eq [IntPtr]::Zero } 'Production tray menu did not close.'
    }
}
function Exit-ProductionApplication([System.Diagnostics.Process]$Application) {
    # Hold the native process handle before exit so an externally observed process also
    # retains its actual exit status. Missing status must never be accepted as zero.
    $null = $Application.Handle
    $ids = Read-ProductionMenu $Application
    [InstallerWindowProbe]::Post($Application.Id, [InstallerWindowProbe]::Find($Application.Id, 'tray'), 0x0111, $ids[2], 0)
    if (-not $Application.WaitForExit(20000)) { throw 'Production application did not complete normal tray exit.' }
    $Application.Refresh()
    if ($Application.ExitCode -ne 0) { throw 'Production application exit failed.' }
}
