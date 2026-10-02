//! Separate native cleanup supervisor. It is deliberately outside the host's kill-on-close Job.
//! Bootstrap is authenticated by current-user pipe, nonce and kernel peer PID; kernel handles
//! remain open across parent/host exit. No account, logs, database, shell or arbitrary HWND input.
use super::{
    RestoreDisposition, Startup, TransportError, recover_terminated_host, security,
    topology::{ProbeError, birth},
    transport::{self, IO_TIMEOUT},
};
use crate::{Envelope, PROTOCOL_VERSION};
use serde::{Deserialize, Serialize};
use std::{
    os::windows::{
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
        process::CommandExt,
    },
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use token_pulse_core::numeric::DecimalInt;
use tokio::{net::windows::named_pipe::ClientOptions, time::timeout};
use windows_sys::Win32::{
    Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT},
    Storage::FileSystem::SECURITY_IDENTIFICATION,
    System::Threading::{
        CREATE_NO_WINDOW, GetCurrentProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        PROCESS_SYNCHRONIZE, QueryFullProcessImageNameW, TerminateProcess, WaitForSingleObject,
    },
};
pub const CLEANUP_DEADLINE: Duration = Duration::from_secs(5);
const RESULT_PREFIX: u32 = 0x5450_0000; // Distinct from ordinary/forced process exit codes 0/1.

