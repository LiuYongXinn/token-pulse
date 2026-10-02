#![cfg(windows)]
use std::fs;
use token_pulse_integration::{
    notify_channel::NotifyCapability,
    notify_config::{ManagedNotifyCommand, prepare_enable},
    notify_registry::{
        NotifyRegistration, RegistryError,
        windows::{NotifyRegistry, RetireDisposition, RetireError},
    },
};
struct Scene {
    app: tempfile::TempDir,
    home: tempfile::TempDir,
    registry: NotifyRegistry,
    record: NotifyRegistration,
    installed: Vec<u8>,
}
impl Scene {
    fn new(active: bool) -> Self {
        let app = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let registry = NotifyRegistry::open(app.path()).unwrap();
        let cap = NotifyCapability::new();
        let command =
            ManagedNotifyCommand::new(&std::env::current_exe().unwrap(), cap.registration_id())
                .unwrap();
        let before = b"model='keep'\n";
        let plan = prepare_enable(before, &command).unwrap();
        let installed = plan.apply_to(before).unwrap();
        fs::write(
            home.path().join("config.toml"),
            if active { &installed[..] } else { before },
        )
        .unwrap();
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
            installed,
        }
    }
}
#[test]
fn active_and_foreign_capability_cannot_retire_recovery_information() {
    let scene = Scene::new(true);
    scene
        .registry
        .mark_pending(scene.record.capability())
        .unwrap();
    assert_eq!(
        scene.registry.retire(scene.record.capability()),
        Err(RetireError::ActiveConfiguration)
    );
    let mut fake = serde_json::to_value(NotifyCapability::new()).unwrap();
    fake["registration_id"] = scene.record.capability().registration_id().into();
    let fake: NotifyCapability = serde_json::from_value(fake).unwrap();
    assert_eq!(
        scene.registry.retire(&fake),
        Err(RetireError::Registry(RegistryError::Unauthorized))
    );
    assert_eq!(
        fs::read(scene.home.path().join("config.toml")).unwrap(),
        scene.installed
    );
    assert!(
        scene
            .registry
            .has_pending(scene.record.capability().registration_id())
            .unwrap()
    );
}
#[test]
fn retire_cleans_pending_and_live_claim_without_drop_resurrecting_registration() {
    let scene = Scene::new(false);
    let id = scene.record.capability().registration_id();
    scene
        .registry
        .mark_pending(scene.record.capability())
        .unwrap();
    let claim = scene.registry.claim_pending(id).unwrap().unwrap();
    scene
        .registry
        .mark_pending(scene.record.capability())
        .unwrap();
    assert_eq!(
        scene.registry.retire(scene.record.capability()).unwrap(),
        RetireDisposition::Retired
    );
    drop(claim);
    assert!(!scene.registry.has_pending(id).unwrap());
    assert!(scene.registry.registration_ids().unwrap().is_empty());
    assert_eq!(
        scene.registry.mark_pending(scene.record.capability()),
        Err(RegistryError::NotFound)
    );
    assert_eq!(
        scene.registry.retire(scene.record.capability()).unwrap(),
        RetireDisposition::AlreadyAbsent
    );
    assert_eq!(
        fs::read(scene.home.path().join("config.toml")).unwrap(),
        b"model='keep'\n"
    );
    assert_eq!(
        fs::read_dir(scene.app.path().join("notify"))
            .unwrap()
            .count(),
        1
    );
    // An in-flight completion already removed by retirement is idempotent too.
    let another = Scene::new(false);
    let id = another.record.capability().registration_id();
    another
        .registry
        .mark_pending(another.record.capability())
        .unwrap();
    let claim = another.registry.claim_pending(id).unwrap().unwrap();
    another
        .registry
        .retire(another.record.capability())
        .unwrap();
    claim.complete().unwrap();
}
#[test]
fn malformed_marker_prevents_all_removals_and_missing_config_stays_missing() {
    let scene = Scene::new(false);
    let id = scene.record.capability().registration_id();
    scene
        .registry
        .mark_pending(scene.record.capability())
        .unwrap();
    let marker = scene.app.path().join("notify").join(format!("{id}.wake"));
    fs::write(&marker, b"unexpected contents").unwrap();
    assert_eq!(
        scene.registry.retire(scene.record.capability()),
        Err(RetireError::Registry(RegistryError::InvalidMarker))
    );
    assert_eq!(fs::read(&marker).unwrap(), b"unexpected contents");
    assert!(scene.registry.get(id).is_ok());
    fs::write(marker, b"").unwrap();
    fs::remove_file(scene.home.path().join("config.toml")).unwrap();
    scene.registry.retire(scene.record.capability()).unwrap();
    assert!(!scene.home.path().join("config.toml").exists());
}
#[test]
fn concurrent_marker_writers_and_retirement_never_leave_orphan_wake() {
    let scene = Scene::new(false);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(9));
    let mut threads = Vec::new();
    for _ in 0..8 {
        let registry = scene.registry.clone();
        let cap = scene.record.capability().clone();
        let barrier = barrier.clone();
        threads.push(std::thread::spawn(move || {
            barrier.wait();
            registry.mark_pending(&cap)
        }));
    }
    barrier.wait();
    scene.registry.retire(scene.record.capability()).unwrap();
    for thread in threads {
        match thread.join().unwrap() {
            Ok(_) | Err(RegistryError::NotFound) => {}
            Err(error) => panic!("unexpected finite error: {error}"),
        }
    }
    assert!(scene.registry.registration_ids().unwrap().is_empty());
    assert!(
        !scene
            .registry
            .has_pending(scene.record.capability().registration_id())
            .unwrap()
    );
}
