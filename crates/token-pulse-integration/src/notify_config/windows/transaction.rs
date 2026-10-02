//! Optional NTFS transaction provider, loaded from System32 only. No product startup dependency
//! on KTM, no fallback that weakens conditional application if the provider is unavailable.
use super::{ConfigError, ConfigFileError, wide};
use std::{ffi::c_void, fs::File, os::windows::io::FromRawHandle, path::Path, ptr};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, ERROR_ACCESS_DENIED, ERROR_ALREADY_EXISTS,
        ERROR_EFS_NOT_ALLOWED_IN_TRANSACTION, ERROR_FILE_EXISTS, ERROR_FILE_NOT_FOUND,
        ERROR_INVALID_FUNCTION, ERROR_LOCK_VIOLATION, ERROR_NOT_SUPPORTED, ERROR_PATH_NOT_FOUND,
        ERROR_RM_NOT_ACTIVE, ERROR_SHARING_VIOLATION, ERROR_TRANSACTIONAL_CONFLICT,
        ERROR_TRANSACTIONS_UNSUPPORTED_REMOTE, FreeLibrary, HANDLE, HMODULE, INVALID_HANDLE_VALUE,
    },
    Security::SECURITY_ATTRIBUTES,
    Storage::FileSystem::{
        CREATE_NEW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_READ,
        FILE_GENERIC_WRITE, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    },
    System::LibraryLoader::{
        GetModuleHandleW, GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW,
    },
};
type Create = unsafe extern "system" fn(
    *mut SECURITY_ATTRIBUTES,
    *mut windows_sys::core::GUID,
    u32,
    u32,
    u32,
    u32,
    *const u16,
) -> HANDLE;
type Finish = unsafe extern "system" fn(HANDLE) -> i32;
type Open = unsafe extern "system" fn(
    *const u16,
    u32,
    u32,
    *const SECURITY_ATTRIBUTES,
    u32,
    u32,
    HANDLE,
    HANDLE,
    *const u16,
    *const c_void,
) -> HANDLE;
struct Api {
    library: HMODULE,
    create: Create,
    commit: Finish,
    rollback: Finish,
    open: Open,
}
impl Api {
    fn load() -> Result<Self, ConfigFileError> {
        unsafe {
            let name: Vec<_> = "ktmw32.dll".encode_utf16().chain(Some(0)).collect();
            let library =
                LoadLibraryExW(name.as_ptr(), ptr::null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32);
            if library.is_null() {
                return Err(ConfigFileError::Unsupported);
            }
            // Keep the library owned before resolving: failures also unload our reference.
            let guard = Library(library);
            let kernel: Vec<_> = "kernel32.dll".encode_utf16().chain(Some(0)).collect();
            let kernel = GetModuleHandleW(kernel.as_ptr());
            if kernel.is_null() {
                return Err(ConfigFileError::Unsupported);
            }
            let create = GetProcAddress(library, c"CreateTransaction".as_ptr().cast())
                .ok_or(ConfigFileError::Unsupported)?;
            let commit = GetProcAddress(library, c"CommitTransaction".as_ptr().cast())
                .ok_or(ConfigFileError::Unsupported)?;
            let rollback = GetProcAddress(library, c"RollbackTransaction".as_ptr().cast())
                .ok_or(ConfigFileError::Unsupported)?;
            let open = GetProcAddress(kernel, c"CreateFileTransactedW".as_ptr().cast())
                .ok_or(ConfigFileError::Unsupported)?;
            std::mem::forget(guard);
            Ok(Self {
                library,
                create: std::mem::transmute::<unsafe extern "system" fn() -> isize, Create>(create),
                commit: std::mem::transmute::<unsafe extern "system" fn() -> isize, Finish>(commit),
                rollback: std::mem::transmute::<unsafe extern "system" fn() -> isize, Finish>(
                    rollback,
                ),
                open: std::mem::transmute::<unsafe extern "system" fn() -> isize, Open>(open),
            })
        }
    }
}
struct Library(HMODULE);
impl Drop for Library {
    fn drop(&mut self) {
        unsafe {
            FreeLibrary(self.0);
        }
    }
}
impl Drop for Api {
    fn drop(&mut self) {
        unsafe {
            FreeLibrary(self.library);
        }
    }
}
pub(super) struct Transaction {
    handle: HANDLE,
    api: Api,
    finished: bool,
}
impl Transaction {
    pub(super) fn begin() -> Result<Self, ConfigFileError> {
        let api = Api::load()?;
        let description: Vec<_> = "TokenPulse notify config"
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let handle = unsafe {
            (api.create)(
                ptr::null_mut(),
                ptr::null_mut(),
                0,
                0,
                0,
                5000,
                description.as_ptr(),
            )
        };
        if handle == INVALID_HANDLE_VALUE || handle.is_null() {
            return Err(last_error());
        }
        Ok(Self {
            handle,
            api,
            finished: false,
        })
    }
    pub(super) fn open_config(&self, path: &Path, create: bool) -> Result<File, ConfigFileError> {
        let path = wide(path)?;
        let raw = unsafe {
            (self.api.open)(
                path.as_ptr(),
                FILE_GENERIC_READ | FILE_GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                ptr::null(),
                if create { CREATE_NEW } else { OPEN_EXISTING },
                FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS,
                ptr::null_mut(),
                self.handle,
                ptr::null(),
                ptr::null(),
            )
        };
        if raw == INVALID_HANDLE_VALUE {
            let error = std::io::Error::last_os_error();
            return Err(match error.raw_os_error().map(|code| code as u32) {
                Some(
                    ERROR_ALREADY_EXISTS | ERROR_FILE_EXISTS | ERROR_FILE_NOT_FOUND
                    | ERROR_PATH_NOT_FOUND,
                ) => ConfigError::StalePlan.into(),
                _ => io_error(error),
            });
        }
        Ok(unsafe { File::from_raw_handle(raw.cast()) })
    }
    pub(super) fn commit(mut self) -> Result<(), ConfigFileError> {
        if unsafe { (self.api.commit)(self.handle) } == 0 {
            return Err(last_error());
        }
        self.finished = true;
        Ok(())
    }
}
impl Drop for Transaction {
    fn drop(&mut self) {
        unsafe {
            if !self.finished {
                (self.api.rollback)(self.handle);
            }
            CloseHandle(self.handle);
        }
    }
}
pub(super) fn last_error() -> ConfigFileError {
    io_error(std::io::Error::last_os_error())
}
pub(super) fn io_error(error: std::io::Error) -> ConfigFileError {
    match error.raw_os_error().map(|code| code as u32) {
        Some(ERROR_ACCESS_DENIED) => ConfigFileError::PermissionDenied,
        Some(ERROR_SHARING_VIOLATION | ERROR_LOCK_VIOLATION | ERROR_TRANSACTIONAL_CONFLICT) => {
            ConfigFileError::Busy
        }
        Some(
            ERROR_NOT_SUPPORTED
            | ERROR_INVALID_FUNCTION
            | ERROR_RM_NOT_ACTIVE
            | ERROR_TRANSACTIONS_UNSUPPORTED_REMOTE
            | ERROR_EFS_NOT_ALLOWED_IN_TRANSACTION,
        ) => ConfigFileError::Unsupported,
        _ => ConfigFileError::Io,
    }
}
