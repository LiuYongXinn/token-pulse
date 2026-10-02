#![cfg(windows)]
use std::{path::Path, time::Duration};
use token_pulse_core::numeric::DecimalInt;
use token_pulse_taskbar::{
    Envelope, HostMessage, HostReply, WireError,
    windows::{
        HostProcess, Startup, TransportError,
        transport::{self, HostConnection},
    },
};
use tokio::{io::AsyncWriteExt, net::windows::named_pipe::ClientOptions, time::timeout};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, HWND, LPARAM},
    System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
    UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetWindowThreadProcessId, IsWindowVisible,
    },
};

fn executable() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_token-pulse-taskbar-host"))
}
fn frame(startup: &Startup, sequence: &str, body: HostMessage) -> Envelope<HostMessage> {
    Envelope {
        protocol_version: 1,
        host_instance_id: startup.instance.clone(),
        nonce: startup.nonce.clone(),
        sequence: DecimalInt::parse(sequence).unwrap(),
        body,
    }
}
struct ProcessWait(HANDLE);
impl ProcessWait {
    fn open(pid: u32) -> Self {
        let raw = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
        assert!(!raw.is_null());
        Self(raw)
    }
    async fn terminated(&self) {
        // Keep the parent I/O reactor running while cancelled pipe operations complete.
        for _ in 0..100 {
            if unsafe { WaitForSingleObject(self.0, 0) } == 0 {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(unsafe { WaitForSingleObject(self.0, 0) }, 0);
    }
}
impl Drop for ProcessWait {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
fn native_controllers(pid: u32) -> Vec<bool> {
    struct Probe {
        pid: u32,
        visibility: Vec<bool>,
    }
    unsafe extern "system" fn collect(window: HWND, parameter: LPARAM) -> i32 {
        let probe = unsafe { &mut *(parameter as *mut Probe) };
        let mut actual = 0;
        unsafe {
            GetWindowThreadProcessId(window, &mut actual);
        }
        if actual == probe.pid {
            let mut class = [0; 128];
            let count = unsafe { GetClassNameW(window, class.as_mut_ptr(), 128) };
            if count > 0
                && String::from_utf16_lossy(&class[..count as usize])
                    .starts_with("TokenPulse.Taskbar.Control.")
            {
                probe
                    .visibility
                    .push(unsafe { IsWindowVisible(window) != 0 });
            }
        }
        1
    }
    let mut probe = Probe {
        pid,
        visibility: vec![],
    };
    unsafe {
        EnumWindows(Some(collect), (&mut probe as *mut Probe) as LPARAM);
    }
    probe.visibility
}

#[tokio::test(flavor = "current_thread")]
async fn real_child_handshake_privacy_heartbeat_shutdown_and_drop_release_owned_process() {
    let mut connection = HostConnection::launch(executable()).await.unwrap();
    let process = ProcessWait::open(connection.process_id());
    assert_eq!(native_controllers(connection.process_id()), [false]);
    assert!(matches!(
        connection
            .exchange(HostMessage::Privacy {
                settings_revision: DecimalInt::parse("1").unwrap(),
                enabled: false
            })
            .await
            .unwrap(),
        HostReply::PrivacyApplied { enabled: false }
    ));
    let view = serde_json::from_value(serde_json::json!({
        "settings_revision":"1","usage_revision":"9007199254740993","price_revision":"3","generated_at_ms":1000,"privacy":false,"scope_label":"SYNTHETIC PRIVATE SCOPE","timezone":"UTC",
        "total_tokens":null,"input_tokens":null,"cached_tokens":"0","output_tokens":null,"usage_status":"unknown","costs":[],"priced_tokens":"0","unpriced_tokens":"0","quota":null
    })).unwrap();
    assert!(matches!(
        connection
            .exchange(HostMessage::Snapshot {
                view: Box::new(view)
            })
            .await
            .unwrap(),
        HostReply::Heartbeat {}
    ));
    assert!(matches!(
        connection
            .exchange(HostMessage::Heartbeat {})
            .await
            .unwrap(),
        HostReply::Heartbeat {}
    ));
    assert!(matches!(
        connection
            .exchange(HostMessage::Privacy {
                settings_revision: DecimalInt::parse("9007199254740993").unwrap(),
                enabled: true
            })
            .await
            .unwrap(),
        HostReply::PrivacyApplied { enabled: true }
    ));
    connection.shutdown().await.unwrap();
    process.terminated().await;
    assert!(native_controllers(connection.process_id()).is_empty());
    assert_eq!(
        connection.exchange(HostMessage::Heartbeat {}).await.err(),
        Some(TransportError::Protocol(WireError::Closed))
    );
    let connection = HostConnection::launch(executable()).await.unwrap();
    let process = ProcessWait::open(connection.process_id());
    drop(connection);
    process.terminated().await;
    let mut connection = HostConnection::launch(executable()).await.unwrap();
    let process = ProcessWait::open(connection.process_id());
    assert!(connection.exchange(HostMessage::Hello {}).await.is_err());
    // Failure closes the child immediately even while the owner object remains alive.
    process.terminated().await;
    assert_eq!(
        connection.exchange(HostMessage::Heartbeat {}).await.err(),
        Some(TransportError::Protocol(WireError::Closed))
    );
}
#[tokio::test(flavor = "current_thread")]
async fn reserved_pipe_and_kernel_peer_pid_reject_an_unexpected_client_or_server() {
    let startup = Startup::new();
    let pipe = transport::create_server(&startup).unwrap();
    assert!(transport::create_server(&startup).is_err());
    let client = ClientOptions::new().open(startup.channel_name()).unwrap();
    timeout(Duration::from_secs(2), pipe.connect())
        .await
        .unwrap()
        .unwrap();
    transport::verify_peer(&pipe, std::process::id(), true).unwrap();
    transport::verify_peer(&client, std::process::id(), false).unwrap();
    assert_eq!(
        transport::verify_peer(&pipe, 0, true),
        Err(TransportError::PeerMismatch)
    );
    assert_eq!(
        transport::verify_peer(&client, 0, false),
        Err(TransportError::PeerMismatch)
    );
}
#[tokio::test(flavor = "current_thread")]
async fn actual_host_rejects_wrong_parent_nonce_and_partial_frames() {
    for mode in 0..3 {
        let mut startup = Startup::new();
        if mode == 0 {
            startup.parent_pid = std::process::id() + 1;
        }
        let mut pipe = transport::create_server(&startup).unwrap();
        let owned = HostProcess::spawn(executable(), &startup).unwrap();
        let process = ProcessWait::open(owned.id());
        timeout(Duration::from_secs(3), pipe.connect())
            .await
            .unwrap()
            .unwrap();
        transport::verify_peer(&pipe, owned.id(), true).unwrap();
        if mode == 0 {
            assert!(
                timeout(
                    Duration::from_secs(3),
                    transport::read::<_, HostReply>(&mut pipe)
                )
                .await
                .unwrap()
                .unwrap()
                .is_none()
            );
        } else if mode == 1 {
            let mut wrong = frame(&startup, "1", HostMessage::Hello {});
            wrong.nonce = "f".repeat(64);
            transport::write(&mut pipe, &wrong).await.unwrap();
            assert!(
                timeout(
                    Duration::from_secs(3),
                    transport::read::<_, HostReply>(&mut pipe)
                )
                .await
                .unwrap()
                .unwrap()
                .is_none()
            );
        } else {
            pipe.write_all(&100u32.to_le_bytes()).await.unwrap();
            pipe.write_all(b"{").await.unwrap();
            drop(pipe);
        }
        process.terminated().await;
        drop(owned);
    }
}
#[tokio::test(flavor = "current_thread")]
async fn idle_heartbeat_deadline_exits_actual_host_without_parent_kill() {
    let startup = Startup::new();
    let mut pipe = transport::create_server(&startup).unwrap();
    let owned = HostProcess::spawn(executable(), &startup).unwrap();
    let process = ProcessWait::open(owned.id());
    timeout(Duration::from_secs(3), pipe.connect())
        .await
        .unwrap()
        .unwrap();
    transport::write(&mut pipe, &frame(&startup, "1", HostMessage::Hello {}))
        .await
        .unwrap();
    assert!(matches!(
        timeout(
            Duration::from_secs(3),
            transport::read::<_, HostReply>(&mut pipe)
        )
        .await
        .unwrap()
        .unwrap()
        .unwrap()
        .body,
        HostReply::Ready {}
    ));
    assert!(
        timeout(
            Duration::from_secs(18),
            transport::read::<_, HostReply>(&mut pipe)
        )
        .await
        .unwrap()
        .unwrap()
        .is_none()
    );
    process.terminated().await;
    drop(owned);
}
#[test]
fn startup_is_narrow_and_native_executable_path_cannot_be_a_shell_or_network() {
    let startup = Startup::new();
    Startup::parse(startup.arguments()).unwrap();
    let mut extra = startup.arguments().to_vec();
    extra.push("--execute".into());
    assert!(Startup::parse(extra).is_err());
    let mut wrong = startup.clone();
    wrong.instance = "../foreign".into();
    assert!(wrong.validate().is_err());
    assert!(HostProcess::spawn(Path::new(r"C:\windows\system32\cmd.exe"), &startup).is_err());
    assert!(
        HostProcess::spawn(
            Path::new(r"\\foreign\share\token-pulse-taskbar-host.exe"),
            &startup
        )
        .is_err()
    );
}
