//! Explicit development-only native acceptance fixture. Never changes process grouping.
use super::{TransportError, topology::wide};
use std::{marker::PhantomData, ptr, rc::Rc};
use windows::Win32::{
    Foundation::HWND,
    Storage::EnhancedStorage::PKEY_AppUserModel_ID,
    System::Com::{
        COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize, StructuredStorage::PROPVARIANT,
    },
    UI::Shell::PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow},
};
use windows_sys::Win32::{
    System::LibraryLoader::GetModuleHandleW,
    UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, SW_SHOWNOACTIVATE, ShowWindow, WS_EX_APPWINDOW,
        WS_EX_NOACTIVATE, WS_OVERLAPPEDWINDOW,
    },
};
pub struct TaskButtonFixture {
    window: windows_sys::Win32::Foundation::HWND,
    store: Option<IPropertyStore>,
    _thread: PhantomData<Rc<()>>,
}
impl TaskButtonFixture {
    pub fn create() -> Result<Self, TransportError> {
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
            .ok()
            .map_err(|_| TransportError::Native)?;
        let window = unsafe {
            CreateWindowExW(
                WS_EX_APPWINDOW | WS_EX_NOACTIVATE,
                wide("STATIC").as_ptr(),
                wide("TokenPulse SYNTHETIC task button").as_ptr(),
                WS_OVERLAPPEDWINDOW,
                80,
                150,
                160,
                120,
                ptr::null_mut(),
                ptr::null_mut(),
                GetModuleHandleW(ptr::null()),
                ptr::null(),
            )
        };
        if window.is_null() {
            unsafe {
                CoUninitialize();
            }
            return Err(TransportError::Native);
        }
        let mut fixture = Self {
            window,
            store: None,
            _thread: PhantomData,
        };
        let store: IPropertyStore = unsafe { SHGetPropertyStoreForWindow(HWND(window)) }
            .map_err(|_| TransportError::Native)?;
        fixture.store = Some(store);
        let id = format!(
            "TokenPulse.DevelopmentFixture.{}",
            uuid::Uuid::new_v4().simple()
        );
        unsafe {
            fixture
                .store
                .as_ref()
                .unwrap()
                .SetValue(&PKEY_AppUserModel_ID, &PROPVARIANT::from(id.as_str()))
        }
        .map_err(|_| TransportError::Native)?;
        unsafe {
            ShowWindow(window, SW_SHOWNOACTIVATE);
        }
        Ok(fixture)
    }
}
impl Drop for TaskButtonFixture {
    fn drop(&mut self) {
        if let Some(store) = self.store.take() {
            // Windows requires clearing the per-window property before destruction.
            unsafe { store.SetValue(&PKEY_AppUserModel_ID, &PROPVARIANT::default()) }.ok();
            drop(store);
        }
        unsafe {
            DestroyWindow(self.window);
            CoUninitialize();
        }
    }
}
