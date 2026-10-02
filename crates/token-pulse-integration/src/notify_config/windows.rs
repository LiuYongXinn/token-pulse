//! Bounded explicit config access and conditional Windows file-transaction application.
//! No auth.json, source logs, full-config backup files, or unsafe replacement fallback.
mod transaction;
use super::{ConfigError, MAX_CONFIG_BYTES, ManagedNotifyCommand, NotifyRestoreRecord};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    mem,
    os::windows::io::{AsRawHandle, FromRawHandle},
    path::{Component, Path, PathBuf, Prefix},
    ptr,
};
use windows_sys::Win32::{
    Foundation::{ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND, INVALID_HANDLE_VALUE},
    Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, CreateFileW, FILE_ATTRIBUTE_DIRECTORY,
        FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_GENERIC_READ, FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE,
        GetFileInformationByHandle, OPEN_EXISTING,
    },
};

/// Finite errors never include input contents, paths or provider parser excerpts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigFileError {
    Config(ConfigError),
    UnsafePath,
    UnsafeFile,
    Busy,
    PermissionDenied,
    Unsupported,
    Io,
}
impl std::fmt::Display for ConfigFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Config(error) => error.fmt(f),
            other => f.write_str(match other {
                Self::UnsafePath => "notify_config_unsafe_path",
                Self::UnsafeFile => "notify_config_unsafe_file",
                Self::Busy => "notify_config_busy",
                Self::PermissionDenied => "notify_config_permission_denied",
                Self::Unsupported => "notify_config_transaction_unavailable",
                _ => "notify_config_io",
            }),
        }
    }
}
impl std::error::Error for ConfigFileError {}
impl From<ConfigError> for ConfigFileError {
    fn from(error: ConfigError) -> Self {
        Self::Config(error)
    }
}
type Identity = (u32, u32, u32);

/// Private full bytes are only temporary memory; the review surface contains notify values only.
/// The physical directory/file and absent-vs-empty state are bound to this preview as well.
pub struct ConfigFilePlan {
    home: PathBuf,
    home_identity: Identity,
    file_identity: Option<Identity>,
    expected_digest: [u8; 32],
    edited: Vec<u8>,
    record: NotifyRestoreRecord,
    before_value: Option<String>,
    after_value: Option<String>,
}
impl ConfigFilePlan {
    pub fn home(&self) -> &Path {
        &self.home
    }
    pub fn creates_config(&self) -> bool {
        self.file_identity.is_none()
    }
    pub fn before_value(&self) -> Option<&str> {
        self.before_value.as_deref()
    }
    pub fn after_value(&self) -> Option<&str> {
        self.after_value.as_deref()
    }
    pub fn restore_record(&self) -> &NotifyRestoreRecord {
        &self.record
    }
    /// No partial update becomes visible: read, revalidation, write and publication are in one
    /// supported OS transaction. Missing APIs/filesystem support return Unsupported unchanged.
    pub fn apply(&self) -> Result<(), ConfigFileError> {
        self.stage()?.commit()
    }
    fn stage(&self) -> Result<StagedWrite, ConfigFileError> {
        let root = open_home(&self.home)?;
        if information(&root)?.0 != self.home_identity {
            return Err(ConfigError::StalePlan.into());
        }
        let transaction = transaction::Transaction::begin()?;
        let mut file = transaction
            .open_config(&self.home.join("config.toml"), self.file_identity.is_none())?;
        let (identity, info) = information(&file)?;
        require_config(&info)?;
        if self
            .file_identity
            .is_some_and(|expected| expected != identity)
        {
            return Err(ConfigError::StalePlan.into());
        }
        let current = read_bytes(&mut file)?;
        if super::digest(&current) != self.expected_digest {
            return Err(ConfigError::StalePlan.into());
        }
        file.seek(SeekFrom::Start(0))
            .map_err(transaction::io_error)?;
        file.write_all(&self.edited)
            .map_err(transaction::io_error)?;
        file.set_len(self.edited.len() as u64)
            .map_err(transaction::io_error)?;
        file.sync_all().map_err(transaction::io_error)?;
        // TxF requires file handles closed before commit. Its reservation remains until commit,
        // so ordinary in-place writers or path replacements cannot enter this last boundary.
        drop(file);
        Ok(StagedWrite {
            transaction,
            _root: root,
        })
    }
}
struct StagedWrite {
    transaction: transaction::Transaction,
    _root: File,
}
impl StagedWrite {
    fn commit(self) -> Result<(), ConfigFileError> {
        self.transaction.commit()
    }
}

