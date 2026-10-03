use super::{RestoreDisposition, guardian::Guardian, topology::ProbeError};
use super::{Startup, TransportError};
use std::{
    os::windows::{io::AsRawHandle, process::CommandExt},
    path::Path,
    process::{Child, Command, Stdio},
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE},
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

struct Job(HANDLE);
// Unique, non-inheritable ownership. Closing kills this job's children; no shared raw handle.
unsafe impl Send for Job {}
impl Drop for Job {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
pub struct HostProcess {
    child: Child,
    job: Option<Job>,
    cleanup: Option<Result<RestoreDisposition, ProbeError>>,
    guardian: Option<Guardian>,
}
impl HostProcess {
    pub async fn spawn(executable: &Path, startup: &Startup) -> Result<Self, TransportError> {
        startup.validate()?;
        if !executable.is_absolute()
            || executable
                .file_name()
                .is_none_or(|n| n != "token-pulse-taskbar-host.exe")
        {
            return Err(TransportError::InvalidStartup);
        }
        token_pulse_core::sources::validate_root(
            executable,
            token_pulse_core::sources::SourceOrigin::Custom,
        )
        .map_err(|_| TransportError::InvalidStartup)?;
        let executable = executable
            .canonicalize()
            .map_err(|_| TransportError::Spawn)?;
        if executable
            .file_name()
            .is_none_or(|name| name != "token-pulse-taskbar-host.exe")
        {
            return Err(TransportError::InvalidStartup);
        }
        token_pulse_core::sources::validate_root(
            &executable,
            token_pulse_core::sources::SourceOrigin::Custom,
        )
        .map_err(|_| TransportError::InvalidStartup)?;
        let child = Command::new(&executable)
            .args(startup.arguments())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(acceptance_error_stream())
            .creation_flags(CREATE_SUSPENDED | CREATE_NO_WINDOW)
            .spawn()
            .map_err(|_| TransportError::Spawn)?;
        let mut owned = Self {
            child,
            job: None,
            cleanup: None,
            guardian: None,
        };
        unsafe {
            let raw = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if raw.is_null() {
                return Err(TransportError::Spawn);
            }
            let job = Job(raw);
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                raw,
                JobObjectExtendedLimitInformation,
                (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of_val(&info) as u32,
            ) == 0
                || AssignProcessToJobObject(raw, owned.child.as_raw_handle().cast()) == 0
            {
                return Err(TransportError::Spawn);
            }
            owned.job = Some(job);
        }
        owned.guardian = Some(Guardian::launch(&executable, startup, &owned.child).await?);
        owned.resume()?;
        Ok(owned)
    }
    fn resume(&self) -> Result<(), TransportError> {
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
            if snapshot == INVALID_HANDLE_VALUE {
                return Err(TransportError::Spawn);
            }
            let mut entry: THREADENTRY32 = std::mem::zeroed();
            entry.dwSize = std::mem::size_of_val(&entry) as u32;
            let mut primary = None;
            let mut multiple = false;
            let mut present = Thread32First(snapshot, &mut entry);
            while present != 0 {
                if entry.th32OwnerProcessID == self.child.id() {
                    if primary.is_some() {
                        multiple = true;
                        break;
                    }
                    primary = Some(entry.th32ThreadID);
                }
                present = Thread32Next(snapshot, &mut entry);
            }
            CloseHandle(snapshot);
            if multiple {
                return Err(TransportError::Spawn);
            }
            let thread = OpenThread(
                THREAD_SUSPEND_RESUME,
                0,
                primary.ok_or(TransportError::Spawn)?,
            );
            if thread.is_null() {
                return Err(TransportError::Spawn);
            }
            let previous = ResumeThread(thread);
            CloseHandle(thread);
            if previous != 1 {
                return Err(TransportError::Spawn);
            }
            Ok(())
        }
    }
    pub fn id(&self) -> u32 {
        self.child.id()
    }
    pub fn guardian_id(&self) -> Option<u32> {
        self.guardian.as_ref().map(Guardian::id)
    }
    pub fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
        self.child.try_wait()
    }
    /// Stop only this created child, wait for its kernel process handle, then conditionally clean
    /// its recorded layout. The native cleanup result is retained; failure is never called restored.
    pub fn stop(&mut self) -> Result<RestoreDisposition, ProbeError> {
        if let Some(result) = self.cleanup {
            return result;
        }
        self.job.take();
        let _ = self.child.kill();
        let until = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let ended = loop {
            match self.child.try_wait() {
                Ok(Some(_)) => break true,
                Err(_) => break false,
                Ok(None) if std::time::Instant::now() >= until => break false,
                Ok(None) => std::thread::sleep(std::time::Duration::from_millis(10)),
            }
        };
        let result = if ended {
            self.guardian
                .as_mut()
                .map(|g| g.finish())
                .unwrap_or(Ok(RestoreDisposition::NoRecord))
        } else {
            Err(ProbeError::CleanupTimeout)
        };
        self.cleanup = Some(result);
        result
    }
}
fn acceptance_error_stream() -> Stdio {
    #[cfg(debug_assertions)]
    if std::env::var_os("TOKENPULSE_ACCEPTANCE_HOST_DIAGNOSTICS").as_deref()
        == Some(std::ffi::OsStr::new("1"))
    {
        return Stdio::inherit();
    }
    Stdio::null()
}
impl Drop for HostProcess {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
