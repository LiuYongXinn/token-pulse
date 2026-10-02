use crate::{
    REQUEST_TIMEOUT,
    framing::read_frame,
    protocol::{self, AccountRequest, ProtocolEvent, RequestKind, RpcToken},
};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    io::{BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use token_pulse_core::error::ErrorCode;

/// Backend-only configuration, constructed after native selection / user consent.
/// Validation alone is not authorization to launch an account connection.
pub struct NativeService {
    executable: PathBuf,
    home: Option<PathBuf>,
    fingerprint: Option<String>,
}
impl NativeService {
    pub fn new(executable: &Path, home: Option<&Path>) -> Result<Self, ErrorCode> {
        if !executable.is_absolute() || home.is_some_and(|p| !p.is_absolute()) {
            return Err(ErrorCode::InvalidQuery);
        }
        #[cfg(windows)]
        for path in std::iter::once(executable).chain(home) {
            token_pulse_core::sources::validate_root(
                path,
                token_pulse_core::sources::SourceOrigin::Custom,
            )?;
        }
        #[cfg(windows)]
        if !executable
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("exe"))
        {
            return Err(ErrorCode::InvalidQuery);
        }
        let executable = executable
            .canonicalize()
            .map_err(|_| ErrorCode::QuotaServiceUnavailable)?;
        #[cfg(windows)]
        token_pulse_core::sources::validate_root(
            &executable,
            token_pulse_core::sources::SourceOrigin::Custom,
        )?;
        #[cfg(windows)]
        if !executable
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("exe"))
        {
            // Also check the resolved target: an .exe link must not select a .cmd shell.
            return Err(ErrorCode::InvalidQuery);
        }
        if !executable.is_file() {
            return Err(ErrorCode::InvalidQuery);
        }
        let home = home
            .map(|p| {
                p.canonicalize()
                    .map_err(|_| ErrorCode::QuotaServiceUnavailable)
            })
            .transpose()?;
        #[cfg(windows)]
        if let Some(home) = &home {
            token_pulse_core::sources::validate_root(
                home,
                token_pulse_core::sources::SourceOrigin::Custom,
            )?;
        }
        if home.as_ref().is_some_and(|p| !p.is_dir()) {
            return Err(ErrorCode::InvalidQuery);
        }
        Ok(Self {
            executable,
            home,
            fingerprint: None,
        })
    }
    pub fn inspect(
        executable: &Path,
        home: Option<&Path>,
    ) -> Result<token_pulse_core::quota::AccountServiceTarget, ErrorCode> {
        let spec = Self::new(executable, home)?;
        let (fingerprint, _) = hash_program(&spec.executable)?;
        let target = token_pulse_core::quota::AccountServiceTarget {
            executable_path: spec
                .executable
                .to_str()
                .ok_or(ErrorCode::InvalidQuery)?
                .into(),
            home_path: spec
                .home
                .as_ref()
                .map(|p| p.to_str().map(str::to_owned).ok_or(ErrorCode::InvalidQuery))
                .transpose()?,
            executable_sha256: fingerprint,
        };
        target.validate()?;
        Ok(target)
    }
    pub fn from_target(
        target: &token_pulse_core::quota::AccountServiceTarget,
    ) -> Result<Self, ErrorCode> {
        target.validate()?;
        let mut spec = Self::new(
            Path::new(&target.executable_path),
            target.home_path.as_deref().map(Path::new),
        )?;
        if hash_program(&spec.executable)?.0 != target.executable_sha256 {
            return Err(ErrorCode::StaleConfirmation);
        }
        spec.fingerprint = Some(target.executable_sha256.clone());
        Ok(spec)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Initializing,
    Ready,
    Closed,
}
struct Pending {
    kind: RequestKind,
    deadline: Instant,
}
struct WriteFrame {
    bytes: Vec<u8>,
    acknowledged: SyncSender<Result<(), ErrorCode>>,
}
enum Incoming {
    Frame(Vec<u8>),
    Failed(ErrorCode),
}

