#![cfg(all(windows, debug_assertions))]
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
    time::Duration,
};
use token_pulse_integration::{
    notify_channel::{NotifyCapability, windows::NotifyListener},
    notify_config::{ManagedNotifyCommand, prepare_enable},
    notify_registry::{NotifyRegistration, windows::NotifyRegistry},
};
struct Scene {
    directory: PathBuf,
    probe_name: String,
    home: tempfile::TempDir,
    registry: NotifyRegistry,
    registration: NotifyRegistration,
    installed: Vec<u8>,
}
fn executable() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_token-pulse-desktop"))
}
impl Scene {
    fn new(exe: &Path, chain: bool) -> Self {
        let probe_name = format!("native-notify-{}", uuid::Uuid::new_v4().simple());
        let directory = dirs::data_local_dir()
            .unwrap()
            .join("com.tokenpulse.desktop.dev")
            .join(&probe_name);
        std::fs::create_dir_all(&directory).unwrap();
        let home = tempfile::tempdir().unwrap();
        let registry = NotifyRegistry::open(&directory).unwrap();
        let cap = NotifyCapability::new();
        let command = ManagedNotifyCommand::new(exe, cap.registration_id()).unwrap();
        let before = b"notify=['original.exe'] # original\r\nmodel='leave alone'\r\n";
        let plan = prepare_enable(before, &command).unwrap();
        let installed = plan.apply_to(before).unwrap();
        std::fs::write(home.path().join("config.toml"), &installed).unwrap();
        let registration = NotifyRegistration::from_prepared(
            home.path(),
            cap,
            plan.restore_record().clone(),
            chain,
        )
        .unwrap();
        registry.create(&registration).unwrap();
        Self {
            directory,
            probe_name,
            home,
            registry,
            registration,
            installed,
        }
    }
    fn invoke(&self, extras: &[&str]) -> std::process::Output {
        let payload = r#"{"type":"agent-turn-complete","thread-id":"native-thread","turn-id":"native-turn","input-messages":["never persist body"],"last-assistant-message":"private response","cwd":"private directory"}"#;
        let mut child = Command::new(executable())
            .env("TOKENPULSE_NATIVE_NOTIFY_PROBE", &self.probe_name)
            .args([
                "--tokenpulse-notify",
                "--integration",
                self.registration.capability().registration_id(),
                payload,
            ])
            .args(extras)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            if child.try_wait().unwrap().is_some() {
                break;
            }
            if std::time::Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("headless executable did not exit");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        child.wait_with_output().unwrap()
    }
    fn require_no_gui_initialization_or_source_write(&self) {
        assert_eq!(
            std::fs::read(self.home.path().join("config.toml")).unwrap(),
            self.installed
        );
        let entries: Vec<_> = std::fs::read_dir(&self.directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(entries, vec![std::ffi::OsString::from("notify")]); // no SQLite / logs / Tauri startup artifacts
    }
}
#[test]
fn production_executable_notifies_online_without_gui_or_source_writes() {
    let scene = Scene::new(&executable(), false);
    let (tx, rx) = std::sync::mpsc::channel();
    let listener = NotifyListener::start(
        scene.registration.capability().clone(),
        Arc::new(move |hint| {
            let _ = tx.send(hint);
        }),
    )
    .unwrap();
    let output = scene.invoke(&[]);
    assert!(output.status.success(), "headless notify failed");
    assert!(output.stdout.is_empty() && output.stderr.is_empty());
    let hint = rx.recv_timeout(Duration::from_secs(1)).unwrap();
    assert_eq!(
        serde_json::to_value(hint).unwrap(),
        serde_json::json!({"thread_id":"native-thread","turn_id":"native-turn"})
    );
    assert!(
        scene
            .registry
            .claim_pending(scene.registration.capability().registration_id())
            .unwrap()
            .is_none()
    );
    scene.require_no_gui_initialization_or_source_write();
    drop(listener);
}
#[test]
fn production_executable_coalesces_offline_notifications() {
    let scene = Scene::new(&executable(), false);
    assert!(scene.invoke(&[]).status.success());
    assert!(scene.invoke(&[]).status.success());
    let marker = scene.directory.join("notify").join(format!(
        "{}.wake",
        scene.registration.capability().registration_id()
    ));
    assert_eq!(std::fs::metadata(marker).unwrap().len(), 0);
    scene
        .registry
        .claim_pending(scene.registration.capability().registration_id())
        .unwrap()
        .unwrap()
        .complete()
        .unwrap();
    assert!(
        scene
            .registry
            .claim_pending(scene.registration.capability().registration_id())
            .unwrap()
            .is_none()
    );
    scene.require_no_gui_initialization_or_source_write();
}
#[test]
fn stale_config_wrong_executable_legacy_choice_and_bad_args_never_launch_gui() {
    let mut inactive = Scene::new(&executable(), false);
    inactive.installed = b"notify=['user-changed']\n".to_vec();
    std::fs::write(
        inactive.home.path().join("config.toml"),
        &inactive.installed,
    )
    .unwrap();
    assert!(inactive.invoke(&[]).status.success());
    assert!(
        inactive
            .registry
            .claim_pending(inactive.registration.capability().registration_id())
            .unwrap()
            .is_none()
    );
    inactive.require_no_gui_initialization_or_source_write();
    let wrong = Scene::new(&std::env::temp_dir().join("another-app.exe"), false);
    let output = wrong.invoke(&[]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        String::from_utf8(output.stderr).unwrap().trim(),
        "notify_registry_unauthorized"
    );
    wrong.require_no_gui_initialization_or_source_write();
    let legacy = Scene::new(&executable(), true);
    let output = legacy.invoke(&[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        std::str::from_utf8(&output.stderr)
            .unwrap()
            .contains("notify_original_chain_unavailable")
    );
    legacy.require_no_gui_initialization_or_source_write();
    let output = inactive.invoke(&["extra-secret"]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        String::from_utf8(output.stderr).unwrap().trim(),
        "notify_invocation_invalid_arguments"
    );
}
