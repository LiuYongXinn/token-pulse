#![cfg(windows)]
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use token_pulse_core::notify::parse_codex_notification;
use token_pulse_integration::{
    notify_channel::{NotifyCapability, windows::send_hint},
    notify_config::{ManagedNotifyCommand, prepare_enable},
    notify_registry::{NotifyRegistration, RegistryError, windows::NotifyRegistry},
    notify_service::{NotifyService, NotifyServiceError},
};
struct Scene {
    app: tempfile::TempDir,
    home: tempfile::TempDir,
    registry: NotifyRegistry,
    record: NotifyRegistration,
    config: Vec<u8>,
}
fn exe() -> PathBuf {
    std::env::current_exe().unwrap()
}
impl Scene {
    fn new() -> Self {
        let app = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let registry = NotifyRegistry::open(app.path()).unwrap();
        let cap = NotifyCapability::new();
        let cmd = ManagedNotifyCommand::new(&exe(), cap.registration_id()).unwrap();
        let plan = prepare_enable(b"# unrelated\r\nmodel='unchanged'\r\n", &cmd).unwrap();
        let config = plan
            .apply_to(b"# unrelated\r\nmodel='unchanged'\r\n")
            .unwrap();
        std::fs::write(home.path().join("config.toml"), &config).unwrap();
        let record = NotifyRegistration::from_prepared(
            home.path(),
            cap,
            plan.restore_record().clone(),
            false,
        )
        .unwrap();
        registry.create(&record).unwrap();
        Self {
            app,
            home,
            registry,
            record,
            config,
        }
    }
    fn start(&self, count: &Arc<AtomicUsize>) -> NotifyService {
        let count = count.clone();
        NotifyService::start(
            self.registry.clone(),
            exe(),
            Arc::new(move || {
                count.fetch_add(1, Ordering::SeqCst);
            }),
        )
        .unwrap()
    }
}
fn wait(condition: impl Fn() -> bool) {
    let until = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(Instant::now() < until, "notify service condition timed out");
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn send(scene: &Scene, turn: &str) {
    let hint=parse_codex_notification(&serde_json::to_vec(&serde_json::json!({"type":"agent-turn-complete","thread-id":"thread","turn-id":turn,"input-messages":["private body"]})).unwrap()).unwrap().unwrap();
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(send_hint(scene.record.capability(), &hint))
        .unwrap();
}
#[test]
fn drains_startup_marker_online_hints_and_survives_reload_then_releases_owner() {
    let scene = Scene::new();
    let count = Arc::new(AtomicUsize::new(0));
    scene
        .registry
        .mark_pending(scene.record.capability())
        .unwrap();
    let service = scene.start(&count);
    wait(|| {
        service.status().ready
            && service.status().listener_count == Some(1)
            && count.load(Ordering::SeqCst) == 1
    });
    assert!(
        !scene
            .registry
            .has_pending(scene.record.capability().registration_id())
            .unwrap()
    );
    send(&scene, "turn-1");
    send(&scene, "turn-1");
    wait(|| count.load(Ordering::SeqCst) == 2);
    service.reload();
    wait(|| service.status().ready);
    send(&scene, "turn-2");
    wait(|| count.load(Ordering::SeqCst) == 3);
    assert_eq!(
        std::fs::read(scene.home.path().join("config.toml")).unwrap(),
        scene.config
    );
    service.shutdown();
    assert_eq!(service.status().listener_count, Some(0));
    let reopened = scene.start(&count);
    wait(|| reopened.status().listener_count == Some(1));
    reopened.shutdown();
}
#[test]
fn rejects_second_owner_and_recovers_leftover_claim_without_deleting_new_marker() {
    let scene = Scene::new();
    let count = Arc::new(AtomicUsize::new(0));
    scene
        .registry
        .mark_pending(scene.record.capability())
        .unwrap();
    // A fixture left by a previous owner, no forced process termination / disaster test.
    let claim = scene
        .registry
        .claim_pending(scene.record.capability().registration_id())
        .unwrap()
        .unwrap();
    std::mem::forget(claim);
    scene
        .registry
        .mark_pending(scene.record.capability())
        .unwrap();
    let service = scene.start(&count);
    wait(|| count.load(Ordering::SeqCst) == 1);
    let duplicate = scene.start(&count);
    wait(|| duplicate.status().last_error == Some(NotifyServiceError::Registry(RegistryError::Io)));
    assert!(!duplicate.status().ready);
    assert!(duplicate.status().listener_count.is_none());
    assert_eq!(count.load(Ordering::SeqCst), 1);
    service.shutdown();
    duplicate.shutdown();
    assert!(
        !scene
            .registry
            .has_pending(scene.record.capability().registration_id())
            .unwrap()
    );
}
#[test]
fn preserves_pending_on_unreadable_config_and_discards_only_inactive_markers() {
    use std::os::windows::fs::OpenOptionsExt;
    let scene = Scene::new();
    let count = Arc::new(AtomicUsize::new(0));
    let locked = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(scene.home.path().join("config.toml"))
        .unwrap();
    scene
        .registry
        .mark_pending(scene.record.capability())
        .unwrap();
    let service = scene.start(&count);
    wait(|| service.status().last_error == Some(NotifyServiceError::ConfigUnreadable));
    assert_eq!(count.load(Ordering::SeqCst), 0);
    // The transient claim may exist while checked, so wait for its Drop to restore the bit.
    wait(|| {
        scene
            .registry
            .has_pending(scene.record.capability().registration_id())
            .unwrap()
    });
    drop(locked);
    service.reload();
    wait(|| count.load(Ordering::SeqCst) == 1);
    std::fs::write(
        scene.home.path().join("config.toml"),
        b"notify=['user changed']\n",
    )
    .unwrap();
    service.reload();
    wait(|| service.status().listener_count == Some(0));
    scene
        .registry
        .mark_pending(scene.record.capability())
        .unwrap();
    wait(|| {
        !scene
            .registry
            .has_pending(scene.record.capability().registration_id())
            .unwrap()
    });
    assert_eq!(count.load(Ordering::SeqCst), 1);
    service.shutdown();
}
#[test]
fn isolates_corrupt_record_and_reloads_new_valid_registration() {
    let scene = Scene::new();
    let count = Arc::new(AtomicUsize::new(0));
    let service = scene.start(&count);
    wait(|| service.status().listener_count == Some(1));
    let other_home = tempfile::tempdir().unwrap();
    let cap = NotifyCapability::new();
    let cmd = ManagedNotifyCommand::new(&exe(), cap.registration_id()).unwrap();
    let plan = prepare_enable(b"", &cmd).unwrap();
    std::fs::write(
        other_home.path().join("config.toml"),
        plan.apply_to(b"").unwrap(),
    )
    .unwrap();
    let other = NotifyRegistration::from_prepared(
        other_home.path(),
        cap,
        plan.restore_record().clone(),
        false,
    )
    .unwrap();
    scene.registry.create(&other).unwrap();
    service.reload();
    wait(|| service.status().listener_count == Some(2));
    let record_path = scene.app.path().join("notify").join(format!(
        "{}.registration.json",
        other.capability().registration_id()
    ));
    let before = std::fs::read(&record_path).unwrap();
    std::fs::write(&record_path, b"broken metadata").unwrap();
    service.reload();
    wait(|| {
        service.status().listener_count == Some(1)
            && service.status().last_error
                == Some(NotifyServiceError::Registry(RegistryError::InvalidRecord))
    });
    send(&scene, "healthy");
    wait(|| count.load(Ordering::SeqCst) == 1);
    std::fs::write(record_path, before).unwrap();
    service.reload();
    wait(|| service.status().listener_count == Some(2) && service.status().last_error.is_none());
    service.shutdown();
}
