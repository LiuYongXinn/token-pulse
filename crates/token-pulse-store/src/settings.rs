//! A single Writer publishes configuration and its revision atomically.
mod opacity;
mod passthrough;
mod quota;
mod shortcuts;
use crate::{Database, ErrorCode, StoreResult};
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};
use token_pulse_core::{
    numeric::{DecimalInt, EpochMs},
    settings::*,
};
pub(crate) fn read_stored(
    tx: &Transaction<'_>,
    revision: i64,
) -> StoreResult<(
    DisplaySettingsSnapshot,
    serde_json::Map<String, serde_json::Value>,
)> {
    let row: Option<(i64, String)> = tx
        .query_row(
            "SELECT settings_version,payload_json FROM settings WHERE singleton=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let payload = match row {
        None => serde_json::Map::new(),
        Some((version, raw)) => {
            if version != i64::from(SETTINGS_VERSION) {
                return Err(ErrorCode::UnsupportedSettingsVersion.into());
            }
            let value: serde_json::Value =
                serde_json::from_str(&raw).map_err(|_| ErrorCode::DbCorrupt)?;
            let value = value.as_object().ok_or(ErrorCode::DbCorrupt)?.clone();
            for (key, field) in &value {
                let valid =
                    match key.as_str() {
                        "display_timezone" => {
                            field.is_null()
                                || field.as_str().is_some_and(|z| validate_timezone(z).is_ok())
                        }
                        "theme" => field
                            .as_str()
                            .is_some_and(|s| matches!(s, "dark" | "light" | "system")),
                        "privacy" | "taskbar_enabled" | "startup_enabled" => field.is_boolean(),
                        "recovery_shortcut" => serde_json::from_value::<
                            token_pulse_core::shortcuts::RecoveryShortcut,
                        >(field.clone())
                        .is_ok_and(|key| key.virtual_key().is_ok()),
                        "mini_window" => serde_json::from_value::<
                            token_pulse_core::placement::MiniWindowPreferences,
                        >(field.clone())
                        .is_ok_and(|value| value.validate().is_ok()),
                        "account_service" => serde_json::from_value::<
                            token_pulse_core::quota::AccountServicePreferences,
                        >(field.clone())
                        .is_ok_and(|value| value.validate().is_ok()),
                        "mini_scope" => serde_json::from_value::<
                            token_pulse_core::protocol::MiniScope,
                        >(field.clone())
                        .is_ok_and(|scope| match scope {
                            token_pulse_core::protocol::MiniScope::TodayAllSources {} => true,
                            token_pulse_core::protocol::MiniScope::Session {
                                session_key, ..
                            } => token_pulse_core::protocol::DimensionSelection::Ids {
                                ids: vec![session_key],
                                include_unknown: false,
                            }
                            .validate()
                            .is_ok(),
                        }),
                        _ => false,
                    };
                if !valid {
                    return Err(ErrorCode::DbCorrupt.into());
                }
            }
            value
        }
    };
    let preferences = DisplayPreferences {
        theme: payload
            .get("theme")
            .map(|value| serde_json::from_value(value.clone()))
            .transpose()
            .map_err(|_| ErrorCode::DbCorrupt)?
            .unwrap_or_default(),
        privacy: payload
            .get("privacy")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        display_timezone: payload
            .get("display_timezone")
            .and_then(|v| v.as_str())
            .map(str::to_owned),
    };
    Ok((
        DisplaySettingsSnapshot {
            settings_version: SETTINGS_VERSION,
            settings_revision: DecimalInt::from_nonnegative(revision.into())?,
            preferences,
        },
        payload,
    ))
}
fn read(tx: &Transaction<'_>, revision: i64) -> StoreResult<DisplaySettingsSnapshot> {
    Ok(read_stored(tx, revision)?.0)
}
impl Database {
    pub fn mini_window_preferences(
        &self,
    ) -> StoreResult<token_pulse_core::placement::MiniWindowPreferences> {
        self.snapshot(|tx, revision| {
            let (_, payload) = read_stored(tx, revision.settings)?;
            payload
                .get("mini_window")
                .map(|value| {
                    serde_json::from_value(value.clone()).map_err(|_| ErrorCode::DbCorrupt.into())
                })
                .unwrap_or_else(|| Ok(Default::default()))
        })
    }
    pub fn update_mini_window_preferences(
        &self,
        change: token_pulse_core::placement::MiniPreferenceChange,
        at: EpochMs,
    ) -> StoreResult<(DecimalInt, bool)> {
        use token_pulse_core::placement::*;
        if let MiniPreferenceChange::Placement(value) = &change {
            value.validate()?;
        }
        self.write(move|conn| {
            let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let revision:i64=tx.query_row("SELECT settings_revision FROM app_state WHERE singleton=1",[],|r|r.get(0))?;
            let (_,mut payload)=read_stored(&tx,revision)?;
            let mut value:MiniWindowPreferences=payload.get("mini_window").map(|v|serde_json::from_value(v.clone())).transpose()?.unwrap_or_default();
            let original=value.clone();
            match change {MiniPreferenceChange::Passthrough(enabled)=>value.passthrough=enabled,MiniPreferenceChange::Expanded(expanded)=>value.interaction.expanded=expanded,MiniPreferenceChange::Pinned(pinned)=>value.interaction.pinned=pinned,MiniPreferenceChange::Placement(placement)=>value.placement=Some(placement)}
            if value==original {tx.commit()?;return Ok((DecimalInt::from_nonnegative(revision.into())?,false));}
            let next=revision.checked_add(1).ok_or(ErrorCode::NumericOverflow)?;
            payload.insert("mini_window".into(),serde_json::to_value(value)?);
            tx.execute("INSERT INTO settings(singleton,settings_version,payload_json,updated_at_ms) VALUES(1,?1,?2,?3) ON CONFLICT(singleton) DO UPDATE SET settings_version=excluded.settings_version,payload_json=excluded.payload_json,updated_at_ms=excluded.updated_at_ms",params![SETTINGS_VERSION,serde_json::to_string(&payload)?,at.value()])?;
            tx.execute("UPDATE app_state SET settings_revision=?1 WHERE singleton=1",[next])?;
            tx.commit()?;Ok((DecimalInt::from_nonnegative(next.into())?,true))
        })
    }
    pub fn mutate_display_theme(
        &self,
        mutation: DisplayThemeMutation,
        at: EpochMs,
    ) -> StoreResult<(DisplaySettingsSnapshot, bool)> {
        mutation.validate()?;
        self.write(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let revision: i64 = tx.query_row("SELECT settings_revision FROM app_state WHERE singleton=1", [], |r| r.get(0))?;
            let (mut snapshot, mut payload) = read_stored(&tx, revision)?;
            if mutation.expected_settings_revision.value() != i128::from(revision) { return Err(ErrorCode::RevisionConflict.into()); }
            if snapshot.preferences.theme == mutation.theme { tx.commit()?; return Ok((snapshot, false)); }
            let next_revision = revision.checked_add(1).ok_or(ErrorCode::NumericOverflow)?;
            payload.insert("theme".into(), serde_json::to_value(mutation.theme)?);
            snapshot.preferences.theme = mutation.theme;
            tx.execute("INSERT INTO settings(singleton,settings_version,payload_json,updated_at_ms) VALUES(1,?1,?2,?3) ON CONFLICT(singleton) DO UPDATE SET settings_version=excluded.settings_version,payload_json=excluded.payload_json,updated_at_ms=excluded.updated_at_ms", params![SETTINGS_VERSION,serde_json::to_string(&payload)?,at.value()])?;
            tx.execute("UPDATE app_state SET settings_revision=?1 WHERE singleton=1", [next_revision])?;
            snapshot.settings_revision = DecimalInt::from_nonnegative(next_revision.into())?;
            tx.commit()?; Ok((snapshot, true))
        })
    }
    pub fn display_settings(&self) -> StoreResult<DisplaySettingsSnapshot> {
        self.snapshot(|tx, revision| read(tx, revision.settings))
    }
    pub fn mutate_display_privacy(
        &self,
        mutation: DisplayPrivacyMutation,
        at: EpochMs,
    ) -> StoreResult<(DisplaySettingsSnapshot, bool)> {
        mutation.validate()?;
        self.write(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let revision: i64 = tx.query_row("SELECT settings_revision FROM app_state WHERE singleton=1", [], |r| r.get(0))?;
            let (mut snapshot, mut payload) = read_stored(&tx, revision)?;
            if mutation.expected_settings_revision.value() != i128::from(revision) { return Err(ErrorCode::RevisionConflict.into()); }
            if snapshot.preferences.privacy == mutation.privacy { tx.commit()?; return Ok((snapshot, false)); }
            let next_revision = revision.checked_add(1).ok_or(ErrorCode::NumericOverflow)?;
            payload.insert("privacy".into(), mutation.privacy.into());
            snapshot.preferences.privacy = mutation.privacy;
            tx.execute("INSERT INTO settings(singleton,settings_version,payload_json,updated_at_ms) VALUES(1,?1,?2,?3) ON CONFLICT(singleton) DO UPDATE SET settings_version=excluded.settings_version,payload_json=excluded.payload_json,updated_at_ms=excluded.updated_at_ms", params![SETTINGS_VERSION,serde_json::to_string(&payload)?,at.value()])?;
            tx.execute("UPDATE app_state SET settings_revision=?1 WHERE singleton=1", [next_revision])?;
            snapshot.settings_revision = DecimalInt::from_nonnegative(next_revision.into())?;
            tx.commit()?; Ok((snapshot, true))
        })
    }
    pub fn mutate_display_timezone(
        &self,
        mutation: TimezoneMutation,
        at: EpochMs,
    ) -> StoreResult<(DisplaySettingsSnapshot, bool)> {
        mutation.validate()?;
        self.write(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let revision: i64 = tx.query_row("SELECT settings_revision FROM app_state WHERE singleton=1", [], |r| r.get(0))?;
            let (mut snapshot, mut payload) = read_stored(&tx, revision)?;
            let next = match mutation {
                TimezoneMutation::Initialize { system_timezone } => {
                    if snapshot.preferences.display_timezone.is_some() { tx.commit()?; return Ok((snapshot, false)); }
                    system_timezone
                }
                TimezoneMutation::Set { display_timezone, expected_settings_revision } => {
                    if expected_settings_revision.value() != i128::from(revision) { return Err(ErrorCode::RevisionConflict.into()); }
                    display_timezone
                }
            };
            if snapshot.preferences.display_timezone.as_ref() == Some(&next) { tx.commit()?; return Ok((snapshot, false)); }
            let next_revision = revision.checked_add(1).ok_or(ErrorCode::NumericOverflow)?;
            payload.insert("display_timezone".into(), next.clone().into());
            snapshot.preferences.display_timezone = Some(next);
            tx.execute("INSERT INTO settings(singleton,settings_version,payload_json,updated_at_ms) VALUES(1,?1,?2,?3) ON CONFLICT(singleton) DO UPDATE SET settings_version=excluded.settings_version,payload_json=excluded.payload_json,updated_at_ms=excluded.updated_at_ms", params![SETTINGS_VERSION,serde_json::to_string(&payload)?,at.value()])?;
            tx.execute("UPDATE app_state SET settings_revision=?1 WHERE singleton=1", [next_revision])?;
            snapshot.settings_revision = DecimalInt::from_nonnegative(next_revision.into())?;
            tx.commit()?; Ok((snapshot, true))
        })
    }
}
#[cfg(test)]
mod tests;
#[cfg(test)]
mod window_tests;
