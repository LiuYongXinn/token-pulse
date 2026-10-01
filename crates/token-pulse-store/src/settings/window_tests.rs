use super::*;
use token_pulse_core::{mini::MiniWindowState, placement::*};
fn at() -> EpochMs {
    EpochMs::new(123).unwrap()
}
#[test]
fn native_fields_persist_preserve_scope_and_theme_and_old_read_transaction() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(dir.path()).unwrap();
    assert_eq!(
        db.mini_window_preferences().unwrap(),
        MiniWindowPreferences::default()
    );
    db.mutate_display_timezone(
        TimezoneMutation::Initialize {
            system_timezone: "Asia/Shanghai".into(),
        },
        at(),
    )
    .unwrap();
    let (revision, changed) = db
        .update_mini_window_preferences(MiniPreferenceChange::Expanded(true), at())
        .unwrap();
    assert!(changed);
    assert_eq!(revision.as_str(), "2");
    let (revision, changed) = db
        .update_mini_window_preferences(MiniPreferenceChange::Expanded(true), at())
        .unwrap();
    assert!(!changed);
    assert_eq!(revision.as_str(), "2");
    let placement = WindowPlacement {
        monitor: Some("left".into()),
        offset_x_dip: 80.25,
        offset_y_dip: 110.5,
    };
    db.snapshot(|tx, revision| {
        let (_, old) = read_stored(tx, revision.settings)?;
        db.update_mini_window_preferences(MiniPreferenceChange::Pinned(false), at())?;
        db.update_mini_window_preferences(
            MiniPreferenceChange::Placement(placement.clone()),
            at(),
        )?;
        let (_, again) = read_stored(tx, revision.settings)?;
        assert_eq!(old, again);
        assert_eq!(old["mini_window"]["interaction"]["pinned"], true);
        Ok(())
    })
    .unwrap();
    let prefs = MiniWindowPreferences {
        interaction: MiniWindowState {
            expanded: true,
            pinned: false,
        },
        placement: Some(placement),
        opacity_percent: 100,
        passthrough: false,
    };
    assert_eq!(db.mini_window_preferences().unwrap(), prefs);
    let display = db.display_settings().unwrap();
    assert_eq!(display.settings_revision.as_str(), "4");
    assert_eq!(
        display.preferences.display_timezone.as_deref(),
        Some("Asia/Shanghai")
    );
    assert!(!display.preferences.privacy);
    db.mutate_display_theme(
        DisplayThemeMutation {
            theme: AppTheme::Light,
            expected_settings_revision: display.settings_revision,
        },
        at(),
    )
    .unwrap();
    assert_eq!(db.mini_window_preferences().unwrap(), prefs);
    assert_eq!(
        db.mini_scope().unwrap().mini_scope,
        token_pulse_core::protocol::MiniScope::TodayAllSources {}
    );
    let revisions = db.snapshot(|_, revisions| Ok(revisions)).unwrap();
    assert_eq!((revisions.data, revisions.price), (0, 0));
    drop(db);
    let reopened = Database::open(dir.path()).unwrap();
    assert_eq!(reopened.mini_window_preferences().unwrap(), prefs);
    assert_eq!(
        reopened.display_settings().unwrap().preferences.theme,
        AppTheme::Light
    );
}
#[test]
fn invalid_or_future_native_configuration_and_writer_failure_keep_original() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(dir.path()).unwrap();
    let invalid = WindowPlacement {
        monitor: None,
        offset_x_dip: f64::NAN,
        offset_y_dip: 0.0,
    };
    assert_eq!(
        db.update_mini_window_preferences(MiniPreferenceChange::Placement(invalid), at())
            .unwrap_err()
            .code,
        ErrorCode::InvalidQuery
    );
    db.write(|conn| {conn.execute_batch("CREATE TRIGGER fail_mini_revision BEFORE UPDATE OF settings_revision ON app_state BEGIN SELECT RAISE(ABORT,'synthetic failure'); END;")?;Ok(())}).unwrap();
    assert_eq!(
        db.update_mini_window_preferences(MiniPreferenceChange::Pinned(false), at())
            .unwrap_err()
            .code,
        ErrorCode::DbWriteFailed
    );
    assert_eq!(
        db.mini_window_preferences().unwrap(),
        MiniWindowPreferences::default()
    );
    assert_eq!(
        db.display_settings().unwrap().settings_revision.as_str(),
        "0"
    );
    db.write(|conn| {
        conn.execute_batch("DROP TRIGGER fail_mini_revision;")?;
        Ok(())
    })
    .unwrap();
    db.update_mini_window_preferences(MiniPreferenceChange::Expanded(true), at())
        .unwrap();
    db.write(|conn| {conn.execute("UPDATE settings SET payload_json=?1",[r#"{"mini_window":{"interaction":{"expanded":true,"pinned":true},"placement":{"monitor":null,"offset_x_dip":1000001,"offset_y_dip":0}}}"#])?;Ok(())}).unwrap();
    assert_eq!(
        db.mini_window_preferences().unwrap_err().code,
        ErrorCode::DbCorrupt
    );
    assert_eq!(
        db.update_mini_window_preferences(MiniPreferenceChange::Pinned(false), at())
            .unwrap_err()
            .code,
        ErrorCode::DbCorrupt
    );
    db.write(|conn| {
        conn.execute_batch("UPDATE settings SET settings_version=99;")?;
        Ok(())
    })
    .unwrap();
    assert_eq!(
        db.mini_window_preferences().unwrap_err().code,
        ErrorCode::UnsupportedSettingsVersion
    );
    assert_eq!(
        db.snapshot(|tx, _| Ok(tx.query_row(
            "SELECT settings_version FROM settings",
            [],
            |r| r.get::<_, i64>(0)
        )?))
        .unwrap(),
        99
    );
}