/// One serialized owner must process events; never put this behind the UI thread.
/// Pipes, queues and request table are bounded. Debug never exposes raw messages.
pub struct StdioSession {
    epoch: String,
    sequence: u64,
    phase: Phase,
    pending: BTreeMap<String, Pending>,
    child: Option<Child>,
    #[cfg(windows)]
    job: Option<OwnedJob>,
    input: Option<SyncSender<WriteFrame>>,
    output: Option<Receiver<Incoming>>,
    workers: Vec<JoinHandle<()>>,
    discarded_stderr_bytes: Arc<AtomicU64>,
}
impl StdioSession {
    /// Starts only the selected native binary with literal `app-server`.
    /// No shell, CLI turns, automatic login, credential access or desktop connection.
    pub fn launch(spec: &NativeService, epoch: &str) -> Result<Self, ErrorCode> {
        if epoch.is_empty()
            || epoch.len() > 96
            || !epoch
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
        {
            return Err(ErrorCode::InvalidQuery);
        }
        let mut command = Command::new(&spec.executable);
        // Keep a deny-write/delete executable handle through spawn on Windows. The
        // saved fingerprint is checked again at the actual launch, after queueing.
        let _program_guard = if let Some(expected) = &spec.fingerprint {
            let (actual, guard) = hash_program(&spec.executable)?;
            if &actual != expected {
                return Err(ErrorCode::StaleConfirmation);
            }
            Some(guard)
        } else {
            None
        };
        command
            .arg("app-server")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // Avoid using the project as service cwd or parsing project-local instructions.
        if let Some(home) = &spec.home {
            command.env("CODEX_HOME", home).current_dir(home);
        } else {
            command.current_dir(spec.executable.parent().ok_or(ErrorCode::InvalidQuery)?);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            use windows_sys::Win32::System::Threading::{CREATE_NO_WINDOW, CREATE_SUSPENDED};
            command.creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED);
        }
        let child = command
            .spawn()
            .map_err(|_| ErrorCode::QuotaServiceUnavailable)?;
        let mut session = Self {
            epoch: epoch.into(),
            sequence: 0,
            phase: Phase::Initializing,
            pending: BTreeMap::new(),
            child: Some(child),
            #[cfg(windows)]
            job: None,
            input: None,
            output: None,
            workers: Vec::new(),
            discarded_stderr_bytes: Arc::new(AtomicU64::new(0)),
        };
        #[cfg(windows)]
        {
            session.job = Some(OwnedJob::assign(
                session.child.as_ref().expect("owned child"),
            )?);
            resume_owned_primary(session.child.as_ref().expect("owned child"))?;
        }
        let child = session.child.as_mut().expect("owned child");
        let mut stdin = child.stdin.take().expect("configured piped stdin");
        let stdout = child.stdout.take().expect("configured piped stdout");
        let mut stderr = child.stderr.take().expect("configured piped stderr");
        let (input, writes) = mpsc::sync_channel::<WriteFrame>(4);
        session.input = Some(input);
        session.workers.push(
            thread::Builder::new()
                .name("quota-stdin".into())
                .spawn(move || {
                    while let Ok(frame) = writes.recv() {
                        let result = stdin
                            .write_all(&frame.bytes)
                            .and_then(|_| stdin.flush())
                            .map_err(|_| ErrorCode::QuotaServiceUnavailable);
                        let failed = result.is_err();
                        let _ = frame.acknowledged.send(result);
                        if failed {
                            break;
                        }
                    }
                })
                .map_err(|_| ErrorCode::QuotaServiceUnavailable)?,
        );
        let (frames, output) = mpsc::sync_channel(16);
        session.output = Some(output);
        session.workers.push(
            thread::Builder::new()
                .name("quota-stdout".into())
                .spawn(move || {
                    let mut reader = BufReader::new(stdout);
                    loop {
                        match read_frame(&mut reader) {
                            Ok(Some(bytes)) => {
                                if frames.send(Incoming::Frame(bytes)).is_err() {
                                    break;
                                }
                            }
                            Ok(None) => {
                                let _ = frames
                                    .send(Incoming::Failed(ErrorCode::QuotaServiceUnavailable));
                                break;
                            }
                            Err(code) => {
                                let _ = frames.send(Incoming::Failed(code));
                                break;
                            }
                        }
                    }
                })
                .map_err(|_| ErrorCode::QuotaServiceUnavailable)?,
        );
        let discarded = session.discarded_stderr_bytes.clone();
        session.workers.push(
            thread::Builder::new()
                .name("quota-stderr".into())
                .spawn(move || {
                    let mut buffer = [0; 8192];
                    loop {
                        match stderr.read(&mut buffer) {
                            Ok(0) | Err(_) => break,
                            Ok(bytes) => {
                                let _ = discarded.fetch_update(
                                    Ordering::Relaxed,
                                    Ordering::Relaxed,
                                    |old| Some(old.saturating_add(bytes as u64)),
                                );
                            }
                        }
                    }
                    // Buffer and raw stderr are never sent to a logger or DTO.
                })
                .map_err(|_| ErrorCode::QuotaServiceUnavailable)?,
        );
        session.send_kind(RequestKind::Initialize)?;
        Ok(session)
    }
    pub fn is_ready(&self) -> bool {
        self.phase == Phase::Ready
    }
    pub fn process_id(&self) -> Option<u32> {
        self.child.as_ref().map(Child::id)
    }
    /// This safe count is the only stderr information available to diagnostics.
    pub fn discarded_stderr_bytes(&self) -> u64 {
        self.discarded_stderr_bytes.load(Ordering::Relaxed)
    }
    fn write(&mut self, value: Value) -> Result<(), ErrorCode> {
        let mut bytes = serde_json::to_vec(&value).map_err(|_| ErrorCode::QuotaProtocolError)?;
        bytes.push(b'\n');
        let (acknowledged, result) = mpsc::sync_channel(1);
        let write = self
            .input
            .as_ref()
            .ok_or(ErrorCode::QuotaDisconnected)?
            .try_send(WriteFrame {
                bytes,
                acknowledged,
            });
        if write.is_err() {
            self.close();
            return Err(ErrorCode::QuotaServiceUnavailable);
        }
        match result.recv_timeout(REQUEST_TIMEOUT) {
            Ok(Ok(())) => Ok(()),
            Ok(Err(code)) => {
                self.close();
                Err(code)
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                self.close();
                Err(ErrorCode::QuotaTimeout)
            }
            Err(_) => {
                self.close();
                Err(ErrorCode::QuotaServiceUnavailable)
            }
        }
    }
    fn send_kind(&mut self, kind: RequestKind) -> Result<RpcToken, ErrorCode> {
        if self.pending.len() >= 4 {
            return Err(ErrorCode::QuotaServiceUnavailable);
        }
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or(ErrorCode::NumericOverflow)?;
        let token = RpcToken {
            connection_epoch: self.epoch.clone(),
            request_id: format!("{}:{}", self.epoch, self.sequence),
        };
        // Include the pipe write in the request's deadline.
        let deadline = Instant::now() + REQUEST_TIMEOUT;
        self.write(protocol::request(kind, &token.request_id))?;
        self.pending
            .insert(token.request_id.clone(), Pending { kind, deadline });
        Ok(token)
    }
    pub fn send(&mut self, request: AccountRequest) -> Result<RpcToken, ErrorCode> {
        if !self.is_ready() {
            return Err(ErrorCode::QuotaDisconnected);
        }
        if self
            .pending
            .values()
            .any(|p| p.kind == RequestKind::Account(request))
        {
            return Err(ErrorCode::QuotaServiceUnavailable);
        }
        self.send_kind(RequestKind::Account(request))
    }
    fn expired(&mut self) -> Option<ProtocolEvent> {
        let now = Instant::now();
        let id = self
            .pending
            .iter()
            .find(|(_, p)| p.deadline <= now)
            .map(|(id, _)| id.clone())?;
        let pending = self.pending.remove(&id).expect("pending identified");
        let event = ProtocolEvent::TimedOut {
            token: RpcToken {
                connection_epoch: self.epoch.clone(),
                request_id: id,
            },
        };
        if pending.kind == RequestKind::Initialize {
            self.close();
        }
        Some(event)
    }
    /// Returns typed account events. Unknown notifications and stale replies are dropped.
    /// Call continuously in an owner worker, including when all UI entries are hidden.
    pub fn next_event(&mut self, timeout: Duration) -> Result<Option<ProtocolEvent>, ErrorCode> {
        let result = self.read_event(timeout.min(REQUEST_TIMEOUT));
        if result.is_err() {
            self.close();
        }
        result
    }
    fn read_event(&mut self, timeout: Duration) -> Result<Option<ProtocolEvent>, ErrorCode> {
        let until = Instant::now() + timeout;
        loop {
            if let Some(event) = self.expired() {
                return Ok(Some(event));
            }
            let remaining = until.saturating_duration_since(Instant::now());
            let wait = self
                .pending
                .values()
                .map(|p| p.deadline.saturating_duration_since(Instant::now()))
                .min()
                .unwrap_or(remaining)
                .min(remaining);
            let received = self
                .output
                .as_ref()
                .ok_or(ErrorCode::QuotaDisconnected)?
                .recv_timeout(wait);
            match received {
                Ok(Incoming::Failed(code)) => return Err(code),
                Ok(Incoming::Frame(bytes)) => {
                    let value: Value = serde_json::from_slice(&bytes)
                        .map_err(|_| ErrorCode::QuotaProtocolError)?;
                    if let Some(event) = self.accept(value)? {
                        return Ok(Some(event));
                    }
                    if Instant::now() >= until {
                        return Ok(None);
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => return Ok(self.expired()),
                Err(_) => return Err(ErrorCode::QuotaServiceUnavailable),
            }
        }
    }
    fn accept(&mut self, value: Value) -> Result<Option<ProtocolEvent>, ErrorCode> {
        let object = value.as_object().ok_or(ErrorCode::QuotaProtocolError)?;
        if object.get("jsonrpc").is_some_and(|v| v != "2.0") {
            return Err(ErrorCode::QuotaProtocolError);
        }
        if let Some(method) = object.get("method") {
            let method = method.as_str().ok_or(ErrorCode::QuotaProtocolError)?;
            if method.is_empty()
                || method.len() > 256
                || method.chars().any(char::is_control)
                || object.contains_key("result")
                || object.contains_key("error")
            {
                return Err(ErrorCode::QuotaProtocolError);
            }
            if let Some(id) = object.get("id") {
                self.write(protocol::reject_server_request(id)?)?;
                return Ok(None);
            }
            return if self.is_ready() {
                let event = protocol::notification(method, object.get("params"))?;
                if matches!(event, Some(ProtocolEvent::AccountChanged)) {
                    // Identity changed: replies from all previous account reads are obsolete.
                    self.pending
                        .retain(|_, pending| pending.kind == RequestKind::Initialize);
                }
                Ok(event)
            } else {
                Ok(None)
            };
        }
        let id = object.get("id").ok_or(ErrorCode::QuotaProtocolError)?;
        if !id.is_string() && !id.is_i64() && !id.is_u64() {
            return Err(ErrorCode::QuotaProtocolError);
        }
        let Some(id) = id.as_str() else {
            return Ok(None);
        };
        let Some(pending) = self.pending.remove(id) else {
            return Ok(None);
        };
        if pending.deadline <= Instant::now() {
            if pending.kind == RequestKind::Initialize {
                self.close();
            }
            return Ok(Some(ProtocolEvent::TimedOut {
                token: RpcToken {
                    connection_epoch: self.epoch.clone(),
                    request_id: id.into(),
                },
            }));
        }
        let result = protocol::reply(pending.kind, &value);
        if pending.kind == RequestKind::Initialize {
            if result.is_ok() {
                self.write(protocol::initialized())?;
                self.phase = Phase::Ready;
            } else {
                self.close();
            }
        }
        Ok(Some(ProtocolEvent::Reply {
            token: RpcToken {
                connection_epoch: self.epoch.clone(),
                request_id: id.into(),
            },
            result,
        }))
    }
    pub fn close(&mut self) {
        self.phase = Phase::Closed;
        self.pending.clear();
        // Unblock bounded channel writers before waiting for pipe workers.
        self.output.take();
        self.input.take();
        #[cfg(windows)]
        self.job.take(); // Kill all processes still in the owned job, including descendants.
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}
impl Drop for StdioSession {
    fn drop(&mut self) {
        self.close();
    }
}

fn hash_program(path: &Path) -> Result<(String, std::fs::File), ErrorCode> {
    use sha2::{Digest, Sha256};
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ);
    }
    let mut file = options
        .open(path)
        .map_err(|_| ErrorCode::QuotaServiceUnavailable)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65_536];
    loop {
        let bytes = file
            .read(&mut buffer)
            .map_err(|_| ErrorCode::QuotaServiceUnavailable)?;
        if bytes == 0 {
            break;
        }
        hash.update(&buffer[..bytes]);
    }
    Ok((format!("{:x}", hash.finalize()), file))
}

