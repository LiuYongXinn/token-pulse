use super::*;
fn setup() -> (tempfile::TempDir, Database) {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(dir.path()).unwrap();
    (dir, db)
}
fn at() -> EpochMs {
    EpochMs::new(1000).unwrap()
}
fn initialize(zone: &str) -> TimezoneMutation {
    TimezoneMutation::Initialize {
        system_timezone: zone.into(),
    }
}
fn set(zone: &str, revision: i128) -> TimezoneMutation {
    TimezoneMutation::Set {
        display_timezone: zone.into(),
        expected_settings_revision: DecimalInt::from_nonnegative(revision).unwrap(),
    }
}
#[test]
fn first_timezone_is_persisted_once_and_user_choice_survives_reopen() {
    let (dir, db) = setup();
    let initial = db.display_settings().unwrap();
    assert!(initial.preferences.display_timezone.is_none());
    assert_eq!(initial.settings_revision.as_str(), "0");
    let (first, changed) = db
        .mutate_display_timezone(initialize("Asia/Shanghai"), at())
        .unwrap();
    assert!(changed);
    assert_eq!(first.settings_revision.as_str(), "1");
    let (same, changed) = db
        .mutate_display_timezone(initialize("America/New_York"), at())
        .unwrap();
    assert!(!changed);
    assert_eq!(
        same.preferences.display_timezone.as_deref(),
        Some("Asia/Shanghai")
    );
    let (next, changed) = db.mutate_display_timezone(set("UTC", 1), at()).unwrap();
    assert!(changed);
    assert_eq!(next.settings_revision.as_str(), "2");
    let (_, changed) = db.mutate_display_timezone(set("UTC", 2), at()).unwrap();
    assert!(!changed);
    assert_eq!(
        db.snapshot(|_, revision| Ok(revision)).unwrap(),
        crate::Revision {
            data: 0,
            price: 0,
            settings: 2
        }
    );
    drop(db);
    let reopened = Database::open(dir.path()).unwrap();
    assert_eq!(
        reopened
            .display_settings()
            .unwrap()
            .preferences
            .display_timezone
            .as_deref(),
        Some("UTC")
    );
}
#[test]
fn initialization_race_converges_and_source_changes_conflict_without_overwriting_timezone() {
    let (_dir, db) = setup();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles = ["Asia/Shanghai", "America/New_York"].map(|zone| {
        let db = db.clone();
        let barrier = barrier.clone();
        std::thread::spawn(move || {
            barrier.wait();
            db.mutate_display_timezone(initialize(zone), at()).unwrap()
        })
    });
    let results = handles.map(|h| h.join().unwrap());
    assert_eq!(results.iter().filter(|r| r.1).count(), 1);
    assert_eq!(
        results[0].0.preferences.display_timezone,
        results[1].0.preferences.display_timezone
    );
    db.add_source(crate::SourceRecord {
        source_id: "source".into(),
        root_path: "synthetic".into(),
        directory_identity: None,
        kind: "local".into(),
        enabled: true,
        created_at_ms: 1,
    })
    .unwrap();
    assert_eq!(
        db.mutate_display_timezone(set("UTC", 1), at())
            .unwrap_err()
            .code,
        ErrorCode::RevisionConflict
    );
    let current = db.display_settings().unwrap();
    assert_eq!(current.settings_revision.as_str(), "2");
    assert_eq!(
        current.preferences.display_timezone,
        results[0].0.preferences.display_timezone
    );
}
#[test]
fn old_snapshot_retains_configuration_and_writer_failure_rolls_back_both_payload_and_revision() {
    let (_dir, db) = setup();
    db.mutate_display_timezone(initialize("America/New_York"), at())
        .unwrap();
    db.snapshot(|tx, revision| {
        db.mutate_display_timezone(set("UTC", 1), at())?;
        let old = read(tx, revision.settings)?;
        assert_eq!(
            old.preferences.display_timezone.as_deref(),
            Some("America/New_York")
        );
        assert_eq!(old.settings_revision.as_str(), "1");
        Ok(())
    })
    .unwrap();
    db.write(|conn| { conn.execute_batch("CREATE TRIGGER reject_settings_revision BEFORE UPDATE OF settings_revision ON app_state BEGIN SELECT RAISE(ABORT,'synthetic failure'); END;")?; Ok(()) }).unwrap();
    assert_eq!(
        db.mutate_display_timezone(set("Asia/Shanghai", 2), at())
            .unwrap_err()
            .code,
        ErrorCode::DbWriteFailed
    );
    let intact = db.display_settings().unwrap();
    assert_eq!(intact.preferences.display_timezone.as_deref(), Some("UTC"));
    assert_eq!(intact.settings_revision.as_str(), "2");
}
#[test]
fn invalid_future_and_corrupt_settings_are_not_replaced_with_defaults() {
    let (_dir, db) = setup();
    assert_eq!(
        db.mutate_display_timezone(initialize("Unknown/Zone"), at())
            .unwrap_err()
            .code,
        ErrorCode::InvalidQuery
    );
    assert!(
        db.display_settings()
            .unwrap()
            .preferences
            .display_timezone
            .is_none()
    );
    db.write(|conn| {
        conn.execute(
            "UPDATE settings SET settings_version=99,payload_json='{\"display_timezone\":\"UTC\"}',updated_at_ms=123 WHERE singleton=1",
            [],
        )?;
        Ok(())
    })
    .unwrap();
    assert_eq!(
        db.display_settings().unwrap_err().code,
        ErrorCode::UnsupportedSettingsVersion
    );
    assert_eq!(
        db.mutate_display_timezone(initialize("Asia/Shanghai"), at())
            .unwrap_err()
            .code,
        ErrorCode::UnsupportedSettingsVersion
    );
    db.write(|conn| { conn.execute("UPDATE settings SET settings_version=1,payload_json='{\"display_timezone\":\"Unknown/Zone\"}'", [])?; Ok(()) }).unwrap();
    assert_eq!(
        db.display_settings().unwrap_err().code,
        ErrorCode::DbCorrupt
    );
    assert_eq!(
        db.mutate_display_timezone(set("UTC", 0), at())
            .unwrap_err()
            .code,
        ErrorCode::DbCorrupt
    );
    assert_eq!(db.snapshot(|_, revision| Ok(revision.settings)).unwrap(), 0);
}
#[test]
fn numeric_revision_overflow_leaves_saved_configuration_untouched() {
    let (_dir, db) = setup();
    db.mutate_display_timezone(initialize("UTC"), at()).unwrap();
    db.write(|conn| {
        conn.execute("UPDATE app_state SET settings_revision=?1", [i64::MAX])?;
        Ok(())
    })
    .unwrap();
    assert_eq!(
        db.mutate_display_timezone(set("Asia/Shanghai", i64::MAX.into()), at())
            .unwrap_err()
            .code,
        ErrorCode::NumericOverflow
    );
    assert_eq!(
        db.display_settings()
            .unwrap()
            .preferences
            .display_timezone
            .as_deref(),
        Some("UTC")
    );
    assert_eq!(
        db.mutate_display_timezone(set("Asia/Shanghai", i128::from(i64::MAX) + 1), at())
            .unwrap_err()
            .code,
        ErrorCode::InvalidQuery
    );
}

