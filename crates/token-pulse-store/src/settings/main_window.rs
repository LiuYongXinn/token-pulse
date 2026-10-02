//! Native-only main placement updates preserve the latest unrelated settings payload.
use super::{read_stored, *};
use token_pulse_core::placement::{MainWindowPreferences, WindowPlacement};
impl Database {
    pub fn main_window_preferences(&self) -> StoreResult<MainWindowPreferences> {
        self.snapshot(|tx, revision| {
            let (_, payload) = read_stored(tx, revision.settings)?;
            payload
                .get("main_window")
                .map(|value| {
                    serde_json::from_value(value.clone()).map_err(|_| ErrorCode::DbCorrupt.into())
                })
                .unwrap_or_else(|| Ok(Default::default()))
        })
    }
    pub fn update_main_window_placement(
        &self,
        placement: WindowPlacement,
        at: EpochMs,
    ) -> StoreResult<(DecimalInt, bool)> {
        placement.validate()?;
        self.write(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let revision: i64 = tx.query_row("SELECT settings_revision FROM app_state WHERE singleton=1", [], |r| r.get(0))?;
            let (_, mut payload) = read_stored(&tx, revision)?;
            let original: MainWindowPreferences = payload.get("main_window").map(|v| serde_json::from_value(v.clone())).transpose()?.unwrap_or_default();
            let updated = MainWindowPreferences { placement: Some(placement) };
            if original == updated {
                tx.commit()?;
                return Ok((DecimalInt::from_nonnegative(revision.into())?, false));
            }
            let next = revision.checked_add(1).ok_or(ErrorCode::NumericOverflow)?;
            payload.insert("main_window".into(), serde_json::to_value(updated)?);
            tx.execute("INSERT INTO settings(singleton,settings_version,payload_json,updated_at_ms) VALUES(1,?1,?2,?3) ON CONFLICT(singleton) DO UPDATE SET settings_version=excluded.settings_version,payload_json=excluded.payload_json,updated_at_ms=excluded.updated_at_ms", params![SETTINGS_VERSION, serde_json::to_string(&payload)?, at.value()])?;
            tx.execute("UPDATE app_state SET settings_revision=?1 WHERE singleton=1", [next])?;
            tx.commit()?;
            Ok((DecimalInt::from_nonnegative(next.into())?, true))
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use token_pulse_core::placement::MiniPreferenceChange;
    fn position() -> WindowPlacement {
        WindowPlacement {
            monitor: Some("synthetic-left-monitor".into()),
            offset_x_dip: 123.5,
            offset_y_dip: 75.25,
        }
    }
    #[test]
    fn main_location_survives_reopen_preserves_mini_display_and_exact_revision() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(directory.path()).unwrap();
        assert_eq!(db.main_window_preferences().unwrap().placement, None);
        db.update_mini_window_preferences(
            MiniPreferenceChange::Pinned(false),
            EpochMs::new(1).unwrap(),
        )
        .unwrap();
        let before = db.display_settings().unwrap();
        let (revision, changed) = db
            .update_main_window_placement(position(), EpochMs::new(2).unwrap())
            .unwrap();
        assert!(changed);
        assert_eq!(revision.value(), before.settings_revision.value() + 1);
        assert_eq!(
            serde_json::to_value(db.display_settings().unwrap().preferences).unwrap(),
            serde_json::to_value(before.preferences).unwrap()
        );
        assert!(!db.mini_window_preferences().unwrap().interaction.pinned);
        assert_eq!(db.mini_window_preferences().unwrap().placement, None);
        assert_eq!(
            db.update_main_window_placement(position(), EpochMs::new(3).unwrap())
                .unwrap(),
            (revision.clone(), false)
        );
        drop(db);
        let reopened = Database::open(directory.path()).unwrap();
        assert_eq!(
            reopened.main_window_preferences().unwrap().placement,
            Some(position())
        );
        assert_eq!(
            reopened.display_settings().unwrap().settings_revision,
            revision
        );
    }
    #[test]
    fn failed_revision_commit_retains_old_main_and_other_preferences() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(directory.path()).unwrap();
        db.update_main_window_placement(position(), EpochMs::new(1).unwrap())
            .unwrap();
        let before = db.display_settings().unwrap();
        db.write(|conn| { conn.execute_batch("CREATE TRIGGER reject_main_revision BEFORE UPDATE OF settings_revision ON app_state BEGIN SELECT RAISE(ABORT,'synthetic failure'); END;")?; Ok(()) }).unwrap();
        let changed = WindowPlacement {
            offset_x_dip: 90.0,
            ..position()
        };
        assert_eq!(
            db.update_main_window_placement(changed, EpochMs::new(2).unwrap())
                .unwrap_err()
                .code,
            ErrorCode::DbWriteFailed
        );
        assert_eq!(
            db.main_window_preferences().unwrap().placement,
            Some(position())
        );
        assert_eq!(
            serde_json::to_value(db.display_settings().unwrap()).unwrap(),
            serde_json::to_value(before).unwrap()
        );
        assert_eq!(
            db.update_main_window_placement(
                WindowPlacement {
                    offset_x_dip: f64::NAN,
                    ..position()
                },
                EpochMs::new(2).unwrap()
            )
            .unwrap_err()
            .code,
            ErrorCode::InvalidQuery
        );
    }
    #[test]
    fn malformed_or_future_payload_is_rejected_without_replacing_it() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(directory.path()).unwrap();
        for (version, raw, code) in [
            (
                1,
                r#"{"main_window":{"placement":{"monitor":null,"offset_x_dip":0,"offset_y_dip":0,"path":"unexpected"}}}"#,
                ErrorCode::DbCorrupt,
            ),
            (
                2,
                r#"{"main_window":{"placement":null}}"#,
                ErrorCode::UnsupportedSettingsVersion,
            ),
        ] {
            let raw = raw.to_owned();
            let expected = raw.clone();
            db.write(move |conn| { conn.execute("INSERT INTO settings(singleton,settings_version,payload_json,updated_at_ms) VALUES(1,?1,?2,0) ON CONFLICT(singleton) DO UPDATE SET settings_version=excluded.settings_version,payload_json=excluded.payload_json", params![version,raw])?; Ok(()) }).unwrap();
            assert_eq!(db.main_window_preferences().unwrap_err().code, code);
            assert_eq!(
                db.update_main_window_placement(position(), EpochMs::new(3).unwrap())
                    .unwrap_err()
                    .code,
                code
            );
            assert_eq!(
                db.snapshot(|tx, _| Ok(tx.query_row(
                    "SELECT payload_json FROM settings WHERE singleton=1",
                    [],
                    |r| r.get::<_, String>(0)
                )?))
                .unwrap(),
                expected
            );
        }
    }
}