#[derive(Clone)]
pub struct GuardianStartup {
    session: Startup,
    host_pid: u32,
    host_birth: [u32; 2],
}
impl GuardianStartup {
    fn channel(&self) -> String {
        format!(
            r"\\.\pipe\TokenPulse.Taskbar.Guard.{}",
            self.session.instance
        )
    }
    fn arguments(&self) -> Vec<String> {
        let mut args = vec!["--cleanup-guardian".to_owned()];
        args.extend(self.session.arguments());
        args.extend([
            "--host-pid".to_owned(),
            self.host_pid.to_string(),
            "--host-birth-low".to_owned(),
            self.host_birth[0].to_string(),
            "--host-birth-high".to_owned(),
            self.host_birth[1].to_string(),
        ]);
        args
    }
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, TransportError> {
        let args: Vec<_> = args.into_iter().take(14).collect();
        if args.len() != 13
            || args[0] != "--cleanup-guardian"
            || args[7] != "--host-pid"
            || args[9] != "--host-birth-low"
            || args[11] != "--host-birth-high"
        {
            return Err(TransportError::InvalidStartup);
        }
        let startup = Self {
            session: Startup::parse(args[1..7].iter().cloned())?,
            host_pid: args[8]
                .parse()
                .map_err(|_| TransportError::InvalidStartup)?,
            host_birth: [
                args[10]
                    .parse()
                    .map_err(|_| TransportError::InvalidStartup)?,
                args[12]
                    .parse()
                    .map_err(|_| TransportError::InvalidStartup)?,
            ],
        };
        if startup.host_pid == 0 || startup.host_pid == startup.session.parent_pid {
            return Err(TransportError::InvalidStartup);
        }
        Ok(startup)
    }
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Armed {
    Armed { host_pid: u32, host_birth: [u32; 2] },
}

pub struct Guardian {
    child: Child,
    armed: bool,
    outcome: Option<Result<RestoreDisposition, ProbeError>>,
}
impl Guardian {
    /// The executable must be the same native program as the caller's already-created host.
    /// Production supplies the validated bundled host path; development examples use themselves.
    pub async fn launch(
        executable: &Path,
        startup: &Startup,
        host: &Child,
    ) -> Result<Self, TransportError> {
        startup.validate()?;
        if !executable.is_absolute() || executable.extension().is_none_or(|e| e != "exe") {
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
        token_pulse_core::sources::validate_root(
            &executable,
            token_pulse_core::sources::SourceOrigin::Custom,
        )
        .map_err(|_| TransportError::InvalidStartup)?;
        let mut session = startup.clone();
        session.parent_pid = std::process::id(); // Guardian binds its actual creating process.
        let config = GuardianStartup {
            session,
            host_pid: host.id(),
            host_birth: birth(host.as_raw_handle().cast()).map_err(|_| TransportError::Native)?,
        };
        let server = security::create_server(&config.channel())?;
        let child = Command::new(&executable)
            .args(config.arguments())
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| TransportError::Spawn)?;
        let mut guardian = Self {
            child,
            armed: false,
            outcome: None,
        };
        timeout(IO_TIMEOUT, server.connect())
            .await
            .map_err(|_| TransportError::Timeout)??;
        transport::verify_peer(&server, guardian.child.id(), true)?;
        let mut server = server;
        let reply: Envelope<Armed> = timeout(IO_TIMEOUT, transport::read(&mut server))
            .await
            .map_err(|_| TransportError::Timeout)??
            .ok_or(TransportError::PeerMismatch)?;
        if reply.protocol_version != PROTOCOL_VERSION
            || reply.host_instance_id != config.session.instance
            || reply.nonce != config.session.nonce
            || reply.sequence.as_str() != "1"
            || !matches!(reply.body, Armed::Armed {host_pid, host_birth} if host_pid == config.host_pid && host_birth == config.host_birth)
        {
            return Err(TransportError::PeerMismatch);
        }
        guardian.armed = true;
        // The process handles, not this bootstrap pipe, supervise lifetime from here.
        Ok(guardian)
    }
    pub fn id(&self) -> u32 {
        self.child.id()
    }
    /// Called after closing the host Job. Does not perform any Explorer operation in the parent.
    pub fn finish(&mut self) -> Result<RestoreDisposition, ProbeError> {
        if let Some(outcome) = self.outcome {
            return outcome;
        }
        let until = Instant::now() + CLEANUP_DEADLINE + Duration::from_secs(1);
        let outcome = loop {
            match self.child.try_wait() {
                Ok(Some(status)) => {
                    break status
                        .code()
                        .and_then(|code| decode_outcome(code as u32))
                        .unwrap_or(Err(ProbeError::GuardianUnavailable));
                }
                Err(_) => break Err(ProbeError::Os),
                Ok(None) if Instant::now() >= until => {
                    let _ = self.child.kill();
                    // Do not turn a timed-out or forcibly killed supervisor into NoRecord.
                    break Err(ProbeError::CleanupTimeout);
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            }
        };
        self.outcome = Some(outcome);
        outcome
    }
}
impl Drop for Guardian {
    fn drop(&mut self) {
        if !self.armed {
            let _ = self.child.kill();
            let until = Instant::now() + Duration::from_secs(1);
            while self.child.try_wait().is_ok_and(|s| s.is_none()) && Instant::now() < until {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        // Once armed it must survive the creating parent. std Child drop closes only our handle.
    }
}

fn open_process(pid: u32) -> Result<OwnedHandle, TransportError> {
    let handle = unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
            0,
            pid,
        )
    };
    if handle.is_null() {
        return Err(TransportError::PeerMismatch);
    }
    Ok(unsafe { OwnedHandle::from_raw_handle(handle.cast()) })
}
fn same_image(process: &OwnedHandle) -> Result<(), TransportError> {
    let mut buffer = vec![0; 32768];
    let mut length = buffer.len() as u32;
    if unsafe {
        QueryFullProcessImageNameW(
            process.as_raw_handle().cast(),
            0,
            buffer.as_mut_ptr(),
            &mut length,
        )
    } == 0
    {
        return Err(TransportError::PeerMismatch);
    }
    let image =
        String::from_utf16(&buffer[..length as usize]).map_err(|_| TransportError::PeerMismatch)?;
    let actual = Path::new(&image)
        .canonicalize()
        .map_err(|_| TransportError::PeerMismatch)?;
    let expected = std::env::current_exe()
        .and_then(|p| p.canonicalize())
        .map_err(|_| TransportError::PeerMismatch)?;
    if !actual
        .to_string_lossy()
        .eq_ignore_ascii_case(&expected.to_string_lossy())
    {
        return Err(TransportError::PeerMismatch);
    }
    Ok(())
}
pub async fn run_guardian(config: GuardianStartup) -> Result<(), TransportError> {
    let mut pipe = ClientOptions::new()
        .security_qos_flags(SECURITY_IDENTIFICATION)
        .open(config.channel())?;
    transport::verify_peer(&pipe, config.session.parent_pid, false)?;
    let parent = open_process(config.session.parent_pid)?;
    let host = open_process(config.host_pid)?;
    if birth(host.as_raw_handle().cast()).map_err(|_| TransportError::PeerMismatch)?
        != config.host_birth
    {
        return Err(TransportError::PeerMismatch);
    }
    same_image(&host)?;
    if unsafe { WaitForSingleObject(parent.as_raw_handle().cast(), 0) } != WAIT_TIMEOUT
        || unsafe { WaitForSingleObject(host.as_raw_handle().cast(), 0) } != WAIT_TIMEOUT
    {
        return Err(TransportError::PeerMismatch);
    }
    let reply = Envelope {
        protocol_version: PROTOCOL_VERSION,
        host_instance_id: config.session.instance.clone(),
        nonce: config.session.nonce.clone(),
        sequence: DecimalInt::parse("1").map_err(|_| TransportError::InvalidStartup)?,
        body: Armed::Armed {
            host_pid: config.host_pid,
            host_birth: config.host_birth,
        },
    };
    transport::write(&mut pipe, &reply).await?;
    drop(pipe);
    let mut parent_ended_at = None;
    loop {
        match unsafe { WaitForSingleObject(host.as_raw_handle().cast(), 0) } {
            WAIT_OBJECT_0 => break,
            WAIT_TIMEOUT => {}
            _ => return Err(TransportError::PeerMismatch),
        }
        if unsafe { WaitForSingleObject(parent.as_raw_handle().cast(), 0) } == WAIT_OBJECT_0 {
            let ended = parent_ended_at.get_or_insert_with(Instant::now);
            if ended.elapsed() >= CLEANUP_DEADLINE {
                std::process::exit(encode_outcome(Err(ProbeError::CleanupTimeout)) as i32);
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    complete_cleanup(|| recover_terminated_host(&config.session.instance, &host))
}

/// Used only in this disposable cleanup process, including explicit development deadline checks.
/// A hung cross-process native call cannot keep the supervisor or caller alive indefinitely.
pub fn complete_cleanup(operation: impl FnOnce() -> Result<RestoreDisposition, ProbeError>) -> ! {
    let (cancel, cancelled) = std::sync::mpsc::channel();
    let timer = std::thread::Builder::new()
        .name("taskbar-cleanup-deadline".into())
        .spawn(move || match cancelled.recv_timeout(CLEANUP_DEADLINE) {
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                terminate_self(Err(ProbeError::CleanupTimeout))
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                terminate_self(Err(ProbeError::GuardianUnavailable))
            }
            Ok(()) => {}
        });
    if timer.is_err() {
        std::process::exit(encode_outcome(Err(ProbeError::GuardianUnavailable)) as i32);
    }
    let outcome = operation();
    let _ = cancel.send(());
    std::process::exit(encode_outcome(outcome) as i32);
}
fn terminate_self(outcome: Result<RestoreDisposition, ProbeError>) -> ! {
    // The disposable process may have a thread stuck in a native DLL. Avoid DLL-detach callbacks
    // at the deadline. This function accepts no PID and can only terminate its own process.
    unsafe {
        TerminateProcess(GetCurrentProcess(), encode_outcome(outcome));
    }
    std::process::abort(); // Only reached if terminating our own process failed.
}
fn encode_outcome(outcome: Result<RestoreDisposition, ProbeError>) -> u32 {
    RESULT_PREFIX
        | match outcome {
            Ok(RestoreDisposition::NoRecord) => 1,
            Ok(RestoreDisposition::Restored) => 2,
            Ok(RestoreDisposition::AlreadyRestored) => 3,
            Ok(RestoreDisposition::ExternalChange) => 4,
            Ok(RestoreDisposition::IdentityLost) => 5,
            Ok(RestoreDisposition::Failed) => 6,
            Ok(RestoreDisposition::Uncertain) => 7,
            Err(ProbeError::Os) => 0x101,
            Err(ProbeError::UnsupportedVersion) => 0x102,
            Err(ProbeError::MissingTaskbar) => 0x103,
            Err(ProbeError::UnexpectedStructure) => 0x104,
            Err(ProbeError::UnsafeGeometry) => 0x105,
            Err(ProbeError::InsufficientSpace) => 0x106,
            Err(ProbeError::BackgroundUnavailable) => 0x107,
            Err(ProbeError::CleanupTimeout) => 0x108,
            Err(ProbeError::GuardianUnavailable) => 0x109,
        }
}
pub fn decode_outcome(code: u32) -> Option<Result<RestoreDisposition, ProbeError>> {
    Some(match code {
        c if c == RESULT_PREFIX | 1 => Ok(RestoreDisposition::NoRecord),
        c if c == RESULT_PREFIX | 2 => Ok(RestoreDisposition::Restored),
        c if c == RESULT_PREFIX | 3 => Ok(RestoreDisposition::AlreadyRestored),
        c if c == RESULT_PREFIX | 4 => Ok(RestoreDisposition::ExternalChange),
        c if c == RESULT_PREFIX | 5 => Ok(RestoreDisposition::IdentityLost),
        c if c == RESULT_PREFIX | 6 => Ok(RestoreDisposition::Failed),
        c if c == RESULT_PREFIX | 7 => Ok(RestoreDisposition::Uncertain),
        c if c == RESULT_PREFIX | 0x101 => Err(ProbeError::Os),
        c if c == RESULT_PREFIX | 0x102 => Err(ProbeError::UnsupportedVersion),
        c if c == RESULT_PREFIX | 0x103 => Err(ProbeError::MissingTaskbar),
        c if c == RESULT_PREFIX | 0x104 => Err(ProbeError::UnexpectedStructure),
        c if c == RESULT_PREFIX | 0x105 => Err(ProbeError::UnsafeGeometry),
        c if c == RESULT_PREFIX | 0x106 => Err(ProbeError::InsufficientSpace),
        c if c == RESULT_PREFIX | 0x107 => Err(ProbeError::BackgroundUnavailable),
        c if c == RESULT_PREFIX | 0x108 => Err(ProbeError::CleanupTimeout),
        c if c == RESULT_PREFIX | 0x109 => Err(ProbeError::GuardianUnavailable),
        _ => return None,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn supervisor_exit_codes_and_startup_are_bounded_and_never_confuse_forced_zero_exit() {
        assert_eq!(decode_outcome(0), None);
        assert_eq!(decode_outcome(1), None);
        assert_eq!(
            decode_outcome(0x54500002),
            Some(Ok(RestoreDisposition::Restored))
        );
        assert_eq!(
            decode_outcome(0x54500108),
            Some(Err(ProbeError::CleanupTimeout))
        );
        assert_eq!(encode_outcome(Ok(RestoreDisposition::NoRecord)), 0x54500001);
        let config = GuardianStartup {
            session: Startup::new(),
            host_pid: std::process::id() + 1,
            host_birth: [0, u32::MAX],
        };
        let parsed = GuardianStartup::parse(config.arguments()).unwrap();
        assert_eq!(parsed.host_birth, [0, u32::MAX]);
        let mut extra = config.arguments();
        extra.push("--anything".into());
        assert!(GuardianStartup::parse(extra).is_err());
        let mut zero = config.arguments();
        zero[8] = "0".into();
        assert!(GuardianStartup::parse(zero).is_err());
    }
}
