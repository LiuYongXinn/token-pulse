#![cfg(windows)]
use std::{fs, os::windows::fs::OpenOptionsExt, path::PathBuf};
use token_pulse_integration::{
    notify_config::{ConfigError, windows::ConfigFileError},
    notify_manager::{NotifyManager, NotifyOperation, OperationError},
    notify_registry::{
        RegistryError,
        windows::{NotifyRegistry, RetireDisposition, RetireError},
    },
};
struct Scene {
    app: tempfile::TempDir,
    home: tempfile::TempDir,
    registry: NotifyRegistry,
    manager: NotifyManager,
}
impl Scene {
    fn new(before: Option<&[u8]>) -> Self {
        let app = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        if let Some(bytes) = before {
            fs::write(home.path().join("config.toml"), bytes).unwrap();
        }
        let registry = NotifyRegistry::open(app.path()).unwrap();
        let manager = NotifyManager::new(registry.clone(), std::env::current_exe().unwrap());
        Self {
            app,
            home,
            registry,
            manager,
        }
    }
    fn config(&self) -> PathBuf {
        self.home.path().join("config.toml")
    }
}
#[test]
fn reviewed_enable_defaults_to_preserve_original_and_disable_restores_latest_user_settings() {
    let before = b"notify=['old.exe'] # original\r\nmodel='keep'\r\n";
    let mut scene = Scene::new(Some(before));
    let preview = scene
        .manager
        .prepare_enable(scene.home.path(), None)
        .unwrap();
    assert_eq!(preview.operation, NotifyOperation::Enable);
    assert!(preview.chain_original && preview.can_chain_original);
    assert_eq!(preview.before_value.as_deref(), Some("['old.exe']"));
    assert_eq!(fs::read(scene.config()).unwrap(), before);
    assert!(scene.registry.registration_ids().unwrap().is_empty());
    let result = scene.manager.apply(&preview.plan_id).unwrap();
    assert!(result.configured && result.cleanup_error.is_none());
    assert_eq!(result.registration_id, preview.registration_id);
    assert_eq!(
        scene.manager.apply(&preview.plan_id).err(),
        Some(OperationError::PlanNotFound)
    );
    let rows = scene.manager.registrations().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].configured, Some(true));
    assert_eq!(rows[0].current_executable, Some(true));
    let installed = fs::read(scene.config()).unwrap();
    let mut changed = installed;
    changed.extend_from_slice(b"new_key='user'\r\n");
    fs::write(scene.config(), changed).unwrap();
    let undo = scene
        .manager
        .prepare_disable(&preview.registration_id)
        .unwrap();
    assert_eq!(undo.operation, NotifyOperation::Disable);
    let result = scene.manager.apply(&undo.plan_id).unwrap();
    assert!(!result.configured && result.cleanup_error.is_none());
    let mut expected = before.to_vec();
    expected.extend_from_slice(b"new_key='user'\r\n");
    assert_eq!(fs::read(scene.config()).unwrap(), expected);
    assert!(scene.manager.registrations().unwrap().is_empty());
    assert_eq!(
        scene
            .manager
            .retire_inactive(&preview.registration_id)
            .unwrap(),
        RetireDisposition::AlreadyAbsent
    );
}
#[test]
fn stale_and_busy_enable_do_not_allocate_records_and_busy_can_retry_same_review() {
    let mut scene = Scene::new(Some(b"model='keep'\n"));
    let preview = scene
        .manager
        .prepare_enable(scene.home.path(), None)
        .unwrap();
    let writer = fs::OpenOptions::new()
        .write(true)
        .share_mode(7)
        .open(scene.config())
        .unwrap();
    assert_eq!(
        scene.manager.apply(&preview.plan_id).err(),
        Some(OperationError::Config(ConfigFileError::Busy))
    );
    assert!(scene.registry.registration_ids().unwrap().is_empty());
    drop(writer);
    fs::write(scene.config(), b"model='user-edited'\n").unwrap();
    assert_eq!(
        scene.manager.apply(&preview.plan_id).err(),
        Some(OperationError::Config(ConfigError::StalePlan.into()))
    );
    assert!(scene.registry.registration_ids().unwrap().is_empty());
    assert!(scene.manager.release(&preview.plan_id));
    let preview = scene
        .manager
        .prepare_enable(scene.home.path(), None)
        .unwrap();
    let writer = fs::OpenOptions::new()
        .write(true)
        .share_mode(7)
        .open(scene.config())
        .unwrap();
    assert!(scene.manager.apply(&preview.plan_id).is_err());
    drop(writer);
    assert!(scene.manager.apply(&preview.plan_id).unwrap().configured);
}
#[test]
fn registry_failure_rolls_back_staged_config_and_allows_same_plan_retry() {
    let before = b"model='keep'\n";
    let mut scene = Scene::new(Some(before));
    let preview = scene
        .manager
        .prepare_enable(scene.home.path(), None)
        .unwrap();
    let lock = scene.app.path().join("notify").join(".registry.lock");
    // Inherited rather than protected file ACL is a real unsafe registry allocation failure.
    fs::write(&lock, b"").unwrap();
    assert_eq!(
        scene.manager.apply(&preview.plan_id).err(),
        Some(OperationError::Registry(RegistryError::UnsafePermissions))
    );
    assert_eq!(fs::read(scene.config()).unwrap(), before);
    assert!(scene.registry.registration_ids().unwrap().is_empty());
    fs::remove_file(lock).unwrap();
    assert!(scene.manager.apply(&preview.plan_id).unwrap().configured);
}
#[test]
fn successful_disable_with_busy_retirement_is_reported_separately_and_can_finish_later() {
    let before = b"model='keep'\n";
    let mut scene = Scene::new(Some(before));
    let enable = scene
        .manager
        .prepare_enable(scene.home.path(), None)
        .unwrap();
    scene.manager.apply(&enable.plan_id).unwrap();
    let disable = scene
        .manager
        .prepare_disable(&enable.registration_id)
        .unwrap();
    let path = scene
        .app
        .path()
        .join("notify")
        .join(format!("{}.registration.json", enable.registration_id));
    let reader = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(path)
        .unwrap();
    let result = scene.manager.apply(&disable.plan_id).unwrap();
    assert!(!result.configured);
    assert_eq!(
        result.cleanup_error,
        Some(OperationError::Retirement(RetireError::Registry(
            RegistryError::Io
        )))
    );
    assert_eq!(fs::read(scene.config()).unwrap(), before);
    assert_eq!(
        scene.manager.registrations().unwrap()[0].configured,
        Some(false)
    );
    drop(reader);
    assert_eq!(
        scene
            .manager
            .retire_inactive(&enable.registration_id)
            .unwrap(),
        RetireDisposition::Retired
    );
    assert!(scene.registry.registration_ids().unwrap().is_empty());
}
#[test]
fn ordinary_enable_disable_more_than_registration_limit_does_not_leak_slots() {
    let mut scene = Scene::new(None);
    assert_eq!(
        scene
            .manager
            .prepare_enable(scene.home.path(), Some(true))
            .err(),
        Some(OperationError::NoOriginalCommand)
    );
    for _ in 0..18 {
        let enable = scene
            .manager
            .prepare_enable(scene.home.path(), None)
            .unwrap();
        assert!(!enable.chain_original);
        scene.manager.apply(&enable.plan_id).unwrap();
        let disable = scene
            .manager
            .prepare_disable(&enable.registration_id)
            .unwrap();
        assert!(
            scene
                .manager
                .apply(&disable.plan_id)
                .unwrap()
                .cleanup_error
                .is_none()
        );
        assert!(scene.registry.registration_ids().unwrap().is_empty());
    }
    assert_eq!(fs::read(scene.config()).unwrap(), b"");
    assert_eq!(
        fs::read_dir(scene.app.path().join("notify"))
            .unwrap()
            .count(),
        1
    ); // fixed lock only
}
#[test]
fn unavailable_config_is_null_and_corrupt_registration_is_isolated_from_healthy_home() {
    let mut scene = Scene::new(Some(b"model='keep'\n"));
    let first = scene
        .manager
        .prepare_enable(scene.home.path(), None)
        .unwrap();
    scene.manager.apply(&first.plan_id).unwrap();
    fs::write(scene.config(), b"not valid toml !").unwrap();
    let rows = scene.manager.registrations().unwrap();
    assert_eq!(rows[0].configured, None);
    assert!(rows[0].error.is_some());
    let record = scene
        .app
        .path()
        .join("notify")
        .join(format!("{}.registration.json", first.registration_id));
    fs::write(record, b"bad record").unwrap();
    let other = tempfile::tempdir().unwrap();
    let second = scene.manager.prepare_enable(other.path(), None).unwrap();
    scene.manager.apply(&second.plan_id).unwrap();
    let rows = scene.manager.registrations().unwrap();
    assert_eq!(rows.len(), 2);
    assert!(
        rows.iter()
            .any(|row| row.registration_id == first.registration_id
                && row.home.is_none()
                && row.configured.is_none()
                && row.error.is_some())
    );
    assert!(
        rows.iter().any(
            |row| row.registration_id == second.registration_id && row.configured == Some(true)
        )
    );
}
