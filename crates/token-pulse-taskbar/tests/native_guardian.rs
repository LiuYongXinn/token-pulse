#![cfg(windows)]
use std::{
    io::Read,
    os::windows::process::CommandExt,
    path::Path,
    process::{Child, Command, Stdio},
};
use token_pulse_taskbar::windows::{Startup, TransportError, guardian::Guardian};
use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;
struct OwnedTestChild(Child);
impl Drop for OwnedTestChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
#[ignore = "Private child gate run explicitly by guardian image test; no native windows or data"]
fn guardian_image_gate_child() {
    let mut gate = [0];
    let _ = std::io::stdin().read(&mut gate);
}
#[tokio::test(flavor = "current_thread")]
async fn guardian_refuses_a_different_native_image_without_terminating_that_owned_process() {
    let child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "guardian_image_gate_child",
            "--ignored",
            "--nocapture",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut owned = OwnedTestChild(child);
    let result = Guardian::launch(
        Path::new(env!("CARGO_BIN_EXE_token-pulse-taskbar-host")),
        &Startup::new(),
        &owned.0,
    )
    .await;
    assert!(
        matches!(result, Err(TransportError::PeerMismatch)),
        "image mismatch must not arm cleanup"
    );
    assert!(
        owned.0.try_wait().unwrap().is_none(),
        "rejected guardian never terminates its target"
    );
}
