//! Explicitly retained original command. No automatic shell, output capture or payload storage.
use crate::notify_registry::NotifyRegistration;
use std::{
    ffi::OsStr,
    fs::{File, OpenOptions},
    os::windows::{
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
        process::CommandExt,
    },
    path::{Path, PathBuf, Prefix},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE},
    Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
        FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, GetFileInformationByHandle,
    },
    System::{
        Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
        },
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
            SetInformationJobObject,
        },
        Threading::{
            CREATE_NO_WINDOW, CREATE_SUSPENDED, OpenThread, ResumeThread, THREAD_SUSPEND_RESUME,
        },
    },
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OriginalCommandError {
    InvalidCommand,
    ProgramUnavailable,
    Spawn,
    Failed,
    Timeout,
}
impl std::fmt::Display for OriginalCommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidCommand => "notify_original_invalid_command",
            Self::ProgramUnavailable => "notify_original_program_unavailable",
            Self::Spawn => "notify_original_spawn_failed",
            Self::Failed => "notify_original_failed",
            Self::Timeout => "notify_original_timeout",
        })
    }
}
impl std::error::Error for OriginalCommandError {}
const DEADLINE: Duration = Duration::from_secs(5);

/// Called only after current registration/config ownership has been verified. The JSON is
/// borrowed from the OS argument, forwarded unchanged once, and never logged or serialized.
pub(crate) fn run_original(
    record: &NotifyRegistration,
    current_executable: &Path,
    payload: &OsStr,
) -> Result<(), OriginalCommandError> {
    if !record.chain_original() {
        return Err(OriginalCommandError::InvalidCommand);
    }
    let arguments = record
        .restore_record()
        .original_arguments()
        .map_err(|_| OriginalCommandError::InvalidCommand)?
        .filter(|args| args.first().is_some_and(|first| !first.is_empty()))
        .ok_or(OriginalCommandError::InvalidCommand)?;
    let executable = resolve_program(&arguments[0])?;
    let current = current_executable
        .canonicalize()
        .map_err(|_| OriginalCommandError::ProgramUnavailable)?;
    // Case-insensitive canonical paths prevent pointing the legacy command at this dispatcher.
    if executable
        .as_os_str()
        .to_string_lossy()
        .eq_ignore_ascii_case(&current.as_os_str().to_string_lossy())
    {
        return Err(OriginalCommandError::InvalidCommand);
    }
    let guard = guard_program(&executable)?;
    let current_guard = guard_program(&current)?;
    if identity(&guard)? == identity(&current_guard)? {
        return Err(OriginalCommandError::InvalidCommand);
    }
    let child = Command::new(executable)
        .args(&arguments[1..])
        .arg(payload)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED)
        .spawn()
        .map_err(|_| OriginalCommandError::Spawn)?;
    let mut owned = OwnedProcess { child, job: None };
    owned.job = Some(assign_job(&owned.child)?);
    resume(&owned.child)?;
    let until = Instant::now() + DEADLINE;
    loop {
        match owned
            .child
            .try_wait()
            .map_err(|_| OriginalCommandError::Spawn)?
        {
            Some(status) => {
                return if status.success() {
                    Ok(())
                } else {
                    Err(OriginalCommandError::Failed)
                };
            }
            None if Instant::now() >= until => return Err(OriginalCommandError::Timeout),
            None => std::thread::sleep(Duration::from_millis(10)),
        }
    }
}
fn resolve_program(program: &str) -> Result<PathBuf, OriginalCommandError> {
    let local = |path: &Path| matches!(path.components().next(), Some(std::path::Component::Prefix(prefix)) if matches!(prefix.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_)));
    let path = Path::new(program);
    let mut candidates = Vec::new();
    if path.is_absolute() {
        candidates.push(path.to_owned());
    } else if path.components().count() > 1 {
        candidates.push(
            std::env::current_dir()
                .map_err(|_| OriginalCommandError::ProgramUnavailable)?
                .join(path),
        );
    } else {
        // Explicit executable lookup, never PATHEXT batch/script interpretation or cmd wrapping.
        let filename = if path.extension().is_none() {
            format!("{program}.exe")
        } else {
            program.to_owned()
        };
        if let Some(paths) = std::env::var_os("PATH") {
            for directory in std::env::split_paths(&paths).take(128) {
                if directory.is_absolute() && local(&directory) {
                    candidates.push(directory.join(&filename));
                }
            }
        }
    }
    for candidate in candidates {
        if !local(&candidate)
            || !candidate
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
        {
            continue;
        }
        if let Ok(canonical) = candidate.canonicalize() {
            if local(&canonical) && guard_program(&canonical).is_ok() {
                return Ok(canonical);
            }
        }
    }
    Err(OriginalCommandError::ProgramUnavailable)
}
fn guard_program(path: &Path) -> Result<File, OriginalCommandError> {
    let file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|_| OriginalCommandError::ProgramUnavailable)?;
    let metadata = file
        .metadata()
        .map_err(|_| OriginalCommandError::ProgramUnavailable)?;
    if !metadata.is_file()
        || metadata.file_attributes() & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT)
            != 0
    {
        return Err(OriginalCommandError::ProgramUnavailable);
    }
    Ok(file)
}
fn identity(file: &File) -> Result<(u32, u32, u32), OriginalCommandError> {
    unsafe {
        let mut info: BY_HANDLE_FILE_INFORMATION = std::mem::zeroed();
        if GetFileInformationByHandle(file.as_raw_handle().cast(), &mut info) == 0 {
            return Err(OriginalCommandError::ProgramUnavailable);
        }
        Ok((
            info.dwVolumeSerialNumber,
            info.nFileIndexHigh,
            info.nFileIndexLow,
        ))
    }
}
struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
struct OwnedProcess {
    child: Child,
    job: Option<Handle>,
}
impl Drop for OwnedProcess {
    fn drop(&mut self) {
        // Close our non-inheritable Job first: the original and its descendants are ours only.
        self.job.take();
        let _ = self.child.kill();
        let until = Instant::now() + Duration::from_secs(1);
        while self.child.try_wait().is_ok_and(|status| status.is_none()) && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
fn assign_job(child: &Child) -> Result<Handle, OriginalCommandError> {
    unsafe {
        let raw = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if raw.is_null() {
            return Err(OriginalCommandError::Spawn);
        }
        let job = Handle(raw);
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if SetInformationJobObject(
            raw,
            JobObjectExtendedLimitInformation,
            (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            std::mem::size_of_val(&info) as u32,
        ) == 0
            || AssignProcessToJobObject(raw, child.as_raw_handle().cast()) == 0
        {
            return Err(OriginalCommandError::Spawn);
        }
        Ok(job)
    }
}
fn resume(child: &Child) -> Result<(), OriginalCommandError> {
    unsafe {
        let raw = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
        if raw == INVALID_HANDLE_VALUE {
            return Err(OriginalCommandError::Spawn);
        }
        let snapshot = Handle(raw);
        let mut entry: THREADENTRY32 = std::mem::zeroed();
        entry.dwSize = std::mem::size_of_val(&entry) as u32;
        let mut primary = None;
        let mut present = Thread32First(snapshot.0, &mut entry);
        while present != 0 {
            if entry.th32OwnerProcessID == child.id() {
                if primary.is_some() {
                    return Err(OriginalCommandError::Spawn);
                }
                primary = Some(entry.th32ThreadID);
            }
            present = Thread32Next(snapshot.0, &mut entry);
        }
        let thread = OpenThread(
            THREAD_SUSPEND_RESUME,
            0,
            primary.ok_or(OriginalCommandError::Spawn)?,
        );
        if thread.is_null() {
            return Err(OriginalCommandError::Spawn);
        }
        let thread = Handle(thread);
        if ResumeThread(thread.0) != 1 {
            return Err(OriginalCommandError::Spawn);
        }
        Ok(())
    }
}
