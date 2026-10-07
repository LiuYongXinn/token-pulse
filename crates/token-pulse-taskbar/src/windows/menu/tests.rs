use super::*;
use std::ptr;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetMenuItemCount, GetMenuItemInfoW, GetMenuState, GetMenuStringW, MF_BYCOMMAND, MF_BYPOSITION,
    MIIM_STRING,
};

#[test]
fn native_menu_retains_labels_accessibility_checked_privacy_and_action_whitelist() {
    for privacy in [false, true] {
        let menu = create(privacy, AppTheme::Light, 96).unwrap();
        assert_eq!(unsafe { GetMenuItemCount(menu.handle) }, 6);
        for (index, (id, title, _, key)) in ITEMS.iter().enumerate() {
            if *id == 0 {
                continue;
            }
            let mut text = [0; 128];
            let n = unsafe {
                GetMenuStringW(
                    menu.handle,
                    index as u32,
                    text.as_mut_ptr(),
                    128,
                    MF_BYPOSITION,
                )
            };
            assert_eq!(String::from_utf16_lossy(&text[..n as usize]), *title);
            let mut info = MENUITEMINFOW {
                cbSize: mem::size_of::<MENUITEMINFOW>() as u32,
                fMask: MIIM_DATA | MIIM_FTYPE | MIIM_STRING,
                dwTypeData: text.as_mut_ptr(),
                cch: 128,
                ..unsafe { mem::zeroed() }
            };
            assert_ne!(
                unsafe { GetMenuItemInfoW(menu.handle, index as u32, 1, &mut info) },
                0
            );
            if menu.style.is_some() {
                assert_ne!(info.fType & MFT_OWNERDRAW, 0);
                assert_eq!(
                    info.dwItemData,
                    &menu.entries[index] as *const Entry as usize
                );
                let msaa = &menu.entries[index].msaa;
                assert_eq!(msaa.dwMSAASignature, MSAA_MENU_SIG as u32);
                assert_eq!(msaa.cchWText as usize, title.encode_utf16().count());
                assert_eq!(
                    msaa.pszWText,
                    menu.entries[index].title.as_ptr() as *mut u16
                );
            }
            assert_eq!(mnemonic(*key as u16), Some(index));
            assert_eq!(mnemonic(key.to_ascii_lowercase() as u16), Some(index));
        }
        assert_eq!(
            unsafe { GetMenuState(menu.handle, PRIVACY, MF_BYCOMMAND) } & MF_CHECKED != 0,
            privacy
        );
        assert_eq!(
            action(PRIVACY, privacy),
            Some(HostAction::SetPrivacy { enabled: !privacy })
        );
    }
    assert_eq!(action(FLOAT, false), Some(HostAction::OpenFloat {}));
    assert_eq!(action(STATS, false), Some(HostAction::OpenStats {}));
    assert_eq!(
        action(SETTINGS, false),
        Some(HostAction::OpenTaskbarSettings {})
    );
    assert_eq!(action(DISABLE, false), Some(HostAction::DisableTaskbar {}));
    assert!(action(0, false).is_none() && action(u32::MAX, false).is_none());
    assert_eq!(mnemonic(0), None);
    assert_eq!(mnemonic('x' as u16), None);
}

fn bitmap(style: &Style, selected: Option<usize>, privacy: bool) -> Vec<u8> {
    let height = (0..ITEMS.len()).map(|i| style.height(i)).sum();
    style
        .font
        .bitmap_with(style.width, height, |dc| {
            let mut top = 0;
            for index in 0..ITEMS.len() {
                let bottom = top + style.height(index);
                unsafe {
                    style.draw(
                        dc,
                        RECT {
                            left: 0,
                            top,
                            right: style.width,
                            bottom,
                        },
                        index,
                        selected == Some(index),
                        privacy,
                    )
                }?;
                top = bottom;
            }
            Ok(())
        })
        .unwrap()
}