#[cfg(windows)]
fn resume_owned_primary(child: &Child) -> Result<(), ErrorCode> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, INVALID_HANDLE_VALUE},
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First,
                Thread32Next,
            },
            Threading::{OpenThread, ResumeThread, THREAD_SUSPEND_RESUME},
        },
    };
    unsafe {
        // std exposes the primary handle only on nightly. Enumerate only this owned,
        // still-suspended process; never suspend or resume a foreign process/thread.
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return Err(ErrorCode::QuotaServiceUnavailable);
        }
        let mut entry: THREADENTRY32 = std::mem::zeroed();
        entry.dwSize = std::mem::size_of_val(&entry) as u32;
        let mut primary = None;
        let mut multiple = false;
        let mut present = Thread32First(snapshot, &mut entry);
        while present != 0 {
            if entry.th32OwnerProcessID == child.id() {
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
            return Err(ErrorCode::QuotaServiceUnavailable);
        }
        let thread_id = primary.ok_or(ErrorCode::QuotaServiceUnavailable)?;
        let thread = OpenThread(THREAD_SUSPEND_RESUME, 0, thread_id);
        if thread.is_null() {
            return Err(ErrorCode::QuotaServiceUnavailable);
        }
        let previous = ResumeThread(thread);
        CloseHandle(thread);
        // Exactly the one CREATE_SUSPENDED hold must have been released.
        if previous != 1 {
            return Err(ErrorCode::QuotaServiceUnavailable);
        }
        Ok(())
    }
}

