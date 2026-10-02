#![cfg(all(windows, feature = "test-fixture"))]
use std::{ffi::OsString, path::Path, sync::Arc, time::Duration};
use token_pulse_integration::{
    notify_channel::{NotifyCapability, windows::NotifyListener},
    notify_config::{ManagedNotifyCommand, prepare_enable},
    notify_invocation::{
        parse_invocation,
        windows::{DispatchOutcome, WakeOutcome, dispatch},
    },
    notify_original::OriginalCommandError,
    notify_registry::{NotifyRegistration, RegistryError, windows::NotifyRegistry},
    notify_service::NotifyService,
};
const PAYLOAD: &str = r#"{"type":"agent-turn-complete","thread-id":"original-thread","turn-id":"original-turn","input-messages":["synthetic body \"; & |"],"last-assistant-message":"合成","cwd":"ignored-directory"}"#;
struct Scene {
    _directory: tempfile::TempDir,
    home: tempfile::TempDir,
    registry: NotifyRegistry,
    record: NotifyRegistration,
    installed: Vec<u8>,
}
impl Scene {
    fn new(original: &[String], chain: bool) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let registry = NotifyRegistry::open(directory.path()).unwrap();
        let cap = NotifyCapability::new();
        let command =
            ManagedNotifyCommand::new(&std::env::current_exe().unwrap(), cap.registration_id())
                .unwrap();
        let before = format!(
            "notify = {}\nmodel='synthetic'\n",
            serde_json::to_string(original).unwrap()
        );
        let plan = prepare_enable(before.as_bytes(), &command).unwrap();
        let installed = plan.apply_to(before.as_bytes()).unwrap();
        std::fs::write(home.path().join("config.toml"), &installed).unwrap();
        let record = NotifyRegistration::from_prepared(
            home.path(),
            cap,
            plan.restore_record().clone(),
            chain,
        )
        .unwrap();
        registry.create(&record).unwrap();
        Self {
            _directory: directory,
            home,
            registry,
            record,
            installed,
        }
    }
    fn invoke(&self, payload: &str) -> Result<DispatchOutcome, RegistryError> {
        let invocation = parse_invocation(
            [
                "--tokenpulse-notify",
                "--integration",
                self.record.capability().registration_id(),
                PAYLOAD,
            ]
            .map(OsString::from),
        )
        .unwrap()
        .unwrap();
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(dispatch(
                &self.registry,
                &std::env::current_exe().unwrap(),
                &invocation,
                std::ffi::OsStr::new(payload),
            ))
    }
    fn unchanged(&self) {
        assert_eq!(
            std::fs::read(self.home.path().join("config.toml")).unwrap(),
            self.installed
        );
        assert_eq!(std::fs::read_dir(self.home.path()).unwrap().count(), 1);
    }
}
fn fixture(mode: &str) -> Vec<String> {
    vec![
        env!("CARGO_BIN_EXE_notify-original-fixture").into(),
        mode.into(),
    ]
}
#[test]
fn original_receives_one_unchanged_argument_and_wake_stays_minimal() {
    let mut original = fixture("success");
    original.push("space quote \" and 中文".into());
    let scene = Scene::new(&original, true);
    let (tx, rx) = std::sync::mpsc::channel();
    let listener = NotifyListener::start(
        scene.record.capability().clone(),
        Arc::new(move |hint| {
            tx.send(hint).unwrap();
        }),
    )
    .unwrap();
    let outcome = scene.invoke(PAYLOAD).unwrap();
    assert_eq!(outcome.wake, Ok(WakeOutcome::Delivered));
    assert_eq!(outcome.original, Some(Ok(())));
    assert_eq!(
        serde_json::to_value(rx.recv_timeout(Duration::from_secs(1)).unwrap()).unwrap(),
        serde_json::json!({"thread_id":"original-thread","turn_id":"original-turn"})
    );
    scene.unchanged();
    drop(listener);
}
#[test]
fn original_failure_never_loses_offline_wake_and_false_choice_never_executes() {
    let scene = Scene::new(&fixture("failure"), true);
    let outcome = scene.invoke(PAYLOAD).unwrap();
    assert_eq!(outcome.wake, Ok(WakeOutcome::Pending));
    assert_eq!(outcome.original, Some(Err(OriginalCommandError::Failed)));
    assert!(
        scene
            .registry
            .has_pending(scene.record.capability().registration_id())
            .unwrap()
    );
    scene.unchanged();
    let disabled = Scene::new(&fixture("failure"), false);
    let outcome = disabled.invoke(PAYLOAD).unwrap();
    assert_eq!(outcome.wake, Ok(WakeOutcome::Pending));
    assert!(outcome.original.is_none());
    disabled.unchanged();
}
#[test]
fn stale_config_mismatched_payload_recursion_and_implicit_batch_are_rejected() {
    let scene = Scene::new(&fixture("failure"), true);
    assert_eq!(
        scene
            .invoke(r#"{"type":"agent-turn-complete","thread-id":"different"}"#)
            .err(),
        Some(RegistryError::InvalidRecord)
    );
    assert!(
        !scene
            .registry
            .has_pending(scene.record.capability().registration_id())
            .unwrap()
    );
    std::fs::write(
        scene.home.path().join("config.toml"),
        "notify=['changed-by-user']\n",
    )
    .unwrap();
    let outcome = scene.invoke(PAYLOAD).unwrap();
    assert_eq!(outcome.wake, Ok(WakeOutcome::Ignored));
    assert!(outcome.original.is_none());
    let recursive = Scene::new(
        &[std::env::current_exe()
            .unwrap()
            .to_str()
            .unwrap()
            .to_uppercase()],
        true,
    );
    assert_eq!(
        recursive.invoke(PAYLOAD).unwrap().original,
        Some(Err(OriginalCommandError::InvalidCommand))
    );
    recursive.unchanged();
    let own_executable = std::env::current_exe().unwrap();
    let alias_directory = tempfile::tempdir_in(own_executable.parent().unwrap()).unwrap();
    let alias = alias_directory.path().join("another-name.exe");
    std::fs::hard_link(&own_executable, &alias).unwrap();
    let alias_scene = Scene::new(&[alias.to_str().unwrap().into()], true);
    assert_eq!(
        alias_scene.invoke(PAYLOAD).unwrap().original,
        Some(Err(OriginalCommandError::InvalidCommand))
    );
    alias_scene.unchanged();
    let batch = Scene::new(&["unapproved-wrapper.cmd".into()], true);
    assert_eq!(
        batch.invoke(PAYLOAD).unwrap().original,
        Some(Err(OriginalCommandError::ProgramUnavailable))
    );
    batch.unchanged();
}
#[test]
fn timeout_kills_only_owned_original_tree_and_preserves_pending_wake() {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, WAIT_OBJECT_0},
        System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
    };
    let marker = tempfile::tempdir().unwrap();
    let marker_path = marker.path().join("owned-child-pid");
    let mut original = fixture("tree");
    original.push(marker_path.to_str().unwrap().into());
    let scene = Scene::new(&original, true);
    let start = std::time::Instant::now();
    let outcome = scene.invoke(PAYLOAD).unwrap();
    assert_eq!(outcome.wake, Ok(WakeOutcome::Pending));
    assert_eq!(outcome.original, Some(Err(OriginalCommandError::Timeout)));
    assert!(start.elapsed() < Duration::from_secs(8)); // functional timeout, not a benchmark
    let pid: u32 = std::fs::read_to_string(marker_path)
        .unwrap()
        .parse()
        .unwrap();
    unsafe {
        let process = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
        if !process.is_null() {
            assert_eq!(WaitForSingleObject(process, 1000), WAIT_OBJECT_0);
            CloseHandle(process);
        }
    }
    assert!(
        scene
            .registry
            .has_pending(scene.record.capability().registration_id())
            .unwrap()
    );
    scene.unchanged();
}
#[test]
fn relative_original_uses_inherited_directory_and_chain_registration_gets_real_listener() {
    let program = Path::new(env!("CARGO_BIN_EXE_notify-original-fixture"));
    let cwd = std::env::current_dir().unwrap();
    let relative = cwd
        .ancestors()
        .enumerate()
        .find_map(|(levels, ancestor)| {
            program.strip_prefix(ancestor).ok().map(|tail| {
                let mut path = std::path::PathBuf::new();
                for _ in 0..levels {
                    path.push("..");
                }
                path.push(tail);
                path
            })
        })
        .unwrap();
    let scene = Scene::new(&[relative.to_str().unwrap().into(), "failure".into()], true);
    assert_eq!(
        scene.invoke(PAYLOAD).unwrap().original,
        Some(Err(OriginalCommandError::Failed))
    );
    scene.unchanged();
    let service = NotifyService::start(
        scene.registry.clone(),
        std::env::current_exe().unwrap(),
        Arc::new(|| {}),
    )
    .unwrap();
    let until = std::time::Instant::now() + Duration::from_secs(2);
    while service.status().listener_count != Some(1) && std::time::Instant::now() < until {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(service.status().listener_count, Some(1));
    assert!(service.status().last_error.is_none());
    service.shutdown();
}
