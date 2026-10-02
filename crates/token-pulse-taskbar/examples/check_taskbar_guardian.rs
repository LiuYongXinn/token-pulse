//! Explicit development-only parent termination and isolated recovery deadline checks.
#[cfg(windows)]
#[tokio::main(flavor = "current_thread")]
async fn main() {
    use std::{
        io::{Read, Write},
        os::windows::process::CommandExt,
        process::{Command, Stdio},
    };
    use token_pulse_taskbar::{
        TaskbarView,
        windows::{
            RestoreDisposition, Startup,
            control::NativeController,
            guardian::{Guardian, GuardianStartup, complete_cleanup, decode_outcome, run_guardian},
            topology::{ProbeError, inspect_primary_taskbar},
        },
    };
    use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;
    let args: Vec<_> = std::env::args().skip(1).take(14).collect();
    if args.first().is_some_and(|a| a == "--cleanup-guardian") {
        let config = GuardianStartup::parse(args).unwrap();
        if run_guardian(config).await.is_err() {
            std::process::exit(1);
        }
        return;
    }
    if args == ["--blocked-cleanup-development"] {
        complete_cleanup(|| {
            std::thread::sleep(std::time::Duration::from_secs(60));
            Ok(RestoreDisposition::NoRecord)
        });
    }
    if args.len() == 2 && args[0] == "--hold-owned-development-layout" {
        let mut start = [0; 5];
        std::io::stdin().read_exact(&mut start).unwrap();
        assert_eq!(&start, b"START");
        let native = NativeController::start_for_instance(&args[1]).unwrap();
        let fixture: TaskbarView =
            serde_json::from_str(include_str!("../../../fixtures/taskbar-display.json")).unwrap();
        native.replace(Some(fixture)).await.unwrap();
        let shown = native.enable_taskbar(true).await.unwrap().embedded;
        println!("{}", if shown { "READY" } else { "FAILED" });
        std::io::stdout().flush().unwrap();
        let mut eof = [0];
        let _ = std::io::stdin().read(&mut eof);
        drop(native);
        return;
    }
    if args.len() == 2 && args[0] == "--owned-parent-development" {
        let mut owned = OwnedLayout::spawn(&args[1]);
        let mut startup = Startup::new();
        startup.instance = args[1].clone();
        let guardian = Guardian::launch(&std::env::current_exe().unwrap(), &startup, &owned.child)
            .await
            .unwrap();
        // Host stays in its stdin gate until the guardian has kernel handles and its ACK is bound.
        owned
            .child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(b"START")
            .unwrap();
        owned.child.stdin.as_mut().unwrap().flush().unwrap();
        assert_eq!(line(owned.child.stdout.take().unwrap()).await, "READY");
        println!("GUARD {}", guardian.id());
        std::io::stdout().flush().unwrap();
        let mut eof = [0];
        let _ = std::io::stdin().read(&mut eof);
        // Normal path also closes host before waiting for guardian. Forced exit skips this.
        owned.stop();
        let _ = guardian;
        return;
    }
    if args != ["--native-taskbar-guardian-development-check"] {
        std::process::exit(2);
    }
    println!("DEVELOPMENT ONLY: synthetic fixture; terminate only the parent process we create");
    let before = inspect_primary_taskbar().expect("supported taskbar baseline");
    let instance = uuid::Uuid::new_v4().simple().to_string();
    let mut parent = Command::new(std::env::current_exe().unwrap())
        .args(["--owned-parent-development", &instance])
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let report = line(parent.stdout.take().unwrap()).await;
    let guard_pid: u32 = report
        .strip_prefix("GUARD ")
        .expect("guard armed and native child embedded")
        .parse()
        .unwrap();
    let guard = ProcessObserver::open(guard_pid);
    let reserved = inspect_primary_taskbar().is_err();
    parent.kill().unwrap(); // No Rust Drop in creating parent; OS closes that parent's host Job.
    parent.wait().unwrap();
    let code = guard.ended().await;
    let outcome = decode_outcome(code).expect("authenticated guardian outcome code");
    let restored = inspect_primary_taskbar().expect("parent-death restored taskbar") == before;
    println!(
        "parent_terminated=true reserved_before_exit={reserved} guardian_outcome={outcome:?} geometry_restored={restored}"
    );
    assert!(
        reserved
            && restored
            && matches!(
                outcome,
                Ok(RestoreDisposition::Restored | RestoreDisposition::AlreadyRestored)
            )
    );
    let mut blocked = Command::new(std::env::current_exe().unwrap())
        .arg("--blocked-cleanup-development")
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let observer = ProcessObserver::open(blocked.id());
    let deadline = decode_outcome(observer.ended().await).expect("deadline outcome code");
    blocked.wait().unwrap();
    println!("simulated_blocked_operation_outcome={deadline:?}");
    assert_eq!(deadline, Err(ProbeError::CleanupTimeout));
}