#[cfg(windows)]
struct OwnedJob(windows_sys::Win32::Foundation::HANDLE);
#[cfg(windows)]
// Unique ownership: its handle is never exposed or used by a second thread.
unsafe impl Send for OwnedJob {}
#[cfg(windows)]
impl OwnedJob {
    fn assign(child: &Child) -> Result<Self, ErrorCode> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::{
            Foundation::CloseHandle,
            System::JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
                SetInformationJobObject,
            },
        };
        unsafe {
            let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if handle.is_null() {
                return Err(ErrorCode::QuotaServiceUnavailable);
            }
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of_val(&info) as u32,
            ) == 0
                || AssignProcessToJobObject(handle, child.as_raw_handle().cast()) == 0
            {
                CloseHandle(handle);
                return Err(ErrorCode::QuotaServiceUnavailable);
            }
            Ok(Self(handle))
        }
    }
}
#[cfg(windows)]
impl Drop for OwnedJob {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

#[cfg(test)]
mod fingerprint_tests {
    use super::*;
    #[test]
    fn hash_has_an_independent_known_sha256_and_windows_guard_blocks_replacement_writes() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("synthetic-bytes");
        std::fs::write(&file, b"abc").unwrap();
        let (actual, guard) = hash_program(&file).unwrap();
        assert_eq!(
            actual,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        #[cfg(windows)]
        {
            assert!(std::fs::OpenOptions::new().write(true).open(&file).is_err());
            assert!(std::fs::remove_file(&file).is_err());
        }
        drop(guard);
        std::fs::write(&file, b"changed bytes").unwrap();
        assert_ne!(hash_program(&file).unwrap().0, actual);
    }
}
