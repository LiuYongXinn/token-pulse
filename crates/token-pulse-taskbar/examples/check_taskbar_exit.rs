//! Explicit development-only cross-process Job termination and native layout cleanup check.
#[cfg(windows)]
#[tokio::main(flavor = "current_thread")]
async fn main() {
    use std::io::{Read, Write};
    use std::os::windows::process::CommandExt;
    use token_pulse_taskbar::{
        TaskbarView,
        windows::{
            RestoreDisposition, control::NativeController, recover_terminated_host,
            topology::inspect_primary_taskbar,
        },
    };
    let args: Vec<_> = std::env::args().skip(1).take(4).collect();
    if args.len() == 2 && args[0] == "--hold-owned-development-layout" {
        // Wait for the creating parent to assign its private Job before touching Explorer.
        let mut start = [0; 5];
        std::io::stdin().read_exact(&mut start).unwrap();
        assert_eq!(&start, b"START");
        let native = NativeController::start_for_instance(&args[1]).unwrap();
        let fixture: TaskbarView =
            serde_json::from_str(include_str!("../../../fixtures/taskbar-display.json")).unwrap();
        native.replace(Some(fixture)).await.unwrap();
        let result = native.enable_taskbar(true).await.unwrap();
        println!(
            "{}",
            if result.embedded && result.embedding_failure.is_none() {
                "READY"
            } else {
                "FAILED"
            }
        );
        std::io::stdout().flush().unwrap();
        let mut closed = [0; 1];
        let _ = std::io::stdin().read(&mut closed);
        drop(native);
        return;
    }
    if args != ["--native-taskbar-exit-development-check"] {
        std::process::exit(2);
    }
    println!(
        "DEVELOPMENT ONLY: synthetic fixture; kill only our created child via our Windows Job"
    );
    let before = inspect_primary_taskbar().expect("supported taskbar baseline");
    let mut owned = OwnedChild::spawn();
    let ready = owned.ready().await;
    if ready != "READY" {
        let outcome = owned.stop();
        panic!("development child failed before actual embedding; cleanup={outcome:?}");
    }
    let live_refused = recover_terminated_host(&owned.instance, &owned.child).is_err();
    let attached_before_kill = inspect_primary_taskbar().is_err(); // Full-width baseline is reserved.
    let closed = owned.close_job_and_wait(); // No shutdown/disable; child cannot run Drop.
    let wrong_instance = format!(
        "{}{}",
        if &owned.instance[..1] == "0" {
            "1"
        } else {
            "0"
        },
        &owned.instance[1..]
    );
    let wrong_refused = recover_terminated_host(&wrong_instance, &owned.child).is_err();
    let mut other = std::process::Command::new(std::env::current_exe().unwrap())
        .arg("--exit-without-layout")
        .creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    other.wait().unwrap();
    let wrong_process_refused = recover_terminated_host(&owned.instance, &other).is_err();
    let result = recover_terminated_host(&owned.instance, &owned.child)
        .expect("owned terminated-host recovery");
    owned.finished = true;
    let restored = inspect_primary_taskbar().expect("restored baseline") == before;
    let second =
        recover_terminated_host(&owned.instance, &owned.child).expect("idempotent recovery");
    println!(
        "live_refused={live_refused} reserved_before_kill={attached_before_kill} job_terminated={closed} wrong_instance_refused={wrong_refused} wrong_process_refused={wrong_process_refused} outcome={result:?} geometry_restored={restored} repeated={second:?}"
    );
    assert!(
        live_refused && attached_before_kill && closed && wrong_refused && wrong_process_refused
    );
    assert!(matches!(
        result,
        RestoreDisposition::Restored | RestoreDisposition::AlreadyRestored
    ));
    assert!(restored);
    assert_eq!(second, RestoreDisposition::NoRecord);
    // A fresh native instance must not be blocked by old owner/geometry properties.
    let native = NativeController::start().unwrap();
    let fixture: TaskbarView =
        serde_json::from_str(include_str!("../../../fixtures/taskbar-display.json")).unwrap();
    native.replace(Some(fixture)).await.unwrap();
    let fresh = native.enable_taskbar(true).await.unwrap().embedded;
    drop(native);
    let fresh_restored = inspect_primary_taskbar().unwrap() == before;
    println!("fresh_instance_embedded={fresh} fresh_drop_restored={fresh_restored}");
    assert!(fresh && fresh_restored);
}

#[cfg(windows)]
struct OwnedJob(windows_sys::Win32::Foundation::HANDLE);
#[cfg(windows)]
impl Drop for OwnedJob {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}
#[cfg(windows)]
struct OwnedChild {
    child: std::process::Child,
    job: Option<OwnedJob>,
    instance: String,
    finished: bool,
}
#[cfg(windows)]
impl OwnedChild {
    fn spawn() -> Self {
        use std::{
            io::Write,
            os::windows::{io::AsRawHandle, process::CommandExt},
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
        let instance = uuid::Uuid::new_v4().simple().to_string();
        let child = Command::new(std::env::current_exe().unwrap())
            .args(["--hold-owned-development-layout", &instance])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .unwrap();
        let mut owned = Self {
            child,
            job: None,
            instance,
            finished: false,
        };
        let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        assert!(!handle.is_null());
        owned.job = Some(OwnedJob(handle));
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        assert_ne!(
            unsafe {
                SetInformationJobObject(
                    handle,
                    JobObjectExtendedLimitInformation,
                    (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                    std::mem::size_of_val(&info) as u32,
                )
            },
            0
        );
        assert_ne!(
            unsafe { AssignProcessToJobObject(handle, owned.child.as_raw_handle().cast()) },
            0
        );
        owned
            .child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(b"START")
            .unwrap();
        owned.child.stdin.as_mut().unwrap().flush().unwrap();
        owned
    }
    async fn ready(&mut self) -> String {
        use std::io::{BufRead, Read};
        let stdout = self.child.stdout.take().unwrap();
        let task = tokio::task::spawn_blocking(move || {
            let mut line = String::new();
            std::io::BufReader::new(stdout.take(64))
                .read_line(&mut line)
                .unwrap();
            line.trim().to_owned()
        });
        tokio::time::timeout(std::time::Duration::from_secs(5), task)
            .await
            .ok()
            .and_then(|r| r.ok())
            .unwrap_or_default()
    }
    fn close_job_and_wait(&mut self) -> bool {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::{
            Foundation::WAIT_OBJECT_0, System::Threading::WaitForSingleObject,
        };
        self.job.take();
        // Job termination may report zero. The kernel handle and surviving reservation prove
        // termination without native Drop, independently of its numeric exit code.
        self.child.wait().is_ok()
            && unsafe { WaitForSingleObject(self.child.as_raw_handle().cast(), 0) } == WAIT_OBJECT_0
    }
    fn stop(
        &mut self,
    ) -> Result<
        token_pulse_taskbar::windows::RestoreDisposition,
        token_pulse_taskbar::windows::topology::ProbeError,
    > {
        self.job.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
        let result =
            token_pulse_taskbar::windows::recover_terminated_host(&self.instance, &self.child);
        self.finished = true;
        result
    }
}
#[cfg(windows)]
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self.stop();
        }
    }
}
#[cfg(not(windows))]
fn main() {
    std::process::exit(2);
}
