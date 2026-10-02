use super::super::RegistryError;
use std::{
    fs::File,
    mem,
    os::windows::{
        fs::MetadataExt,
        io::{AsRawHandle, FromRawHandle},
    },
    path::Path,
    ptr,
};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, ERROR_ALREADY_EXISTS, ERROR_FILE_EXISTS, ERROR_FILE_NOT_FOUND,
        ERROR_PATH_NOT_FOUND, HANDLE, INVALID_HANDLE_VALUE, LocalFree,
    },
    Security::{
        ACCESS_ALLOWED_ACE, ACL_SIZE_INFORMATION, AclSizeInformation,
        Authorization::{
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
            GetSecurityInfo, SDDL_REVISION_1, SE_FILE_OBJECT,
        },
        DACL_SECURITY_INFORMATION, GetAce, GetAclInformation, GetSecurityDescriptorControl,
        GetTokenInformation, OWNER_SECURITY_INFORMATION, SE_DACL_PROTECTED, SECURITY_ATTRIBUTES,
        TOKEN_QUERY, TOKEN_USER, TokenUser,
    },
    Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, CREATE_NEW, CreateDirectoryW, CreateFileW, FILE_ALL_ACCESS,
        FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_REPARSE_POINT,
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_READ,
        FILE_GENERIC_WRITE, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ,
        FILE_SHARE_WRITE, GetFileInformationByHandle, MOVEFILE_WRITE_THROUGH, MoveFileExW,
        OPEN_ALWAYS, OPEN_EXISTING, READ_CONTROL,
    },
    System::Threading::{GetCurrentProcess, OpenProcessToken},
};
struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
struct LocalMemory(*mut std::ffi::c_void);
impl Drop for LocalMemory {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}
fn last() -> RegistryError {
    match std::io::Error::last_os_error().raw_os_error() {
        Some(code)
            if code == ERROR_FILE_NOT_FOUND as i32 || code == ERROR_PATH_NOT_FOUND as i32 =>
        {
            RegistryError::NotFound
        }
        Some(code) if code == ERROR_ALREADY_EXISTS as i32 || code == ERROR_FILE_EXISTS as i32 => {
            RegistryError::AlreadyExists
        }
        _ => RegistryError::Io,
    }
}
fn wide(path: &Path) -> Result<Vec<u16>, RegistryError> {
    use std::os::windows::ffi::OsStrExt;
    let mut wide: Vec<_> = path.as_os_str().encode_wide().collect();
    if wide.contains(&0) {
        return Err(RegistryError::UnsafePath);
    }
    wide.push(0);
    Ok(wide)
}
unsafe fn sid_text(sid: *mut std::ffi::c_void) -> Result<String, RegistryError> {
    unsafe {
        let mut text = ptr::null_mut();
        if ConvertSidToStringSidW(sid, &mut text) == 0 {
            return Err(RegistryError::UnsafePermissions);
        }
        let _memory = LocalMemory(text.cast());
        let mut length = 0;
        while *text.add(length) != 0 {
            length += 1;
        }
        String::from_utf16(std::slice::from_raw_parts(text, length))
            .map_err(|_| RegistryError::UnsafePermissions)
    }
}
fn user_sid() -> Result<String, RegistryError> {
    unsafe {
        let mut raw = ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw) == 0 {
            return Err(RegistryError::UnsafePermissions);
        }
        let token = Handle(raw);
        let mut length = 0;
        GetTokenInformation(token.0, TokenUser, ptr::null_mut(), 0, &mut length);
        if length == 0 || length > 65_536 {
            return Err(RegistryError::UnsafePermissions);
        }
        let mut buffer = vec![0usize; (length as usize).div_ceil(mem::size_of::<usize>())];
        if GetTokenInformation(
            token.0,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            length,
            &mut length,
        ) == 0
        {
            return Err(RegistryError::UnsafePermissions);
        }
        sid_text((*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid)
    }
}
fn private_descriptor(directory: bool) -> Result<LocalMemory, RegistryError> {
    let sid = user_sid()?;
    let inheritance = if directory { "OICI" } else { "" };
    let sddl: Vec<u16> = format!("O:{sid}D:P(A;{inheritance};FA;;;{sid})")
        .encode_utf16()
        .chain(Some(0))
        .collect();
    unsafe {
        let mut descriptor = ptr::null_mut();
        if ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            ptr::null_mut(),
        ) == 0
        {
            return Err(RegistryError::UnsafePermissions);
        }
        Ok(LocalMemory(descriptor))
    }
}
pub(super) fn require_plain_directory(path: &Path) -> Result<(), RegistryError> {
    let metadata = std::fs::symlink_metadata(path).map_err(|_| RegistryError::UnsafePath)?;
    if !metadata.is_dir() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(RegistryError::UnsafePath);
    }
    Ok(())
}
pub(super) fn create_private_directory(path: &Path) -> Result<(), RegistryError> {
    let path = wide(path)?;
    let descriptor = private_descriptor(true)?;
    let attributes = SECURITY_ATTRIBUTES {
        nLength: mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.0,
        bInheritHandle: 0,
    };
    unsafe {
        if CreateDirectoryW(path.as_ptr(), &attributes) == 0 {
            match last() {
                RegistryError::AlreadyExists => {}
                error => return Err(error),
            }
        }
    }
    Ok(())
}
fn open_private(path: &Path, disposition: u32, share: u32) -> Result<File, RegistryError> {
    let path = wide(path)?;
    let descriptor = private_descriptor(false)?;
    let attributes = SECURITY_ATTRIBUTES {
        nLength: mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.0,
        bInheritHandle: 0,
    };
    unsafe {
        let handle = CreateFileW(
            path.as_ptr(),
            FILE_GENERIC_READ | FILE_GENERIC_WRITE,
            share,
            &attributes,
            disposition,
            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OPEN_REPARSE_POINT,
            ptr::null_mut(),
        );
        if handle == INVALID_HANDLE_VALUE {
            return Err(last());
        }
        let file = File::from_raw_handle(handle.cast());
        verify_handle(&file, false)?;
        Ok(file)
    }
}
pub(super) fn create_private_file(path: &Path) -> Result<File, RegistryError> {
    open_private(
        path,
        CREATE_NEW,
        FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
    )
}
pub(super) fn allocation_guard(path: &Path) -> Result<File, RegistryError> {
    let file = open_private(path, OPEN_ALWAYS, 0)?;
    if file.metadata().map_err(|_| RegistryError::Io)?.len() != 0 {
        return Err(RegistryError::InvalidRecord);
    }
    Ok(file)
}
pub(super) fn open_verified(path: &Path, directory: bool) -> Result<File, RegistryError> {
    open_verified_with_sharing(path, directory, directory)
}
pub(super) fn open_marker(path: &Path) -> Result<File, RegistryError> {
    open_verified_with_sharing(path, false, true)
}
fn open_verified_with_sharing(
    path: &Path,
    directory: bool,
    share_all: bool,
) -> Result<File, RegistryError> {
    let path = wide(path)?;
    unsafe {
        let access = if directory {
            FILE_READ_ATTRIBUTES | READ_CONTROL
        } else {
            FILE_GENERIC_READ
        };
        let flags = FILE_FLAG_OPEN_REPARSE_POINT
            | if directory {
                FILE_FLAG_BACKUP_SEMANTICS
            } else {
                0
            };
        let handle = CreateFileW(
            path.as_ptr(),
            access,
            FILE_SHARE_READ
                | if share_all {
                    FILE_SHARE_WRITE | FILE_SHARE_DELETE
                } else {
                    0
                },
            ptr::null(),
            OPEN_EXISTING,
            flags,
            ptr::null_mut(),
        );
        if handle == INVALID_HANDLE_VALUE {
            return Err(last());
        }
        let file = File::from_raw_handle(handle.cast());
        verify_handle(&file, directory)?;
        Ok(file)
    }
}
fn verify_handle(file: &File, directory: bool) -> Result<(), RegistryError> {
    unsafe {
        let handle = file.as_raw_handle().cast();
        let mut info: BY_HANDLE_FILE_INFORMATION = mem::zeroed();
        if GetFileInformationByHandle(handle, &mut info) == 0 {
            return Err(RegistryError::Io);
        }
        if info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
            || (info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0) != directory
            || (!directory && info.nNumberOfLinks != 1)
        {
            return Err(RegistryError::UnsafePath);
        }
        let mut owner = ptr::null_mut();
        let mut acl = ptr::null_mut();
        let mut descriptor = ptr::null_mut();
        if GetSecurityInfo(
            handle,
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            ptr::null_mut(),
            &mut acl,
            ptr::null_mut(),
            &mut descriptor,
        ) != 0
        {
            return Err(RegistryError::UnsafePermissions);
        }
        let _memory = LocalMemory(descriptor);
        if acl.is_null() || owner.is_null() {
            return Err(RegistryError::UnsafePermissions);
        }
        let sid = user_sid()?;
        if sid_text(owner)? != sid {
            return Err(RegistryError::UnsafePermissions);
        }
        let mut control = 0;
        let mut revision = 0;
        if GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) == 0
            || control & SE_DACL_PROTECTED == 0
        {
            return Err(RegistryError::UnsafePermissions);
        }
        let mut acl_info: ACL_SIZE_INFORMATION = mem::zeroed();
        if GetAclInformation(
            acl,
            (&mut acl_info as *mut ACL_SIZE_INFORMATION).cast(),
            mem::size_of_val(&acl_info) as u32,
            AclSizeInformation,
        ) == 0
            || acl_info.AceCount != 1
        {
            return Err(RegistryError::UnsafePermissions);
        }
        let mut raw = ptr::null_mut();
        if GetAce(acl, 0, &mut raw) == 0 {
            return Err(RegistryError::UnsafePermissions);
        }
        let ace = &*raw.cast::<ACCESS_ALLOWED_ACE>();
        if ace.Header.AceType != 0
            || ace.Mask & FILE_ALL_ACCESS != FILE_ALL_ACCESS
            || sid_text((&ace.SidStart as *const u32).cast_mut().cast())? != sid
        {
            return Err(RegistryError::UnsafePermissions);
        }
    }
    Ok(())
}
pub(super) fn move_new(source: &Path, destination: &Path) -> Result<(), RegistryError> {
    let source = wide(source)?;
    let destination = wide(destination)?;
    unsafe {
        if MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_WRITE_THROUGH,
        ) == 0
        {
            return Err(last());
        }
    }
    Ok(())
}
