use super::*;
use token_pulse_core::shortcuts::*;
impl Database {
    pub fn recovery_shortcut(&self) -> StoreResult<(RecoveryShortcut, DecimalInt)> {
        self.snapshot(|tx, revision| {
            let (display, payload) = read_stored(tx, revision.settings)?;
            let shortcut = payload
                .get("recovery_shortcut")
                .map(|v| serde_json::from_value(v.clone()))
                .transpose()?
                .unwrap_or_default();
            Ok((shortcut, display.settings_revision))
        })
    }
    pub fn mutate_recovery_shortcut(
        &self,
        request: RecoveryShortcutMutation,
        at: EpochMs,
    ) -> StoreResult<(RecoveryShortcut, DecimalInt, bool)> {
        request.validate()?;
        self.write(move|conn| {
            let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let revision:i64=tx.query_row("SELECT settings_revision FROM app_state WHERE singleton=1",[],|r|r.get(0))?;
            let (_,mut payload)=read_stored(&tx,revision)?;
            if request.expected_settings_revision.value()!=i128::from(revision) {return Err(ErrorCode::RevisionConflict.into());}
            let old:RecoveryShortcut=payload.get("recovery_shortcut").map(|v|serde_json::from_value(v.clone())).transpose()?.unwrap_or_default();
            if old==request.shortcut {tx.commit()?;return Ok((old,DecimalInt::from_nonnegative(revision.into())?,false));}
            let next=revision.checked_add(1).ok_or(ErrorCode::NumericOverflow)?;
            payload.insert("recovery_shortcut".into(),serde_json::to_value(&request.shortcut)?);
            tx.execute("INSERT INTO settings(singleton,settings_version,payload_json,updated_at_ms) VALUES(1,?1,?2,?3) ON CONFLICT(singleton) DO UPDATE SET settings_version=excluded.settings_version,payload_json=excluded.payload_json,updated_at_ms=excluded.updated_at_ms",params![SETTINGS_VERSION,serde_json::to_string(&payload)?,at.value()])?;
            tx.execute("UPDATE app_state SET settings_revision=?1 WHERE singleton=1",[next])?;
            tx.commit()?;Ok((request.shortcut,DecimalInt::from_nonnegative(next.into())?,true))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn at() -> EpochMs {
        EpochMs::new(123).unwrap()
    }
    #[test]
    fn exact_cas_same_value_reopen_and_other_native_fields_are_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(dir.path()).unwrap();
        let (default, revision) = db.recovery_shortcut().unwrap();
        assert_eq!(default, RecoveryShortcut::default());
        assert_eq!(revision.as_str(), "0");
        let key = RecoveryShortcut {
            key: "F11".into(),
            ..default.clone()
        };
        let (_, revision, changed) = db
            .mutate_recovery_shortcut(
                RecoveryShortcutMutation {
                    shortcut: key.clone(),
                    expected_settings_revision: revision,
                },
                at(),
            )
            .unwrap();
        assert!(changed);
        assert_eq!(revision.as_str(), "1");
        let (_, same, changed) = db
            .mutate_recovery_shortcut(
                RecoveryShortcutMutation {
                    shortcut: key.clone(),
                    expected_settings_revision: revision.clone(),
                },
                at(),
            )
            .unwrap();
        assert!(!changed);
        assert_eq!(same, revision);
        assert_eq!(
            db.mutate_recovery_shortcut(
                RecoveryShortcutMutation {
                    shortcut: default,
                    expected_settings_revision: DecimalInt::parse("0").unwrap()
                },
                at()
            )
            .unwrap_err()
            .code,
            ErrorCode::RevisionConflict
        );
        db.update_mini_window_preferences(
            token_pulse_core::placement::MiniPreferenceChange::Pinned(false),
            at(),
        )
        .unwrap();
        let snapshot = db.display_settings().unwrap();
        db.mutate_display_theme(
            DisplayThemeMutation {
                theme: AppTheme::Light,
                expected_settings_revision: snapshot.settings_revision,
            },
            at(),
        )
        .unwrap();
        assert_eq!(db.recovery_shortcut().unwrap().0, key);
        assert!(!db.mini_window_preferences().unwrap().interaction.pinned);
        drop(db);
        let reopened = Database::open(dir.path()).unwrap();
        assert_eq!(reopened.recovery_shortcut().unwrap().0, key);
        assert_eq!(
            reopened.display_settings().unwrap().preferences.theme,
            AppTheme::Light
        );
    }
    #[test]
    fn failed_writer_invalid_key_and_corrupt_future_configuration_do_not_replace_saved_key() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(dir.path()).unwrap();
        db.write(|conn| {conn.execute_batch("CREATE TRIGGER fail_shortcut BEFORE UPDATE OF settings_revision ON app_state BEGIN SELECT RAISE(ABORT,'synthetic failure'); END;")?;Ok(())}).unwrap();
        let key = RecoveryShortcut {
            key: "U".into(),
            ..Default::default()
        };
        let request = RecoveryShortcutMutation {
            shortcut: key,
            expected_settings_revision: DecimalInt::parse("0").unwrap(),
        };
        assert_eq!(
            db.mutate_recovery_shortcut(request, at()).unwrap_err().code,
            ErrorCode::DbWriteFailed
        );
        assert_eq!(
            db.recovery_shortcut().unwrap().0,
            RecoveryShortcut::default()
        );
        assert_eq!(db.recovery_shortcut().unwrap().1.as_str(), "0");
        db.write(|conn| {
            conn.execute_batch("DROP TRIGGER fail_shortcut;")?;
            Ok(())
        })
        .unwrap();
        let bad = RecoveryShortcutMutation {
            shortcut: RecoveryShortcut {
                key: "F12".into(),
                ..Default::default()
            },
            expected_settings_revision: DecimalInt::parse("0").unwrap(),
        };
        assert_eq!(
            db.mutate_recovery_shortcut(bad, at()).unwrap_err().code,
            ErrorCode::InvalidQuery
        );
        db.mutate_display_timezone(
            TimezoneMutation::Initialize {
                system_timezone: "UTC".into(),
            },
            at(),
        )
        .unwrap();
        db.write(|conn| {
            conn.execute(
                "UPDATE settings SET payload_json=?1",
                [r#"{"recovery_shortcut":{"control":true,"alt":false,"shift":false,"key":"F12"}}"#],
            )?;
            Ok(())
        })
        .unwrap();
        assert_eq!(
            db.recovery_shortcut().unwrap_err().code,
            ErrorCode::DbCorrupt
        );
        db.write(|conn| {
            conn.execute_batch("UPDATE settings SET settings_version=99;")?;
            Ok(())
        })
        .unwrap();
        assert_eq!(
            db.recovery_shortcut().unwrap_err().code,
            ErrorCode::UnsupportedSettingsVersion
        );
    }
}
