//! A bounded read of the explicit Home/config.toml. Never auth.json or source logs.
use super::{ConfigError, MAX_CONFIG_BYTES};
use std::{
    fs::File,
    io::Read,
    mem,
    os::windows::{fs::MetadataExt, io::FromRawHandle},
    path::Path,
    ptr,
};
use windows_sys::Win32::{
    Foundation::INVALID_HANDLE_VALUE,
    Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, CreateFileW, FILE_ATTRIBUTE_DIRECTORY,
        FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_READ,
        FILE_SHARE_READ, GetFileInformationByHandle, OPEN_EXISTING,
    },
};

pub fn read_config(home: &Path) -> Result<Vec<u8>, ConfigError> {
    use std::os::windows::ffi::OsStrExt;
    let metadata = std::fs::symlink_metadata(home).map_err(|_| ConfigError::InvalidToml)?;
    if !home.is_absolute()
        || !metadata.is_dir()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    {
        return Err(ConfigError::InvalidToml);
    }
    let mut path: Vec<_> = home.join("config.toml").as_os_str().encode_wide().collect();
    if path.contains(&0) {
        return Err(ConfigError::InvalidToml);
    }
    path.push(0);
    let file = unsafe {
        let raw = CreateFileW(
            path.as_ptr(),
            FILE_GENERIC_READ,
            FILE_SHARE_READ,
            ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_OPEN_REPARSE_POINT,
            ptr::null_mut(),
        );
        if raw == INVALID_HANDLE_VALUE {
            return Err(ConfigError::InvalidToml);
        }
        let file = File::from_raw_handle(raw.cast());
        let mut info: BY_HANDLE_FILE_INFORMATION = mem::zeroed();
        if GetFileInformationByHandle(raw, &mut info) == 0
            || info.dwFileAttributes & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT)
                != 0
            || info.nNumberOfLinks != 1
        {
            return Err(ConfigError::InvalidToml);
        }
        file
    };
    if file.metadata().map_err(|_| ConfigError::InvalidToml)?.len() > MAX_CONFIG_BYTES as u64 {
        return Err(ConfigError::TooLarge);
    }
    let mut bytes = Vec::new();
    file.take(MAX_CONFIG_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ConfigError::InvalidToml)?;
    if bytes.len() > MAX_CONFIG_BYTES {
        return Err(ConfigError::TooLarge);
    }
    Ok(bytes)
}