#[test]
fn timezone_edit_preserves_existing_theme_privacy_scope_and_native_preferences() {
    let (_dir, db) = setup();
    let original = serde_json::json!({"theme":"light","privacy":true,"mini_scope":{"kind":"session","session_key":"kept-session","start":{"kind":"fixed","start_ms":0}},"taskbar_enabled":true,"startup_enabled":true});
    let encoded = original.to_string();
    db.write(move |conn| {
        conn.execute("UPDATE settings SET payload_json=?1", [encoded])?;
        Ok(())
    })
    .unwrap();
    db.mutate_display_timezone(initialize("Asia/Shanghai"), at())
        .unwrap();
    db.mutate_display_timezone(set("UTC", 1), at()).unwrap();
    let mut stored: serde_json::Value = db
        .snapshot(|tx, _| {
            let raw: String =
                tx.query_row("SELECT payload_json FROM settings", [], |r| r.get(0))?;
            Ok(serde_json::from_str(&raw)?)
        })
        .unwrap();
    assert_eq!(stored["display_timezone"], "UTC");
    stored.as_object_mut().unwrap().remove("display_timezone");
    assert_eq!(stored, original);
    let public = serde_json::to_value(db.display_settings().unwrap()).unwrap();
    assert!(public["preferences"].get("mini_scope").is_none());
}