pub fn prepare_enable_file(
    home: &Path,
    command: &ManagedNotifyCommand,
) -> Result<ConfigFilePlan, ConfigFileError> {
    let snapshot = snapshot(home)?;
    let current = snapshot.bytes.as_deref().unwrap_or_default();
    let plan = super::prepare_enable(current, command)?;
    let before_value = plan.record.original_value().map(str::to_owned);
    Ok(ConfigFilePlan {
        home: snapshot.home,
        home_identity: snapshot.home_identity,
        file_identity: snapshot.file_identity,
        expected_digest: plan.expected_digest,
        edited: plan.edited,
        record: plan.record,
        before_value,
        after_value: Some(plan.next_value),
    })
}

/// Prepare against the latest contents. Other new settings survive undo; a changed / absent
/// notify returns OwnershipConflict, never overwrites another program's current command.
pub fn prepare_restore_file(
    home: &Path,
    record: &NotifyRestoreRecord,
) -> Result<ConfigFilePlan, ConfigFileError> {
    let snapshot = snapshot(home)?;
    let current = snapshot
        .bytes
        .as_deref()
        .ok_or(ConfigError::OwnershipConflict)?;
    let edited = super::restore(current, record)?;
    let source = super::config_text(current)?;
    let before_value = super::parse(source)?
        .field
        .map(|field| source[field.value].to_owned());
    Ok(ConfigFilePlan {
        home: snapshot.home,
        home_identity: snapshot.home_identity,
        file_identity: snapshot.file_identity,
        expected_digest: super::digest(current),
        edited,
        record: record.clone(),
        before_value,
        after_value: record.original_value().map(str::to_owned),
    })
}

