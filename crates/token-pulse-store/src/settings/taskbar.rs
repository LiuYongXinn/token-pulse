use super::*;
use token_pulse_core::{mini::MiniUsageSnapshot, taskbar::*};
pub struct TaskbarInput {
    pub configuration: TaskbarPreferencesSnapshot,
    pub privacy: bool,
    pub theme: token_pulse_core::settings::AppTheme,
    pub usage: Option<MiniUsageSnapshot>,
}
fn preferences(
    payload: &serde_json::Map<String, serde_json::Value>,
) -> StoreResult<TaskbarPreferences> {
    let value = if let Some(value) = payload.get("taskbar") {
        serde_json::from_value(value.clone()).map_err(|_| ErrorCode::DbCorrupt)?
    } else {
        TaskbarPreferences {
            enabled: payload
                .get("taskbar_enabled")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            ..Default::default()
        }
    };
    value.validate().map_err(|_| ErrorCode::DbCorrupt)?;
    Ok(value)
}
impl Database {
    pub fn taskbar_preferences(&self) -> StoreResult<TaskbarPreferencesSnapshot> {
        self.snapshot(|tx, revision| {
            let (settings, payload) = read_stored(tx, revision.settings)?;
            Ok(TaskbarPreferencesSnapshot {
                preferences: preferences(&payload)?,
                settings_revision: settings.settings_revision,
            })
        })
    }
    pub fn taskbar_input(&self, at: EpochMs, snapshot_id: &str) -> StoreResult<TaskbarInput> {
        token_pulse_core::protocol::validate_request_id(snapshot_id)?;
        self.snapshot(|tx, revision| {
            let (settings, payload) = read_stored(tx, revision.settings)?;
            let preferences = preferences(&payload)?;
            let usage = if preferences.enabled {
                Some(crate::mini::usage(tx, revision, at, snapshot_id)?)
            } else {
                None
            };
            Ok(TaskbarInput {
                configuration: TaskbarPreferencesSnapshot {
                    preferences,
                    settings_revision: settings.settings_revision,
                },
                privacy: settings.preferences.privacy,
                theme: settings.preferences.theme,
                usage,
            })
        })
    }
    pub fn mutate_taskbar_preferences(
        &self,
        request: TaskbarPreferencesMutation,
        at: EpochMs,
    ) -> StoreResult<(TaskbarPreferencesSnapshot, bool)> {
        request.validate()?;
        self.write(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let revision: i64 = tx.query_row("SELECT settings_revision FROM app_state WHERE singleton=1", [], |r| r.get(0))?;
            let (_, mut payload) = read_stored(&tx, revision)?;
            if request.expected_settings_revision.value() != i128::from(revision) { return Err(ErrorCode::RevisionConflict.into()); }
            let changed = preferences(&payload)? != request.preferences;
            let next = if changed { revision.checked_add(1).ok_or(ErrorCode::NumericOverflow)? } else { revision };
            if changed {
                payload.remove("taskbar_enabled");
                payload.insert("taskbar".into(), serde_json::to_value(&request.preferences)?);
                tx.execute("INSERT INTO settings(singleton,settings_version,payload_json,updated_at_ms) VALUES(1,?1,?2,?3) ON CONFLICT(singleton) DO UPDATE SET settings_version=excluded.settings_version,payload_json=excluded.payload_json,updated_at_ms=excluded.updated_at_ms", params![SETTINGS_VERSION,serde_json::to_string(&payload)?,at.value()])?;
                tx.execute("UPDATE app_state SET settings_revision=?1 WHERE singleton=1", [next])?;
            }
            tx.commit()?;
            Ok((TaskbarPreferencesSnapshot { preferences: request.preferences, settings_revision: DecimalInt::from_nonnegative(next.into())? }, changed))
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_input_theme_privacy_and_usage_share_the_same_committed_revision() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(dir.path()).unwrap();
        let at = EpochMs::new(1000).unwrap();
        db.mutate_display_timezone(
            TimezoneMutation::Initialize {
                system_timezone: "UTC".into(),
            },
            at,
        )
        .unwrap();
        db.mutate_taskbar_preferences(
            request(
                TaskbarPreferences {
                    enabled: true,
                    ..Default::default()
                },
                "1",
            ),
            at,
        )
        .unwrap();
        db.mutate_display_theme(
            token_pulse_core::settings::DisplayThemeMutation {
                theme: token_pulse_core::settings::AppTheme::Light,
                expected_settings_revision: DecimalInt::parse("2").unwrap(),
            },
            at,
        )
        .unwrap();
        db.mutate_display_privacy(
            token_pulse_core::settings::DisplayPrivacyMutation {
                privacy: true,
                expected_settings_revision: DecimalInt::parse("3").unwrap(),
            },
            at,
        )
        .unwrap();
        let input = db.taskbar_input(at, "synthetic-native-details").unwrap();
        assert_eq!(input.theme, token_pulse_core::settings::AppTheme::Light);
        assert!(input.privacy);
        assert_eq!(input.configuration.settings_revision.as_str(), "4");
        assert_eq!(
            input.usage.unwrap().settings_revision,
            input.configuration.settings_revision
        );
    }
    fn request(preferences: TaskbarPreferences, revision: &str) -> TaskbarPreferencesMutation {
        TaskbarPreferencesMutation {
            preferences,
            expected_settings_revision: DecimalInt::parse(revision).unwrap(),
        }
    }
    #[test]
    fn taskbar_cas_noop_reopen_preserves_other_display_settings() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(dir.path()).unwrap();
        let at = EpochMs::new(1000).unwrap();
        let ledger_revision = db.snapshot(|_, r| Ok((r.data, r.price))).unwrap();
        assert!(!db.taskbar_preferences().unwrap().preferences.enabled);
        assert!(
            !db.mutate_taskbar_preferences(request(Default::default(), "0"), at)
                .unwrap()
                .1
        );
        db.mutate_display_timezone(
            TimezoneMutation::Initialize {
                system_timezone: "Asia/Shanghai".into(),
            },
            at,
        )
        .unwrap();
        let configured = TaskbarPreferences {
            enabled: true,
            ..Default::default()
        };
        let saved = db
            .mutate_taskbar_preferences(request(configured.clone(), "1"), at)
            .unwrap();
        assert!(saved.1);
        assert_eq!(saved.0.settings_revision.as_str(), "2");
        assert_eq!(
            db.mutate_taskbar_preferences(request(configured.clone(), "1"), at)
                .unwrap_err()
                .code,
            ErrorCode::RevisionConflict
        );
        assert!(
            !db.mutate_taskbar_preferences(request(configured.clone(), "2"), at)
                .unwrap()
                .1
        );
        let input = db.taskbar_input(at, "synthetic-taskbar-snapshot").unwrap();
        let usage = input.usage.unwrap();
        assert_eq!(
            usage.settings_revision,
            input.configuration.settings_revision
        );
        assert_eq!(usage.range.timezone, "Asia/Shanghai");
        assert_eq!(
            db.snapshot(|_, r| Ok((r.data, r.price))).unwrap(),
            ledger_revision
        );
        assert!(matches!(
            usage.mini_scope,
            token_pulse_core::protocol::MiniScope::TodayAllSources {}
        ));
        drop(db);
        let db = Database::open(dir.path()).unwrap();
        assert_eq!(db.taskbar_preferences().unwrap().preferences, configured);
        assert_eq!(
            db.display_settings()
                .unwrap()
                .preferences
                .display_timezone
                .as_deref(),
            Some("Asia/Shanghai")
        );
    }
    #[test]
    fn taskbar_invalid_payload_and_failed_atomic_write_do_not_publish_intent() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(dir.path()).unwrap();
        let at = EpochMs::new(1).unwrap();
        let mut preferences = TaskbarPreferences {
            enabled: true,
            ..Default::default()
        };
        preferences.display = TaskbarDisplayPreferences {
            layout: TaskbarDisplayLayout::SingleRow,
            show_tokens: false,
            show_costs: false,
            show_quota: false,
            show_weekly_reset: false,
        };
        assert_eq!(
            db.mutate_taskbar_preferences(request(preferences, "0"), at)
                .unwrap_err()
                .code,
            ErrorCode::InvalidQuery
        );
        assert!(!db.taskbar_preferences().unwrap().preferences.enabled);
        db.write(|c| { c.execute_batch("CREATE TRIGGER fail_taskbar_revision BEFORE UPDATE OF settings_revision ON app_state BEGIN SELECT RAISE(ABORT,'synthetic'); END;")?; Ok(()) }).unwrap();
        assert!(
            db.mutate_taskbar_preferences(
                request(
                    TaskbarPreferences {
                        enabled: true,
                        ..Default::default()
                    },
                    "0"
                ),
                at
            )
            .is_err()
        );
        let snapshot = db.taskbar_preferences().unwrap();
        assert!(!snapshot.preferences.enabled);
        assert_eq!(snapshot.settings_revision.as_str(), "0");
    }
    #[test]
    fn legacy_enabled_is_read_explicitly_and_corrupt_taskbar_never_defaults_off() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(dir.path()).unwrap();
        db.write(|c| {
            c.execute(
                "UPDATE settings SET payload_json=?1 WHERE singleton=1",
                [r#"{"taskbar_enabled":true,"privacy":true}"#],
            )?;
            Ok(())
        })
        .unwrap();
        let snapshot = db.taskbar_preferences().unwrap();
        assert!(snapshot.preferences.enabled);
        let mut preferences = snapshot.preferences;
        preferences.display.layout = TaskbarDisplayLayout::SingleRow;
        db.mutate_taskbar_preferences(request(preferences, "0"), EpochMs::new(2).unwrap())
            .unwrap();
        assert!(db.display_settings().unwrap().preferences.privacy);
        db.write(|c| { c.execute("UPDATE settings SET payload_json=json_set(payload_json,'$.taskbar.enabled','invalid')", [])?; Ok(()) }).unwrap();
        assert_eq!(
            db.taskbar_preferences().unwrap_err().code,
            ErrorCode::DbCorrupt
        );
    }
}