#[test]
fn theme_dpi_font_fit_hover_and_privacy_render_with_real_gdi() {
    for dpi in [96, 120, 144, 192] {
        for theme in [AppTheme::Light, AppTheme::Dark] {
            let style = Style::new(theme, dpi).unwrap();
            for (id, _, label, _) in ITEMS {
                let label_end = style.px(44) + style.font.width(label).unwrap();
                let key_start = style.width - style.px(16) - style.key_width;
                let trailing_start = key_start
                    - if id == PRIVACY {
                        style.px(8) + style.status_width
                    } else {
                        0
                    };
                assert!(label_end + style.px(12) <= trailing_start);
                assert!(style.row_height >= style.font.height() + style.px(18));
            }
            let normal = bitmap(&style, None, false);
            let hover = bitmap(&style, Some(2), false);
            let private = bitmap(&style, Some(4), true);
            assert_ne!(normal, hover);
            assert_ne!(hover, private);
            let background = style.colors.background;
            assert_eq!(
                &normal[54..57],
                &[
                    (background >> 16) as u8,
                    (background >> 8) as u8,
                    background as u8
                ]
            );
            // Optional review artifacts come from the production renderer, not a mockup.
            if dpi == 144 {
                if let Ok(directory) = std::env::var("TOKENPULSE_MENU_PREVIEW_DIR") {
                    std::fs::create_dir_all(&directory).unwrap();
                    let name = if theme == AppTheme::Light {
                        "light"
                    } else {
                        "dark"
                    };
                    std::fs::write(
                        std::path::Path::new(&directory).join(format!("menu-{name}.bmp")),
                        &hover,
                    )
                    .unwrap();
                    std::fs::write(
                        std::path::Path::new(&directory).join(format!("menu-{name}-privacy.bmp")),
                        &private,
                    )
                    .unwrap();
                }
            }
        }
    }
}