/// Compatible read-only headless / owner API. It never starts a file transaction or writes.
pub fn read_config(home: &Path) -> Result<Vec<u8>, ConfigError> {
    snapshot(home)
        .map_err(|error| match error {
            ConfigFileError::Config(error) => error,
            _ => ConfigError::InvalidToml,
        })?
        .bytes
        .ok_or(ConfigError::InvalidToml)
}
struct Snapshot {
    home: PathBuf,
    home_identity: Identity,
    file_identity: Option<Identity>,
    bytes: Option<Vec<u8>>,
}
fn snapshot(home: &Path) -> Result<Snapshot, ConfigFileError> {
    let root = open_home(home)?;
    let home_identity = information(&root)?.0;
    let home = home.canonicalize().map_err(transaction::io_error)?;
    let canonical_root = open_home(&home)?;
    if information(&canonical_root)?.0 != home_identity {
        return Err(ConfigFileError::UnsafePath);
    }
    let path = wide(&home.join("config.toml"))?;
    let raw = unsafe {
        CreateFileW(
            path.as_ptr(),
            FILE_GENERIC_READ,
            FILE_SHARE_READ,
            ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS,
            ptr::null_mut(),
        )
    };
    if raw == INVALID_HANDLE_VALUE {
        let error = std::io::Error::last_os_error();
        return match error.raw_os_error().map(|code| code as u32) {
            Some(ERROR_FILE_NOT_FOUND) => Ok(Snapshot {
                home,
                home_identity,
                file_identity: None,
                bytes: None,
            }),
            _ => Err(transaction::io_error(error)),
        };
    }
    let mut file = unsafe { File::from_raw_handle(raw.cast()) };
    let (file_identity, info) = information(&file)?;
    require_config(&info)?;
    let bytes = read_bytes(&mut file)?;
    Ok(Snapshot {
        home,
        home_identity,
        file_identity: Some(file_identity),
        bytes: Some(bytes),
    })
}
fn local_path(path: &Path) -> bool {
    path.is_absolute()
        && matches!(path.components().next(), Some(Component::Prefix(prefix)) if matches!(prefix.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_)))
}
fn open_home(home: &Path) -> Result<File, ConfigFileError> {
    if !local_path(home) {
        return Err(ConfigFileError::UnsafePath);
    }
    let path = wide(home)?;
    let raw = unsafe {
        CreateFileW(
            path.as_ptr(),
            FILE_READ_ATTRIBUTES,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            ptr::null_mut(),
        )
    };
    if raw == INVALID_HANDLE_VALUE {
        let error = std::io::Error::last_os_error();
        return Err(match error.raw_os_error().map(|code| code as u32) {
            Some(ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND) => ConfigFileError::UnsafePath,
            _ => transaction::io_error(error),
        });
    }
    let file = unsafe { File::from_raw_handle(raw.cast()) };
    let (_, info) = information(&file)?;
    if info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY == 0
        || info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
    {
        return Err(ConfigFileError::UnsafePath);
    }
    Ok(file)
}
fn information(file: &File) -> Result<(Identity, BY_HANDLE_FILE_INFORMATION), ConfigFileError> {
    unsafe {
        let mut info: BY_HANDLE_FILE_INFORMATION = mem::zeroed();
        if GetFileInformationByHandle(file.as_raw_handle().cast(), &mut info) == 0 {
            return Err(transaction::last_error());
        }
        Ok((
            (
                info.dwVolumeSerialNumber,
                info.nFileIndexHigh,
                info.nFileIndexLow,
            ),
            info,
        ))
    }
}
fn require_config(info: &BY_HANDLE_FILE_INFORMATION) -> Result<(), ConfigFileError> {
    if info.dwFileAttributes & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT) != 0
        || info.nNumberOfLinks != 1
    {
        return Err(ConfigFileError::UnsafeFile);
    }
    Ok(())
}
fn read_bytes(file: &mut File) -> Result<Vec<u8>, ConfigFileError> {
    if file.metadata().map_err(transaction::io_error)?.len() > MAX_CONFIG_BYTES as u64 {
        return Err(ConfigError::TooLarge.into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_CONFIG_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(transaction::io_error)?;
    if bytes.len() > MAX_CONFIG_BYTES {
        return Err(ConfigError::TooLarge.into());
    }
    Ok(bytes)
}
fn wide(path: &Path) -> Result<Vec<u16>, ConfigFileError> {
    use std::os::windows::ffi::OsStrExt;
    let mut value: Vec<_> = path.as_os_str().encode_wide().collect();
    if value.contains(&0) || value.len() >= 32767 {
        return Err(ConfigFileError::UnsafePath);
    }
    value.push(0);
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_SHARE_DELETE, MOVEFILE_REPLACE_EXISTING, MoveFileExW,
    };
    fn scene() -> (tempfile::TempDir, ManagedNotifyCommand) {
        (
            tempfile::tempdir().unwrap(),
            ManagedNotifyCommand::new(
                &std::env::current_exe().unwrap(),
                "0123456789abcdef0123456789abcdef",
            )
            .unwrap(),
        )
    }
    fn outside_read(path: &Path) -> Vec<u8> {
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .open(path)
            .unwrap();
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).unwrap();
        bytes
    }
    #[test]
    fn staged_config_is_old_outside_and_competing_inplace_and_replacement_writes_are_refused() {
        let (home, command) = scene();
        let path = home.path().join("config.toml");
        let before = format!("# {}\nmodel='keep'\n", "synthetic comment ".repeat(8192));
        std::fs::write(&path, before.as_bytes()).unwrap();
        let plan = prepare_enable_file(home.path(), &command).unwrap();
        let staged = plan.stage().unwrap();
        assert_eq!(outside_read(&path), before.as_bytes());
        assert!(std::fs::write(&path, b"notify=['external']\n").is_err());
        let external = home.path().join("editor-stage");
        std::fs::write(&external, b"notify=['external']\n").unwrap();
        unsafe {
            assert_eq!(
                MoveFileExW(
                    wide(&external).unwrap().as_ptr(),
                    wide(&path).unwrap().as_ptr(),
                    MOVEFILE_REPLACE_EXISTING
                ),
                0
            );
        }
        assert_eq!(outside_read(&path), before.as_bytes());
        assert_eq!(std::fs::read(&external).unwrap(), b"notify=['external']\n");
        staged.commit().unwrap();
        assert_eq!(
            std::fs::read(&path).unwrap(),
            format!("notify = {}\n{before}", plan.after_value().unwrap()).as_bytes()
        );
        assert_eq!(std::fs::read_dir(home.path()).unwrap().count(), 2);
    }
    #[test]
    fn dropped_staged_update_is_invisible_and_original_is_unchanged() {
        let (home, command) = scene();
        let path = home.path().join("config.toml");
        let before = b"notify=['original.exe']\nmodel='keep'\n";
        std::fs::write(&path, before).unwrap();
        let plan = prepare_enable_file(home.path(), &command).unwrap();
        let staged = plan.stage().unwrap();
        assert_eq!(outside_read(&path), before);
        drop(staged);
        assert_eq!(read_config(home.path()).unwrap(), before);
        assert_eq!(std::fs::read_dir(home.path()).unwrap().count(), 1);
        plan.apply().unwrap();
    }
    #[test]
    fn absent_creation_reserves_name_and_drop_leaves_no_file() {
        let (home, command) = scene();
        let path = home.path().join("config.toml");
        let plan = prepare_enable_file(home.path(), &command).unwrap();
        assert!(plan.creates_config());
        let staged = plan.stage().unwrap();
        assert!(!path.exists());
        assert!(std::fs::write(&path, b"user-created=true\n").is_err());
        drop(staged);
        assert!(!path.exists());
        assert_eq!(std::fs::read_dir(home.path()).unwrap().count(), 0);
        plan.apply().unwrap();
        assert_eq!(
            std::fs::read(&path).unwrap(),
            format!("notify = {}\n", plan.after_value().unwrap()).as_bytes()
        );
    }
    #[test]
    fn second_transaction_is_busy_then_retry_succeeds_without_partial_bytes() {
        let (home, command) = scene();
        let path = home.path().join("config.toml");
        std::fs::write(&path, b"model='keep'\n").unwrap();
        let first = prepare_enable_file(home.path(), &command).unwrap();
        let second = prepare_enable_file(home.path(), &command).unwrap();
        let staged = first.stage().unwrap();
        assert_eq!(second.apply(), Err(ConfigFileError::Busy));
        drop(staged);
        second.apply().unwrap();
        assert_eq!(std::fs::read_dir(home.path()).unwrap().count(), 1);
    }
    #[test]
    fn directory_is_pinned_through_commit_and_same_bytes_on_replaced_home_are_stale() {
        let parent = tempfile::tempdir().unwrap();
        let home = parent.path().join("home");
        std::fs::create_dir(&home).unwrap();
        std::fs::write(home.join("config.toml"), b"model='keep'\n").unwrap();
        let command = scene().1;
        let plan = prepare_enable_file(&home, &command).unwrap();
        let staged = plan.stage().unwrap();
        assert!(std::fs::rename(&home, parent.path().join("moved")).is_err());
        drop(staged);
        std::fs::rename(&home, parent.path().join("moved")).unwrap();
        std::fs::create_dir(&home).unwrap();
        std::fs::write(home.join("config.toml"), b"model='keep'\n").unwrap();
        assert_eq!(plan.apply(), Err(ConfigError::StalePlan.into()));
        assert_eq!(
            std::fs::read(home.join("config.toml")).unwrap(),
            b"model='keep'\n"
        );
    }
    #[test]
    fn unavailable_provider_and_conflict_codes_are_finite() {
        assert_eq!(
            transaction::io_error(std::io::Error::from_raw_os_error(50)),
            ConfigFileError::Unsupported
        );
        assert_eq!(
            ConfigFileError::Unsupported.to_string(),
            "notify_config_transaction_unavailable"
        );
        assert_eq!(
            transaction::io_error(std::io::Error::from_raw_os_error(6800)),
            ConfigFileError::Busy
        );
    }
}