#[cfg(windows)]
async fn line(stdout: std::process::ChildStdout) -> String {
    use std::io::{BufRead, Read};
    let task = tokio::task::spawn_blocking(move || {
        let mut line = String::new();
        std::io::BufReader::new(stdout.take(64))
            .read_line(&mut line)
            .unwrap();
        line.trim().to_owned()
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap()
}
#[cfg(windows)]
struct OwnedLayout {
    child: std::process::Child,
    job: Option<std::os::windows::io::OwnedHandle>,
}
#[cfg(windows)]
impl OwnedLayout {
    fn spawn(instance: &str) -> Self {
        use std::{
            os::windows::{
                io::{AsRawHandle, FromRawHandle, OwnedHandle},
                process::CommandExt,
            },
            process::{Command, Stdio},
        };
        use windows_sys::Win32::System::{
            JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
                SetInformationJobObject,
            },
            Threading::CREATE_NO_WINDOW,
        };
        let child = Command::new(std::env::current_exe().unwrap())
            .args(["--hold-owned-development-layout", instance])
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut owned = Self { child, job: None };
        let raw = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        assert!(!raw.is_null());
        owned.job = Some(unsafe { OwnedHandle::from_raw_handle(raw.cast()) });
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        assert_ne!(
            unsafe {
                SetInformationJobObject(
                    raw,
                    JobObjectExtendedLimitInformation,
                    (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                    std::mem::size_of_val(&info) as u32,
                )
            },
            0
        );
        assert_ne!(
            unsafe { AssignProcessToJobObject(raw, owned.child.as_raw_handle().cast()) },
            0
        );
        owned
    }
    fn stop(&mut self) {
        self.job.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
#[cfg(windows)]
impl Drop for OwnedLayout {
    fn drop(&mut self) {
        self.stop();
    }
}
#[cfg(windows)]
struct ProcessObserver(std::os::windows::io::OwnedHandle);
#[cfg(windows)]
impl ProcessObserver {
    fn open(pid: u32) -> Self {
        use std::os::windows::io::{FromRawHandle, OwnedHandle};
        use windows_sys::Win32::System::Threading::{
            OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
        };
        let raw = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                0,
                pid,
            )
        };
        assert!(!raw.is_null());
        Self(unsafe { OwnedHandle::from_raw_handle(raw.cast()) })
    }
    async fn ended(&self) -> u32 {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::{
            Foundation::WAIT_OBJECT_0,
            System::Threading::{GetExitCodeProcess, WaitForSingleObject},
        };
        let until = std::time::Instant::now() + std::time::Duration::from_secs(8);
        while unsafe { WaitForSingleObject(self.0.as_raw_handle().cast(), 0) } != WAIT_OBJECT_0 {
            assert!(
                std::time::Instant::now() < until,
                "supervisor functional deadline"
            );
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        let mut code = 0;
        assert_ne!(
            unsafe { GetExitCodeProcess(self.0.as_raw_handle().cast(), &mut code) },
            0
        );
        code
    }
}
#[cfg(not(windows))]
fn main() {
    std::process::exit(2);
}
