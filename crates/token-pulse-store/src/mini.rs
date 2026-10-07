//! Persistent shared mini scope and local usage read in a single SQLite snapshot.
mod sessions;
use crate::{Database, ErrorCode, Revision, StoreResult};
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};
use token_pulse_core::{
    mini::*,
    numeric::{DecimalInt, EpochMs},
    protocol::{MiniScope, SnapshotMeta, validate_request_id},
};

fn scope(payload: &serde_json::Map<String, serde_json::Value>) -> StoreResult<MiniScope> {
    let scope: MiniScope = payload
        .get("mini_scope")
        .map(|v| serde_json::from_value(v.clone()))
        .transpose()
        .map_err(|_| ErrorCode::DbCorrupt)?
        .unwrap_or_default();
    scope.validate().map_err(|_| ErrorCode::DbCorrupt)?;
    Ok(scope)
}
fn session_label(tx: &Transaction<'_>, scope: &MiniScope) -> StoreResult<Option<String>> {
    match scope {
        MiniScope::TodayAllSources {} => Ok(Some("全部来源 · 今日".into())),
        MiniScope::Session { session_key, .. } => tx.query_row("SELECT COALESCE(provider_session_id,session_key) FROM sessions WHERE session_key=?1 AND active_ledger_id IS NOT NULL", [session_key], |r| r.get(0)).optional()?.map(Some).ok_or(ErrorCode::InvalidQuery.into()),
    }
}
pub(crate) fn usage(
    tx: &Transaction<'_>,
    revision: Revision,
    at: EpochMs,
    snapshot_id: &str,
) -> StoreResult<MiniUsageSnapshot> {
    validate_request_id(snapshot_id)?;
    let (settings, payload) = crate::settings::read_stored(tx, revision.settings)?;
    let mini_scope = scope(&payload)?;
    let timezone = settings
        .preferences
        .display_timezone
        .as_deref()
        .ok_or(ErrorCode::InvalidQuery)?;
    let filter = usage_filter(&mini_scope, timezone, at)?;
    let scope_display_name = session_label(tx, &mini_scope)?;
    let usage = crate::query::totals(tx, &filter)?;
    let coverage = crate::query::coverage::coverage(tx, &filter, &usage)?;
    let pricing = crate::query::pricing::summary(tx, &filter, &price_basis(), revision.price)?;
    if pricing
        .priced_total_tokens
        .value()
        .checked_add(pricing.unpriced_total_tokens.value())
        .ok_or(ErrorCode::NumericOverflow)?
        != usage.total_tokens.value()
    {
        return Err(ErrorCode::DbCorrupt.into());
    }
    let (parser_versions, accounting_versions) =
        crate::query::dashboard::versions(tx, &filter, &filter)?;
    Ok(MiniUsageSnapshot {
        meta: SnapshotMeta {
            snapshot_id: snapshot_id.into(),
            data_revision: DecimalInt::from_nonnegative(revision.data.into())?,
            price_revision: DecimalInt::from_nonnegative(revision.price.into())?,
            generated_at_ms: at,
            parser_versions,
            accounting_versions,
            display_timezone: filter.range.timezone.clone(),
        },
        settings_revision: settings.settings_revision,
        mini_scope,
        scope_display_name,
        range: filter.range,
        usage,
        pricing,
        coverage,
    })
}
impl Database {
    pub fn mini_scope(&self) -> StoreResult<MiniScopeSnapshot> {
        self.interactive_snapshot(|tx, revision| {
            let (settings, payload) = crate::settings::read_stored(tx, revision.settings)?;
            Ok(MiniScopeSnapshot {
                settings_revision: settings.settings_revision,
                mini_scope: scope(&payload)?,
            })
        })
    }
    pub fn mini_usage(&self, at: EpochMs, snapshot_id: &str) -> StoreResult<MiniUsageSnapshot> {
        self.interactive_snapshot(|tx, revision| usage(tx, revision, at, snapshot_id))
    }
    pub fn mutate_mini_scope(
        &self,
        mutation: MiniScopeMutation,
        at: EpochMs,
    ) -> StoreResult<(MiniScopeSnapshot, bool)> {
        mutation.validate(at)?;
        self.write(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let revision: i64 = tx.query_row("SELECT settings_revision FROM app_state WHERE singleton=1", [], |r| r.get(0))?;
            let (_, mut payload) = crate::settings::read_stored(&tx, revision)?;
            if mutation.expected_settings_revision.value() != i128::from(revision) { return Err(ErrorCode::RevisionConflict.into()); }
            session_label(&tx, &mutation.mini_scope)?;
            if scope(&payload)? == mutation.mini_scope { tx.commit()?; return Ok((MiniScopeSnapshot { settings_revision: mutation.expected_settings_revision, mini_scope: mutation.mini_scope }, false)); }
            let next = revision.checked_add(1).ok_or(ErrorCode::NumericOverflow)?;
            payload.insert("mini_scope".into(), serde_json::to_value(&mutation.mini_scope)?);
            tx.execute("INSERT INTO settings(singleton,settings_version,payload_json,updated_at_ms) VALUES(1,1,?1,?2) ON CONFLICT(singleton) DO UPDATE SET payload_json=excluded.payload_json,updated_at_ms=excluded.updated_at_ms", params![serde_json::to_string(&payload)?,at.value()])?;
            tx.execute("UPDATE app_state SET settings_revision=?1 WHERE singleton=1", [next])?;
            tx.commit()?;
            Ok((MiniScopeSnapshot { settings_revision: DecimalInt::from_nonnegative(next.into())?, mini_scope: mutation.mini_scope }, true))
        })
    }
}
#[cfg(test)]
mod tests;
