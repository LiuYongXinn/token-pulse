use super::*;
use token_pulse_core::quota::*;
impl Database {
    /// Target and settings revision are captured in one SQLite read transaction.
    pub fn account_service_preferences(
        &self,
    ) -> StoreResult<(AccountServicePreferences, DecimalInt)> {
        self.snapshot(|tx, revision| {
            let (_, payload) = read_stored(tx, revision.settings)?;
            let preferences = payload
                .get("account_service")
                .map(|value| serde_json::from_value(value.clone()))
                .transpose()?
                .unwrap_or_default();
            Ok((
                preferences,
                DecimalInt::from_nonnegative(revision.settings.into())?,
            ))
        })
    }
    pub fn mutate_account_service(
        &self,
        preferences: AccountServicePreferences,
        expected_revision: DecimalInt,
        at: EpochMs,
    ) -> StoreResult<(AccountServiceConfigSnapshot, bool)> {
        preferences.validate()?;
        i64::try_from(expected_revision.value()).map_err(|_| ErrorCode::InvalidQuery)?;
        self.write(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let revision: i64 = tx.query_row("SELECT settings_revision FROM app_state WHERE singleton=1", [], |r| r.get(0))?;
            let (_, mut payload) = read_stored(&tx, revision)?;
            if expected_revision.value() != i128::from(revision) { return Err(ErrorCode::RevisionConflict.into()); }
            let old: AccountServicePreferences = payload.get("account_service").map(|value| serde_json::from_value(value.clone())).transpose()?.unwrap_or_default();
            let changed = old != preferences;
            let next = if changed { revision.checked_add(1).ok_or(ErrorCode::NumericOverflow)? } else { revision };
            if changed {
                payload.insert("account_service".into(), serde_json::to_value(&preferences)?);
                tx.execute("INSERT INTO settings(singleton,settings_version,payload_json,updated_at_ms) VALUES(1,?1,?2,?3) ON CONFLICT(singleton) DO UPDATE SET settings_version=excluded.settings_version,payload_json=excluded.payload_json,updated_at_ms=excluded.updated_at_ms", params![SETTINGS_VERSION,serde_json::to_string(&payload)?,at.value()])?;
                tx.execute("UPDATE app_state SET settings_revision=?1 WHERE singleton=1", [next])?;
            }
            tx.commit()?;
            Ok((AccountServiceConfigSnapshot::from_preferences(&preferences, DecimalInt::from_nonnegative(next.into())?), changed))
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn prefs(auto_connect: bool) -> AccountServicePreferences {
        #[cfg(windows)]
        let program = "C:\\Synthetic Codex\\codex.exe";
        #[cfg(not(windows))]
        let program = "/synthetic/codex";
        AccountServicePreferences {
            target: Some(AccountServiceTarget {
                executable_path: program.into(),
                home_path: None,
                executable_sha256: "a".repeat(64),
            }),
            auto_connect,
        }
    }
    #[test]
    fn configuration_cas_atomic_reads_reopen_and_other_preferences_are_independent() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(dir.path()).unwrap();
        let (default, revision) = db.account_service_preferences().unwrap();
        assert_eq!(default, AccountServicePreferences::default());
        let at = EpochMs::new(123).unwrap();
        let (saved, changed) = db
            .mutate_account_service(prefs(false), revision, at)
            .unwrap();
        assert!(changed);
        assert!(!saved.auto_connect);
        db.update_mini_window_preferences(
            token_pulse_core::placement::MiniPreferenceChange::Expanded(true),
            at,
        )
        .unwrap();
        let (_, revision) = db.account_service_preferences().unwrap();
        let (same, changed) = db
            .mutate_account_service(prefs(false), revision.clone(), at)
            .unwrap();
        assert!(!changed);
        assert_eq!(same.settings_revision, revision);
        let original = db.clone();
        db.snapshot(|tx, before| {
            let (_, old) = read_stored(tx, before.settings)?;
            assert_eq!(old["account_service"]["auto_connect"], false);
            original.mutate_account_service(prefs(true), revision.clone(), at)?;
            let (_, old_again) = read_stored(tx, before.settings)?;
            assert_eq!(old_again["account_service"]["auto_connect"], false);
            Ok(())
        })
        .unwrap();
        assert_eq!(
            db.mutate_account_service(prefs(false), revision, at)
                .unwrap_err()
                .code,
            ErrorCode::RevisionConflict
        );
        assert!(db.mini_window_preferences().unwrap().interaction.expanded);
        assert_eq!(db.snapshot(|_, r| Ok((r.data, r.price))).unwrap(), (0, 0));
        drop(db);
        let reopened = Database::open(dir.path()).unwrap();
        assert_eq!(
            reopened.account_service_preferences().unwrap().0,
            prefs(true)
        );
    }
    #[test]
    fn writer_failure_invalid_and_future_configuration_never_replace_old_target() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(dir.path()).unwrap();
        let at = EpochMs::new(123).unwrap();
        let (_, revision) = db.account_service_preferences().unwrap();
        let (saved, _) = db
            .mutate_account_service(prefs(false), revision, at)
            .unwrap();
        db.write(|conn| {conn.execute_batch("CREATE TRIGGER fail_quota BEFORE UPDATE OF settings_revision ON app_state BEGIN SELECT RAISE(ABORT,'synthetic'); END;")?; Ok(())}).unwrap();
        assert_eq!(
            db.mutate_account_service(prefs(true), saved.settings_revision.clone(), at)
                .unwrap_err()
                .code,
            ErrorCode::DbWriteFailed
        );
        let current = db.account_service_preferences().unwrap();
        assert_eq!(current.0, prefs(false));
        assert_eq!(current.1, saved.settings_revision);
        assert_eq!(
            db.mutate_account_service(
                AccountServicePreferences {
                    target: None,
                    auto_connect: true
                },
                current.1,
                at
            )
            .unwrap_err()
            .code,
            ErrorCode::InvalidQuery
        );
        db.write(|conn| {
            conn.execute_batch(
                "DROP TRIGGER fail_quota; UPDATE settings SET settings_version=999;",
            )?;
            Ok(())
        })
        .unwrap();
        assert_eq!(
            db.account_service_preferences().unwrap_err().code,
            ErrorCode::UnsupportedSettingsVersion
        );
        assert_eq!(
            db.mutate_account_service(prefs(true), saved.settings_revision, at)
                .unwrap_err()
                .code,
            ErrorCode::UnsupportedSettingsVersion
        );
        db.write(|conn| {conn.execute_batch("UPDATE settings SET settings_version=1,payload_json=json_set(payload_json,'$.account_service.credentials','SECRET');")?;Ok(())}).unwrap();
        assert_eq!(
            db.account_service_preferences().unwrap_err().code,
            ErrorCode::DbCorrupt
        );
    }
}
