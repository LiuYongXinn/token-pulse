//! Explicit, bounded development-only native check. Normal tests never resize Explorer.
#[cfg(windows)]
#[tokio::main(flavor = "current_thread")]
async fn main() {
    use token_pulse_taskbar::{
        TaskbarView,
        windows::{control::NativeController, topology::inspect_primary_taskbar},
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let visual_check = arguments == ["--native-taskbar-visible-check"];
    if !visual_check && arguments != ["--native-taskbar-development-check"] {
        std::process::exit(2);
    }
    println!(
        "DEVELOPMENT ONLY: synthetic token/cost/quota fixture; bounded actual Explorer attachment"
    );
    let before = inspect_primary_taskbar().expect("supported taskbar baseline");
    let foreground = unsafe { GetForegroundWindow() };
    let controller = NativeController::start().expect("native controller");
    let fixture: TaskbarView =
        serde_json::from_str(include_str!("../../../fixtures/taskbar-display.json")).unwrap();
    let prepared = controller
        .replace(Some(fixture.clone()))
        .await
        .expect("native measured fixture");
    assert!(!prepared.embedded && !prepared.readout_visible);
    let shown = controller
        .enable_taskbar(true)
        .await
        .expect("native layout request");
    println!(
        "dpi={} embedded={} visible={} width={:?} density={:?} failure={:?}",
        before.dpi,
        shown.embedded,
        shown.readout_visible,
        shown.measured_width,
        shown.measured_density,
        shown.embedding_failure
    );
    // Scope exit also detaches on a failed check; explicit detachment verifies restoration.
    if visual_check {
        // No capture API is called during this observation: screen capture can
        // change Shell composition and hide the real presentation defect.
        for _ in 0..60 {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            let observed = controller.replace(Some(fixture.clone())).await.unwrap();
            assert!(observed.embedded && !observed.paint_failed);
            verify_own_input_surface();
        }
    } else {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
    if shown.embedded && !visual_check {
        capture_own_readout("visible");
    }
    let visible_check = controller
        .inspect()
        .await
        .expect("native attached inspection");
    let mut private = fixture;
    private.privacy = true;
    private.scope_label = None;
    private.costs.clear();
    private.quota = None;
    let updated = controller
        .replace(Some(private))
        .await
        .expect("native privacy update");
    if updated.embedded && !visual_check {
        capture_own_readout("privacy");
    }
    let focus_preserved = unsafe { GetForegroundWindow() } == foreground;
    let detached = controller
        .enable_taskbar(false)
        .await
        .expect("native detach request");
    let restored = inspect_primary_taskbar().expect("restored baseline");
    println!(
        "restoration={:?} geometry_restored={} foreground_preserved={} hidden={}",
        detached.last_restore,
        restored == before,
        focus_preserved,
        !detached.readout_visible
    );
    let reattached = controller
        .enable_taskbar(true)
        .await
        .expect("native reattach");
    let drop_was_embedded = reattached.embedded;
    drop(controller);
    let drop_restored = inspect_primary_taskbar().expect("Drop restored baseline") == before;
    println!(
        "reattached={} drop_restored={}",
        drop_was_embedded, drop_restored
    );
    assert_eq!(restored, before, "original taskbar geometry restored");
    assert!(!detached.embedded && !detached.readout_visible);
    assert!(
        drop_was_embedded && drop_restored,
        "ordinary owner Drop restores layout"
    );
    if !visual_check {
        assert!(focus_preserved, "attachment never activates its window");
    }
    assert!(
        shown.embedded
            && visible_check.embedded
            && updated.embedded
            && !updated.private_fields_present
            && !visible_check.paint_failed
            && !updated.paint_failed
            && shown.embedding_failure.is_none(),
        "actual guarded native embedding"
    );
}
#[cfg(windows)]
fn verify_own_input_surface() {
    use std::{mem, ptr};
    use windows_sys::Win32::{
        Foundation::{HWND, LPARAM, POINT, RECT},
        UI::{
            HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetThreadDpiAwarenessContext},
            WindowsAndMessaging::{
                EnumChildWindows, FindWindowW, GetClassNameW, GetWindowRect,
                GetWindowThreadProcessId, WindowFromPoint,
            },
        },
    };
    unsafe extern "system" fn find(window: HWND, data: LPARAM) -> i32 {
        let found = unsafe { &mut *(data as *mut Vec<HWND>) };
        let mut pid = 0;
        unsafe {
            GetWindowThreadProcessId(window, &mut pid);
        }
        let mut class = [0; 128];
        let length = unsafe { GetClassNameW(window, class.as_mut_ptr(), 128) };
        if pid == std::process::id()
            && length > 0
            && String::from_utf16_lossy(&class[..length as usize])
                .starts_with("TokenPulse.Taskbar.Readout.")
        {
            found.push(window);
        }
        1
    }
    let root_name: Vec<_> = "Shell_TrayWnd".encode_utf16().chain(Some(0)).collect();
    let root = unsafe { FindWindowW(root_name.as_ptr(), ptr::null()) };
    let mut found = Vec::<HWND>::new();
    unsafe {
        EnumChildWindows(root, Some(find), (&mut found as *mut Vec<HWND>) as LPARAM);
    }
    assert_eq!(found.len(), 1);
    let previous =
        unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    assert!(!previous.is_null());
    let mut rect: RECT = unsafe { mem::zeroed() };
    let valid = unsafe { GetWindowRect(found[0], &mut rect) } != 0;
    let points = [
        POINT {
            x: rect.left + 1,
            y: rect.top + 1,
        },
        POINT {
            x: (rect.left + rect.right) / 2,
            y: (rect.top + rect.bottom) / 2,
        },
        POINT {
            x: rect.right - 2,
            y: rect.bottom - 2,
        },
    ];
    let matches = valid
        && points.iter().all(|point| {
            let hit = unsafe { WindowFromPoint(*point) };
            if hit != found[0] {
                let mut pid = 0;
                unsafe {
                    GetWindowThreadProcessId(hit, &mut pid);
                }
                let mut class = [0; 128];
                let length = unsafe { GetClassNameW(hit, class.as_mut_ptr(), 128) };
                eprintln!(
                    "INPUT_SURFACE_MISMATCH point={},{} owner={} class={} owned={:?} hit={:?}",
                    point.x,
                    point.y,
                    pid,
                    String::from_utf16_lossy(&class[..length.max(0) as usize]),
                    found[0],
                    hit
                );
            }
            hit == found[0]
        });
    unsafe {
        SetThreadDpiAwarenessContext(previous);
    }
    assert!(
        matches,
        "transparent spacing must still target our owned readout"
    );
}
/// Capture only the visible readout window owned by this process. This helper never enters the
/// production host and never copies desktop, taskbar-wide or another application's pixels.
#[cfg(windows)]
fn capture_own_readout(label: &str) {
    use std::{mem, ptr};
    use windows_sys::Win32::UI::HiDpi::{
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetThreadDpiAwarenessContext,
    };
    struct RestoreDpi(windows_sys::Win32::Foundation::HANDLE);
    impl Drop for RestoreDpi {
        fn drop(&mut self) {
            unsafe {
                SetThreadDpiAwarenessContext(self.0);
            }
        }
    }
    let previous =
        unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    assert!(!previous.is_null());
    let _dpi = RestoreDpi(previous);
    use windows_sys::Win32::{
        Foundation::RECT,
        Graphics::Gdi::{
            BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CreateCompatibleDC, CreateDIBSection,
            DIB_RGB_COLORS, DeleteDC, DeleteObject, GdiFlush, GetDC, ReleaseDC, SRCCOPY,
            SelectObject,
        },
        UI::WindowsAndMessaging::{
            FindWindowW, GW_CHILD, GW_HWNDNEXT, GetClassNameW, GetClientRect, GetWindow,
            GetWindowThreadProcessId, IsWindowVisible,
        },
    };
    let root_name: Vec<_> = "Shell_TrayWnd".encode_utf16().chain(Some(0)).collect();
    let root = unsafe { FindWindowW(root_name.as_ptr(), ptr::null()) };
    assert!(!root.is_null());
    let mut window = unsafe { GetWindow(root, GW_CHILD) };
    let mut found: windows_sys::Win32::Foundation::HWND = ptr::null_mut();
    for _ in 0..256 {
        if window.is_null() {
            break;
        }
        let mut pid = 0;
        unsafe {
            GetWindowThreadProcessId(window, &mut pid);
        }
        if pid == std::process::id() && unsafe { IsWindowVisible(window) } != 0 {
            let mut text = [0; 128];
            let length = unsafe { GetClassNameW(window, text.as_mut_ptr(), 128) };
            if length > 0
                && String::from_utf16(&text[..length as usize])
                    .unwrap()
                    .starts_with("TokenPulse.Taskbar.Readout.")
            {
                assert!(found.is_null(), "one owned readout");
                found = window;
            }
        }
        window = unsafe { GetWindow(window, GW_HWNDNEXT) };
    }
    assert!(!found.is_null());
    let mut bounds: RECT = unsafe { mem::zeroed() };
    assert_ne!(unsafe { GetClientRect(found, &mut bounds) }, 0);
    let (width, height) = (bounds.right, bounds.bottom);
    assert!(
        (1..=4096).contains(&width)
            && (1..=2048).contains(&height)
            && i64::from(width) * i64::from(height) <= 1_048_576
    );
    let source = unsafe { GetDC(found) };
    assert!(!source.is_null());
    let dc = unsafe { CreateCompatibleDC(source) };
    assert!(!dc.is_null());
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..unsafe { mem::zeroed() }
        },
        ..unsafe { mem::zeroed() }
    };
    let mut pixels = ptr::null_mut();
    let bitmap =
        unsafe { CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut pixels, ptr::null_mut(), 0) };
    assert!(!bitmap.is_null() && !pixels.is_null());
    let previous = unsafe { SelectObject(dc, bitmap) };
    assert!(!previous.is_null() && previous as isize != -1);
    let copied = unsafe { BitBlt(dc, 0, 0, width, height, source, 0, 0, SRCCOPY) } != 0;
    let flushed = unsafe { GdiFlush() } != 0;
    let payload =
        unsafe { std::slice::from_raw_parts(pixels.cast::<u8>(), (width * height * 4) as usize) }
            .to_vec();
    unsafe {
        SelectObject(dc, previous);
        DeleteObject(bitmap);
        DeleteDC(dc);
        ReleaseDC(found, source);
    }
    assert!(copied && flushed);
    let mut output = vec![];
    output.extend_from_slice(b"BM");
    output.extend_from_slice(&((54 + payload.len()) as u32).to_le_bytes());
    output.extend_from_slice(&[0; 4]);
    output.extend_from_slice(&54u32.to_le_bytes());
    output.extend_from_slice(&40u32.to_le_bytes());
    output.extend_from_slice(&width.to_le_bytes());
    output.extend_from_slice(&(-height).to_le_bytes());
    output.extend_from_slice(&1u16.to_le_bytes());
    output.extend_from_slice(&32u16.to_le_bytes());
    output.extend_from_slice(&[0; 24]);
    output.extend_from_slice(&payload);
    let directory = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-results/taskbar-native-attachment");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join(format!("DEVELOPMENT-FIXTURE-{label}.bmp")),
        output,
    )
    .unwrap();
    std::fs::write(directory.join("README.txt"), "SYNTHETIC DEVELOPMENT ONLY. Actual pixels of this process's native readout only. No account read, no desktop/other-window capture.\n").unwrap();
}
#[cfg(not(windows))]
fn main() {
    std::process::exit(2);
}
