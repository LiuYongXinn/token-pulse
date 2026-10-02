use std::path::{Path, PathBuf};
use token_pulse_integration::{
    notify_channel::NotifyCapability,
    notify_config::{ManagedNotifyCommand, prepare_enable},
    notify_registry::{NotifyRegistration, RegistryError},
};

fn registration(home: &Path, original: &[u8], chain: bool) -> NotifyRegistration {
    let capability = NotifyCapability::new();
    let exe = std::env::temp_dir().join("TokenPulse.exe");
    let command = ManagedNotifyCommand::new(&exe, capability.registration_id()).unwrap();
    let plan = prepare_enable(original, &command).unwrap();
    NotifyRegistration::from_prepared(home, capability, plan.restore_record().clone(), chain)
        .unwrap()
}
fn home() -> PathBuf {
    std::env::temp_dir().join("synthetic-codex-home")
}

#[test]
fn record_links_the_same_registration_and_requires_explicit_original_chain_choice() {
    let original = b"notify=['original.exe','arg']\nsecret='never retain'\n";
    let registered = registration(&home(), original, true);
    assert!(registered.chain_original());
    assert_eq!(
        registered.restore_record().original_arguments().unwrap(),
        Some(vec!["original.exe".into(), "arg".into()])
    );
    let encoded = serde_json::to_vec(&registered).unwrap();
    assert!(
        !std::str::from_utf8(&encoded)
            .unwrap()
            .contains("never retain")
    );
    let decoded: NotifyRegistration = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(
        decoded.capability().registration_id(),
        registered.capability().registration_id()
    );
    assert_eq!(decoded.codex_home(), home());
    assert!(decoded.chain_original());
    assert_eq!(
        NotifyRegistration::from_prepared(
            &home(),
            NotifyCapability::new(),
            registered.restore_record().clone(),
            false
        )
        .err()
        .unwrap(),
        RegistryError::InvalidRecord
    );
    let no_original = registration(&home(), b"", false);
    assert_eq!(
        NotifyRegistration::from_prepared(
            &home(),
            no_original.capability().clone(),
            no_original.restore_record().clone(),
            true
        )
        .err()
        .unwrap(),
        RegistryError::InvalidRecord
    );
    assert_eq!(
        NotifyRegistration::from_prepared(
            Path::new("relative"),
            registered.capability().clone(),
            registered.restore_record().clone(),
            false
        )
        .err()
        .unwrap(),
        RegistryError::InvalidRecord
    );
    #[cfg(windows)]
    assert_eq!(
        NotifyRegistration::from_prepared(
            Path::new(r"\\server\share\codex-home"),
            registered.capability().clone(),
            registered.restore_record().clone(),
            false
        )
        .err()
        .unwrap(),
        RegistryError::InvalidRecord
    );
}
#[test]
fn malformed_persisted_records_cannot_change_version_home_identity_or_choice_defaults() {
    let registered = registration(&home(), b"notify=['old']", false);
    let wire = serde_json::to_value(&registered).unwrap();
    for (key, value) in [
        ("version", serde_json::json!(2)),
        ("codex_home", serde_json::json!("relative")),
        ("chain_original", serde_json::json!("yes")),
        ("extra", serde_json::json!("secret")),
    ] {
        let mut changed = wire.clone();
        changed[key] = value;
        assert!(serde_json::from_value::<NotifyRegistration>(changed).is_err());
    }
    let mut changed = wire.clone();
    changed["capability"]["registration_id"] = serde_json::json!("0".repeat(32));
    assert!(serde_json::from_value::<NotifyRegistration>(changed).is_err());
    let mut missing = wire;
    missing.as_object_mut().unwrap().remove("chain_original");
    assert!(serde_json::from_value::<NotifyRegistration>(missing).is_err());
}

