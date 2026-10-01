use super::*;
use token_pulse_core::{
    mini_passthrough::MiniPassthroughMutation, placement::MiniWindowPreferences,
    shortcuts::RecoveryShortcut,
};
impl Database {
    /// Native recovery validation and desired mode use the very same settings transaction.
    pub fn mini_recovery_preferences(
        &self,
    ) -> StoreResult<(MiniWindowPreferences, RecoveryShortcut, DecimalInt)> {
        self.snapshot(|tx, revision| {
            let (_, payload) = read_stored(tx, revision.settings)?;
            let prefs = payload
                .get("mini_window")
                .map(|v| serde_json::from_value(v.clone()))
                .transpose()?
                .unwrap_or_default();
            let key = payload
                .get("recovery_shortcut")
                .map(|v| serde_json::from_value(v.clone()))
                .transpose()?
                .unwrap_or_default();
            Ok((
                prefs,
                key,
                DecimalInt::from_nonnegative(revision.settings.into())?,
            ))
        })
    }
    pub fn mutate_mini_passthrough(
        &self,
        request: MiniPassthroughMutation,
        at: EpochMs,
    ) -> StoreResult<(DecimalInt, bool)> {
        request.validate()?;
        self.write(move|conn| {
            let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let revision:i64=tx.query_row("SELECT settings_revision FROM app_state WHERE singleton=1",[],|r|r.get(0))?;
            let (_,mut payload)=read_stored(&tx,revision)?;
            if request.expected_settings_revision.value()!=i128::from(revision) {return Err(ErrorCode::RevisionConflict.into());}
            if request.enabled {
                let key:RecoveryShortcut=payload.get("recovery_shortcut").map(|v|serde_json::from_value(v.clone())).transpose()?.unwrap_or_default();
                if request.acknowledged_recovery.as_ref()!=Some(&key) {return Err(ErrorCode::RevisionConflict.into());}
            }
            let mut prefs:MiniWindowPreferences=payload.get("mini_window").map(|v|serde_json::from_value(v.clone())).transpose()?.unwrap_or_default();
            if prefs.passthrough==request.enabled {tx.commit()?;return Ok((DecimalInt::from_nonnegative(revision.into())?,false));}
            prefs.passthrough=request.enabled;
            let next=revision.checked_add(1).ok_or(ErrorCode::NumericOverflow)?;
            payload.insert("mini_window".into(),serde_json::to_value(prefs)?);
            tx.execute("INSERT INTO settings(singleton,settings_version,payload_json,updated_at_ms) VALUES(1,?1,?2,?3) ON CONFLICT(singleton) DO UPDATE SET settings_version=excluded.settings_version,payload_json=excluded.payload_json,updated_at_ms=excluded.updated_at_ms",params![SETTINGS_VERSION,serde_json::to_string(&payload)?,at.value()])?;
            tx.execute("UPDATE app_state SET settings_revision=?1 WHERE singleton=1",[next])?;
            tx.commit()?;Ok((DecimalInt::from_nonnegative(next.into())?,true))
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn request(enabled: bool, revision: DecimalInt) -> MiniPassthroughMutation {
        MiniPassthroughMutation {
            enabled,
            acknowledged_recovery: enabled.then(RecoveryShortcut::default),
            expected_settings_revision: revision,
        }
    }
    #[test]
    fn atomic_recovery_configuration_exact_cas_reopen_and_narrow_updates() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(dir.path()).unwrap();
        let at = EpochMs::new(123).unwrap();
        let (p, key, revision) = db.mini_recovery_preferences().unwrap();
        assert!(!p.passthrough);
        assert_eq!(key, RecoveryShortcut::default());
        let (revision, changed) = db
            .mutate_mini_passthrough(request(true, revision), at)
            .unwrap();
        assert!(changed);
        assert_eq!(revision.as_str(), "1");
        assert!(
            !db.mutate_mini_passthrough(request(true, revision.clone()), at)
                .unwrap()
                .1
        );
        assert_eq!(
            db.mutate_mini_passthrough(request(true, DecimalInt::parse("0").unwrap()), at)
                .unwrap_err()
                .code,
            ErrorCode::RevisionConflict
        );
        let mut wrong = request(true, revision);
        wrong.acknowledged_recovery.as_mut().unwrap().key = "U".into();
        assert_eq!(
            db.mutate_mini_passthrough(wrong, at).unwrap_err().code,
            ErrorCode::RevisionConflict
        );
        db.update_mini_window_preferences(
            token_pulse_core::placement::MiniPreferenceChange::Pinned(false),
            at,
        )
        .unwrap();
        assert!(db.mini_window_preferences().unwrap().passthrough);
        drop(db);
        let db = Database::open(dir.path()).unwrap();
        let (p, _, revision) = db.mini_recovery_preferences().unwrap();
        assert!(p.passthrough);
        assert!(!p.interaction.pinned);
        assert!(
            db.mutate_mini_passthrough(request(false, revision), at)
                .unwrap()
                .1
        );
        assert!(!db.mini_window_preferences().unwrap().passthrough);
    }
    #[test]
    fn invalid_configuration_and_writer_failure_do_not_overwrite_preferences() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(dir.path()).unwrap();
        let at = EpochMs::new(123).unwrap();
        db.write(|c|{c.execute_batch("CREATE TRIGGER fail_pass BEFORE UPDATE OF settings_revision ON app_state BEGIN SELECT RAISE(ABORT,'synthetic'); END;")?;Ok(())}).unwrap();
        assert_eq!(
            db.mutate_mini_passthrough(request(true, DecimalInt::parse("0").unwrap()), at)
                .unwrap_err()
                .code,
            ErrorCode::DbWriteFailed
        );
        assert!(!db.mini_window_preferences().unwrap().passthrough);
        db.write(|c| {
            c.execute_batch("DROP TRIGGER fail_pass;")?;
            Ok(())
        })
        .unwrap();
        db.update_mini_window_preferences(
            token_pulse_core::placement::MiniPreferenceChange::Pinned(false),
            at,
        )
        .unwrap();
        db.write(|c| {c.execute_batch("UPDATE settings SET payload_json=json_remove(payload_json,'$.mini_window.passthrough');")?;Ok(())}).unwrap();
        assert!(!db.mini_recovery_preferences().unwrap().0.passthrough);
        db.write(|c|{c.execute_batch("UPDATE settings SET payload_json=json_set(payload_json,'$.mini_window.passthrough',NULL);")?;Ok(())}).unwrap();
        assert_eq!(
            db.mini_recovery_preferences().unwrap_err().code,
            ErrorCode::DbCorrupt
        );
    }
}