#[test]
fn native_modal_menu_measures_draws_executes_mnemonics_and_cancels() {
    use windows_sys::Win32::{
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, EndMenu, KillTimer, PostMessageW,
            RegisterClassExW, SetTimer, UnregisterClassW, WM_CHAR, WM_TIMER, WNDCLASSEXW, WS_POPUP,
        },
    };
    thread_local! { static COMMAND: Cell<u16> = const { Cell::new(0) }; static DRAWN: Cell<usize> = const { Cell::new(0) }; }
    unsafe extern "system" fn procedure(
        window: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if let Some(result) = handle_message(window, message, wparam, lparam) {
            if message == WM_DRAWITEM {
                DRAWN.with(|n| n.set(n.get() + 1));
            }
            return result;
        }
        if message == WM_TIMER {
            unsafe {
                KillTimer(window, 1);
            }
            let key = COMMAND.with(Cell::get);
            inspect_popup();
            if key == 27 {
                unsafe {
                    PostMessageW(
                        window,
                        windows_sys::Win32::UI::WindowsAndMessaging::WM_KEYDOWN,
                        windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE as usize,
                        1,
                    );
                }
            } else if key == 0 {
                unsafe {
                    EndMenu();
                }
            } else {
                unsafe {
                    PostMessageW(window, WM_CHAR, key as usize, 1);
                }
            }
            return 0;
        }
        unsafe { DefWindowProcW(window, message, wparam, lparam) }
    }
    fn inspect_popup() {
        use windows::Win32::{
            Foundation::HWND as ComWindow,
            System::Com::{
                CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
                CoUninitialize,
            },
            UI::Accessibility::{
                CUIAutomation8, IUIAutomation, TreeScope_Children, UIA_MenuItemControlTypeId,
            },
        };
        use windows_sys::Win32::{
            System::Threading::GetCurrentThreadId,
            UI::WindowsAndMessaging::{
                EnumThreadWindows, GetClassNameW, GetWindowRect, PRF_CLIENT, PRF_NONCLIENT,
                SendMessageW, WM_PRINT,
            },
        };
        unsafe extern "system" fn find(window: HWND, param: LPARAM) -> i32 {
            let mut class = [0u16; 64];
            let n = unsafe { GetClassNameW(window, class.as_mut_ptr(), class.len() as i32) };
            if n > 0 && String::from_utf16_lossy(&class[..n as usize]) == "#32768" {
                unsafe {
                    *(param as *mut HWND) = window;
                }
                return 0;
            }
            1
        }
        let mut popup: HWND = ptr::null_mut();
        unsafe {
            EnumThreadWindows(
                GetCurrentThreadId(),
                Some(find),
                &mut popup as *mut HWND as LPARAM,
            );
        }
        assert!(
            !popup.is_null(),
            "native popup belongs to this test UI thread"
        );
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap();
            let automation: IUIAutomation =
                CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER).unwrap();
            let root = automation.ElementFromHandle(ComWindow(popup)).unwrap();
            let children = root
                .FindAll(
                    TreeScope_Children,
                    &automation.CreateTrueCondition().unwrap(),
                )
                .unwrap();
            let mut names = vec![];
            for index in 0..children.Length().unwrap() {
                let item = children.GetElement(index).unwrap();
                if item.CurrentControlType().unwrap() == UIA_MenuItemControlTypeId {
                    names.push(item.CurrentName().unwrap().to_string());
                    assert!(item.CurrentIsEnabled().unwrap().as_bool());
                }
            }
            assert_eq!(
                names,
                ITEMS
                    .iter()
                    .filter(|(id, _, _, _)| *id != 0)
                    .map(|(_, title, _, _)| title.replace('&', ""))
                    .collect::<Vec<_>>()
            );
            drop(children);
            drop(root);
            drop(automation);
            CoUninitialize();
        }
        if let Ok(directory) = std::env::var("TOKENPULSE_MENU_PREVIEW_DIR") {
            let menu = ACTIVE.with(|active| Rc::clone(&active.borrow().as_ref().unwrap().1));
            let style = menu.style.as_ref().unwrap();
            let mut rect: RECT = unsafe { mem::zeroed() };
            assert_ne!(unsafe { GetWindowRect(popup, &mut rect) }, 0);
            let bytes = style
                .font
                .bitmap_with(rect.right - rect.left, rect.bottom - rect.top, |dc| {
                    unsafe {
                        SendMessageW(
                            popup,
                            WM_PRINT,
                            dc as usize,
                            (PRF_CLIENT | PRF_NONCLIENT) as isize,
                        );
                    }
                    Ok(())
                })
                .unwrap();
            std::fs::write(
                std::path::Path::new(&directory).join("menu-native-dark.bmp"),
                bytes,
            )
            .unwrap();
        }
    }
    let _dpi = super::super::topology::DpiGuard::enter().unwrap();
    let class = wide(&format!(
        "TokenPulse.Menu.Test.{}",
        uuid::Uuid::new_v4().simple()
    ));
    let module = unsafe { GetModuleHandleW(ptr::null()) };
    let info = WNDCLASSEXW {
        cbSize: mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(procedure),
        hInstance: module,
        lpszClassName: class.as_ptr(),
        ..unsafe { mem::zeroed() }
    };
    assert_ne!(unsafe { RegisterClassExW(&info) }, 0);
    let window = unsafe {
        CreateWindowExW(
            0,
            class.as_ptr(),
            wide("Menu test").as_ptr(),
            WS_POPUP,
            100,
            400,
            1,
            1,
            ptr::null_mut(),
            ptr::null_mut(),
            module,
            ptr::null(),
        )
    };
    assert!(!window.is_null());
    for (key, expected) in [
        ('f', Some(HostAction::OpenFloat {})),
        ('s', Some(HostAction::OpenStats {})),
        ('t', Some(HostAction::OpenTaskbarSettings {})),
        ('p', Some(HostAction::SetPrivacy { enabled: false })),
        ('h', Some(HostAction::DisableTaskbar {})),
        ('\u{1b}', None),
        ('\0', None),
    ] {
        COMMAND.with(|command| command.set(key as u16));
        assert_ne!(unsafe { SetTimer(window, 1, 100, None) }, 0);
        assert_eq!(
            show(
                window,
                Some(POINT { x: 100, y: 400 }),
                true,
                AppTheme::Dark,
                144
            )
            .unwrap(),
            expected
        );
        assert!(ACTIVE.with(|active| active.borrow().is_none()));
    }
    assert!(DRAWN.with(Cell::get) > 0);
    unsafe {
        DestroyWindow(window);
        UnregisterClassW(class.as_ptr(), module);
    }
}