#[cfg(windows)]
mod native {
    use super::*;
    use token_pulse_integration::notify_registry::{
        MAX_REGISTRATION_BYTES, MAX_REGISTRATIONS,
        windows::{MarkDisposition, NotifyRegistry},
    };
    fn registry_path(temp: &tempfile::TempDir, id: &str) -> PathBuf {
        temp.path()
            .join("notify")
            .join(format!("{id}.registration.json"))
    }
    #[test]
    fn real_private_record_round_trip_is_immutable_and_survives_reopen() {
        let temp = tempfile::tempdir().unwrap();
        let registry = NotifyRegistry::open(temp.path()).unwrap();
        let home = temp.path().join("codex-home");
        std::fs::create_dir(&home).unwrap();
        let source = b"notify = ['old.exe'] # keep\r\nmodel='unchanged'\r\n";
        std::fs::write(home.join("config.toml"), source).unwrap();
        let record = registration(&home, source, true);
        registry.create(&record).unwrap();
        assert_eq!(
            registry.create(&record).unwrap_err(),
            RegistryError::AlreadyExists
        );
        assert_eq!(std::fs::read(home.join("config.toml")).unwrap(), source);
        drop(registry);
        let reopened = NotifyRegistry::open(temp.path()).unwrap();
        let actual = reopened.get(record.capability().registration_id()).unwrap();
        assert_eq!(actual.codex_home(), home);
        assert!(actual.chain_original());
        assert_eq!(
            actual.restore_record().original_value(),
            Some("['old.exe']")
        );
        assert_eq!(reopened.registrations().unwrap().len(), 1);
        assert!(
            !std::fs::read_to_string(registry_path(&temp, record.capability().registration_id()))
                .unwrap()
                .contains("model")
        );
    }
    #[test]
    fn coalesces_offline_hints_and_completion_preserves_a_new_arrival() {
        let temp = tempfile::tempdir().unwrap();
        let registry = NotifyRegistry::open(temp.path()).unwrap();
        let record = registration(&home(), b"", false);
        registry.create(&record).unwrap();
        let cap = record.capability();
        assert_eq!(
            registry.mark_pending(cap).unwrap(),
            MarkDisposition::Created
        );
        assert_eq!(
            registry.mark_pending(cap).unwrap(),
            MarkDisposition::AlreadyPending
        );
        let marker = temp
            .path()
            .join("notify")
            .join(format!("{}.wake", cap.registration_id()));
        assert_eq!(std::fs::metadata(marker).unwrap().len(), 0);
        let claimed = registry
            .claim_pending(cap.registration_id())
            .unwrap()
            .unwrap();
        assert!(
            registry
                .claim_pending(cap.registration_id())
                .unwrap()
                .is_none()
        );
        assert_eq!(
            registry.mark_pending(cap).unwrap(),
            MarkDisposition::Created
        );
        claimed.complete().unwrap();
        registry
            .claim_pending(cap.registration_id())
            .unwrap()
            .unwrap()
            .complete()
            .unwrap();
        assert!(
            registry
                .claim_pending(cap.registration_id())
                .unwrap()
                .is_none()
        );
    }
    #[test]
    fn abandoned_claim_restores_or_coalesces_the_dirty_bit() {
        let temp = tempfile::tempdir().unwrap();
        let registry = NotifyRegistry::open(temp.path()).unwrap();
        let record = registration(&home(), b"", false);
        registry.create(&record).unwrap();
        let cap = record.capability();
        registry.mark_pending(cap).unwrap();
        drop(
            registry
                .claim_pending(cap.registration_id())
                .unwrap()
                .unwrap(),
        );
        let claimed = registry
            .claim_pending(cap.registration_id())
            .unwrap()
            .unwrap();
        registry.mark_pending(cap).unwrap();
        drop(claimed);
        registry
            .claim_pending(cap.registration_id())
            .unwrap()
            .unwrap()
            .complete()
            .unwrap();
        assert!(
            registry
                .claim_pending(cap.registration_id())
                .unwrap()
                .is_none()
        );
    }
    #[test]
    fn concurrent_writers_create_one_marker_and_missing_or_wrong_capabilities_fail() {
        let temp = tempfile::tempdir().unwrap();
        let registry = NotifyRegistry::open(temp.path()).unwrap();
        let record = registration(&home(), b"", false);
        registry.create(&record).unwrap();
        let cap = record.capability().clone();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let registry = registry.clone();
                let cap = cap.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    registry.mark_pending(&cap)
                })
            })
            .collect();
        let results: Vec<_> = threads
            .into_iter()
            .map(|thread| thread.join().unwrap().unwrap())
            .collect();
        assert_eq!(
            results
                .iter()
                .filter(|&&result| result == MarkDisposition::Created)
                .count(),
            1
        );
        registry
            .claim_pending(cap.registration_id())
            .unwrap()
            .unwrap()
            .complete()
            .unwrap();
        assert_eq!(
            registry.mark_pending(&NotifyCapability::new()).unwrap_err(),
            RegistryError::NotFound
        );
        let mut wrong = serde_json::to_value(&cap).unwrap();
        wrong["nonce"] = serde_json::json!("0".repeat(64));
        let wrong: NotifyCapability = serde_json::from_value(wrong).unwrap();
        assert_eq!(
            registry.mark_pending(&wrong).unwrap_err(),
            RegistryError::Unauthorized
        );
    }
    #[test]
    fn rejects_broad_existing_directory_and_file_or_hard_link_substitutions() {
        let broad = tempfile::tempdir().unwrap();
        std::fs::create_dir(broad.path().join("notify")).unwrap();
        assert_eq!(
            NotifyRegistry::open(broad.path()).err().unwrap(),
            RegistryError::UnsafePermissions
        );
        let temp = tempfile::tempdir().unwrap();
        let registry = NotifyRegistry::open(temp.path()).unwrap();
        let record = registration(&home(), b"", false);
        registry.create(&record).unwrap();
        let path = registry_path(&temp, record.capability().registration_id());
        let before = std::fs::read(&path).unwrap();
        std::fs::hard_link(&path, temp.path().join("alias")).unwrap();
        assert_eq!(
            registry
                .get(record.capability().registration_id())
                .err()
                .unwrap(),
            RegistryError::UnsafePath
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);
        std::fs::remove_file(temp.path().join("alias")).unwrap();
        std::fs::remove_file(&path).unwrap();
        // A new ordinary file inherits the parent ACL but lacks the protected file descriptor.
        std::fs::write(&path, before).unwrap();
        assert_eq!(
            registry
                .get(record.capability().registration_id())
                .err()
                .unwrap(),
            RegistryError::UnsafePermissions
        );
        assert_eq!(
            registry.get("../private-sentinel").err().unwrap(),
            RegistryError::InvalidRecord
        );
    }
    #[test]
    fn corrupt_and_oversized_records_or_nonempty_markers_are_rejected_without_rewriting() {
        let temp = tempfile::tempdir().unwrap();
        let registry = NotifyRegistry::open(temp.path()).unwrap();
        let record = registration(&home(), b"", false);
        registry.create(&record).unwrap();
        let id = record.capability().registration_id();
        let path = registry_path(&temp, id);
        let original = std::fs::read(&path).unwrap();
        std::fs::write(&path, b"not JSON sensitive").unwrap();
        let error = registry.get(id).err().unwrap();
        assert_eq!(error.to_string(), "notify_registry_invalid_record");
        assert_eq!(std::fs::read(&path).unwrap(), b"not JSON sensitive");
        let large = std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&path)
            .unwrap();
        large.set_len(MAX_REGISTRATION_BYTES as u64 + 1).unwrap();
        drop(large);
        assert_eq!(registry.get(id).err().unwrap(), RegistryError::TooLarge);
        std::fs::write(&path, original).unwrap();
        registry.mark_pending(record.capability()).unwrap();
        let marker = temp.path().join("notify").join(format!("{id}.wake"));
        std::fs::write(&marker, b"do not treat as empty marker").unwrap();
        assert_eq!(
            registry.mark_pending(record.capability()).unwrap_err(),
            RegistryError::InvalidMarker
        );
        assert_eq!(
            registry.claim_pending(id).err().unwrap(),
            RegistryError::InvalidMarker
        );
    }
    #[test]
    fn limits_registrations_and_allocates_only_owned_atomic_records() {
        let temp = tempfile::tempdir().unwrap();
        let registry = NotifyRegistry::open(temp.path()).unwrap();
        for _ in 0..MAX_REGISTRATIONS {
            registry.create(&registration(&home(), b"", false)).unwrap();
        }
        assert_eq!(
            registry
                .create(&registration(&home(), b"", false))
                .unwrap_err(),
            RegistryError::LimitReached
        );
        assert_eq!(registry.registrations().unwrap().len(), MAX_REGISTRATIONS);
        for entry in std::fs::read_dir(temp.path().join("notify")).unwrap() {
            let name = entry.unwrap().file_name();
            assert!(!name.to_str().unwrap().starts_with(".tmp."));
        }
    }
}
