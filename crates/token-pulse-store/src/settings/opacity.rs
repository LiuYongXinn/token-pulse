use super::*;
use token_pulse_core::{mini_opacity::*, placement::MiniWindowPreferences};

impl Database {
    pub fn mini_opacity(&self, supported: bool) -> StoreResult<MiniOpacitySnapshot> {
        self.snapshot(|tx, revision| {
            let (_, payload) = read_stored(tx, revision.settings)?;
            let preferences: MiniWindowPreferences = payload
                .get("mini_window")
                .map(|v| serde_json::from_value(v.clone()))
                .transpose()?
                .unwrap_or_default();
            Ok(MiniOpacitySnapshot {
                opacity_percent: preferences.opacity_percent,
                supported,
                settings_revision: DecimalInt::from_nonnegative(revision.settings.into())?,
            })
        })
    }
    pub fn mutate_mini_opacity(
        &self,
        request: MiniOpacityMutation,
        supported: bool,
        at: EpochMs,
    ) -> StoreResult<(MiniOpacitySnapshot, bool)> {
        request.validate()?;
        self.write(move |conn| {
            let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let revision:i64=tx.query_row("SELECT settings_revision FROM app_state WHERE singleton=1", [], |r|r.get(0))?;
            let (_,mut payload)=read_stored(&tx,revision)?;
            if request.expected_settings_revision.value()!=i128::from(revision) { return Err(ErrorCode::RevisionConflict.into()); }
            let mut preferences:MiniWindowPreferences=payload.get("mini_window").map(|v|serde_json::from_value(v.clone())).transpose()?.unwrap_or_default();
            let changed=preferences.opacity_percent!=request.opacity_percent;
            let next=if changed {revision.checked_add(1).ok_or(ErrorCode::NumericOverflow)?} else {revision};
            if changed {
                preferences.opacity_percent=request.opacity_percent;
                payload.insert("mini_window".into(),serde_json::to_value(preferences)?);
                tx.execute("INSERT INTO settings(singleton,settings_version,payload_json,updated_at_ms) VALUES(1,?1,?2,?3) ON CONFLICT(singleton) DO UPDATE SET settings_version=excluded.settings_version,payload_json=excluded.payload_json,updated_at_ms=excluded.updated_at_ms",params![SETTINGS_VERSION,serde_json::to_string(&payload)?,at.value()])?;
                tx.execute("UPDATE app_state SET settings_revision=?1 WHERE singleton=1",[next])?;
            }
            tx.commit()?;
            Ok((MiniOpacitySnapshot {opacity_percent:request.opacity_percent,supported,settings_revision:DecimalInt::from_nonnegative(next.into())?},changed))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn mutation(percent: u8, revision: &str) -> MiniOpacityMutation {
        MiniOpacityMutation {
            opacity_percent: percent,
            expected_settings_revision: DecimalInt::parse(revision).unwrap(),
        }
    }
    #[test]
    fn exact_cas_noop_and_reopen_preserve_other_window_and_display_fields() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(dir.path()).unwrap();
        let at = EpochMs::new(123).unwrap();
        assert_eq!(db.mini_opacity(true).unwrap().opacity_percent, 100);
        assert!(
            !db.mutate_mini_opacity(mutation(100, "0"), true, at)
                .unwrap()
                .1
        );
        db.update_mini_window_preferences(
            token_pulse_core::placement::MiniPreferenceChange::Pinned(false),
            at,
        )
        .unwrap();
        let (saved, changed) = db.mutate_mini_opacity(mutation(80, "1"), true, at).unwrap();
        assert!(changed);
        assert_eq!(saved.settings_revision.as_str(), "2");
        assert_eq!(
            db.mutate_mini_opacity(mutation(80, "1"), true, at)
                .unwrap_err()
                .code,
            ErrorCode::RevisionConflict
        );
        assert!(!db.mini_window_preferences().unwrap().interaction.pinned);
        db.update_mini_window_preferences(
            token_pulse_core::placement::MiniPreferenceChange::Expanded(true),
            at,
        )
        .unwrap();
        assert_eq!(db.mini_opacity(true).unwrap().opacity_percent, 80);
        drop(db);
        let db = Database::open(dir.path()).unwrap();
        assert_eq!(db.mini_opacity(true).unwrap().opacity_percent, 80);
        assert!(db.mini_window_preferences().unwrap().interaction.expanded);
        assert_eq!(db.snapshot(|_, r| Ok((r.data, r.price))).unwrap(), (0, 0));
    }
    #[test]
    fn legacy_configuration_defaults_only_missing_opacity_and_writer_failure_rolls_back() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(dir.path()).unwrap();
        let at = EpochMs::new(123).unwrap();
        db.update_mini_window_preferences(
            token_pulse_core::placement::MiniPreferenceChange::Expanded(true),
            at,
        )
        .unwrap();
        db.write(|conn|{conn.execute_batch("UPDATE settings SET payload_json=json_remove(payload_json,'$.mini_window.opacity_percent'); CREATE TRIGGER fail_opacity BEFORE UPDATE OF settings_revision ON app_state BEGIN SELECT RAISE(ABORT,'synthetic'); END;")?;Ok(())}).unwrap();
        assert_eq!(db.mini_opacity(true).unwrap().opacity_percent, 100);
        assert_eq!(
            db.mutate_mini_opacity(mutation(75, "1"), true, at)
                .unwrap_err()
                .code,
            ErrorCode::DbWriteFailed
        );
        assert_eq!(
            db.mini_opacity(true).unwrap().settings_revision.as_str(),
            "1"
        );
        assert_eq!(db.mini_opacity(true).unwrap().opacity_percent, 100);
        db.write(|conn|{conn.execute_batch("DROP TRIGGER fail_opacity; UPDATE settings SET payload_json=json_set(payload_json,'$.mini_window.opacity_percent',69);")?;Ok(())}).unwrap();
        assert_eq!(
            db.mini_opacity(true).unwrap_err().code,
            ErrorCode::DbCorrupt
        );
    }
}
