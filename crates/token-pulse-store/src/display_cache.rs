//! Backend-owned, bounded restoration. Every admission rechecks the committed policy.
use crate::{Database, StoreResult, database::read_usage_revision};
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};
use token_pulse_core::display_cache::*;
const FORMAT: i64 = 1;
const BUDGET: usize = 16 * 1024 * 1024;
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredDisplay {
    request: UsageDisplayRequest,
    snapshot: UsageDisplaySnapshot,
}
fn stamp(tx: &Transaction<'_>, settings: i64) -> StoreResult<DisplayCacheStamp> {
    let prefs = crate::settings::read_stored(tx, settings)?.0;
    Ok(DisplayCacheStamp {
        usage: read_usage_revision(tx)?,
        settings_revision: prefs.settings_revision,
        privacy: prefs.preferences.privacy,
    })
}
fn key(request: &UsageDisplayRequest) -> StoreResult<String> {
    request.validate()?;
    let mut value = serde_json::to_value(request)?;
    let filter = &mut value["request"]["filter"];
    for name in ["sources", "models", "projects", "sessions"] {
        if let Some(ids) = filter[name]["ids"].as_array_mut() {
            ids.sort_by_key(|id| id.to_string());
            ids.dedup();
        }
    }
    Ok(serde_json::to_string(&value)?)
}
fn matches(request: &UsageDisplayRequest, data: &UsageDisplayData) -> bool {
    let filter = match (request, data) {
        (UsageDisplayRequest::Dashboard { request }, UsageDisplayData::Dashboard { .. }) => {
            &request.filter
        }
        (UsageDisplayRequest::Groups { request }, UsageDisplayData::Groups { .. }) => {
            &request.filter
        }
        (UsageDisplayRequest::Sessions { request }, UsageDisplayData::Sessions { value })
            if value.next_cursor.is_none()
                && value.sessions.len() <= usize::from(request.page_size) =>
        {
            &request.filter
        }
        (UsageDisplayRequest::Events { request }, UsageDisplayData::Events { value })
            if value.next_cursor.is_none()
                && value.events.len() <= usize::from(request.page_size) =>
        {
            &request.filter
        }
        _ => return false,
    };
    data.meta().display_timezone == filter.range.timezone
}
impl Database {
    pub fn display_cache_stamp(&self) -> StoreResult<DisplayCacheStamp> {
        self.light_snapshot(|tx, rev| stamp(tx, rev.settings))
    }
    pub fn remember_usage_display(
        &self,
        request: &UsageDisplayRequest,
        mut data: UsageDisplayData,
        captured: DisplayCacheStamp,
    ) -> StoreResult<()> {
        let scope_key = key(request)?;
        let has_more = data.strip_cursor();
        if captured.privacy
            || !matches(request, &data)
            || data.meta().data_revision != captured.usage.data_revision
            || data.meta().price_revision != captured.usage.price_revision
        {
            return Ok(());
        }
        let result = serde_json::to_string(&StoredDisplay {
            request: request.clone(),
            snapshot: UsageDisplaySnapshot {
                data: Some(data),
                has_more,
            },
        })?;
        if result.len() > BUDGET {
            return Ok(());
        }
        self.write(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let revision: i64 = tx.query_row("SELECT settings_revision FROM app_state WHERE singleton=1", [], |row| row.get(0))?;
            if stamp(&tx, revision)? != captured { return Ok(()); }
            let stamp_json = serde_json::to_string(&captured)?;
            // Old versions cannot be restored. Prune them before accepting a complete result.
            tx.execute("DELETE FROM usage_display_cache WHERE stamp_json<>?1 OR format_version<>?2", params![stamp_json, FORMAT])?;
            let sequence: i64 = tx.query_row("SELECT COALESCE(MAX(used_sequence),0)+1 FROM usage_display_cache", [], |row| row.get(0))?;
            tx.execute("INSERT INTO usage_display_cache VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(scope_key) DO UPDATE SET format_version=excluded.format_version,stamp_json=excluded.stamp_json,result_json=excluded.result_json,byte_length=excluded.byte_length,used_sequence=excluded.used_sequence", params![scope_key, FORMAT, stamp_json, result, result.len() as i64, sequence])?;
            loop {
                let (count, bytes): (i64, i64) = tx.query_row("SELECT COUNT(*),COALESCE(SUM(byte_length),0) FROM usage_display_cache", [], |row| Ok((row.get(0)?, row.get(1)?)))?;
                if count <= 20 && bytes <= BUDGET as i64 { break; }
                tx.execute("DELETE FROM usage_display_cache WHERE scope_key=(SELECT scope_key FROM usage_display_cache ORDER BY used_sequence,scope_key LIMIT 1)", [])?;
            }
            tx.commit()?; Ok(())
        })
    }
    pub fn restore_usage_display(
        &self,
        request: &UsageDisplayRequest,
    ) -> StoreResult<UsageDisplaySnapshot> {
        let scope_key = key(request)?;
        self.light_snapshot(|tx, rev| {
            let current = stamp(tx, rev.settings)?;
            let empty = || UsageDisplaySnapshot { data: None, has_more: false };
            if current.privacy { return Ok(empty()); }
            let row: Option<(i64, String, String, i64)> = tx.query_row("SELECT format_version,stamp_json,result_json,byte_length FROM usage_display_cache WHERE scope_key=?1", [scope_key], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).optional()?;
            let Some((format, version, raw, size)) = row else { return Ok(empty()); };
            let stored_stamp = serde_json::from_str::<DisplayCacheStamp>(&version).ok();
            // Placement/theme saves can advance settings without changing this scope. Privacy
            // enable deletes all rows transactionally; every admission still requires exact settings.
            if format != FORMAT || size < 0 || size as usize != raw.len() || raw.len() > BUDGET || !stored_stamp.as_ref().is_some_and(|saved| !saved.privacy && saved.usage == current.usage && saved.settings_revision.value() <= current.settings_revision.value()) { return Ok(empty()); }
            let Some(stored) = serde_json::from_str::<StoredDisplay>(&raw).ok() else { return Ok(empty()); };
            if key(&stored.request).ok() != Some(key(request)?) { return Ok(empty()); }
            let result = stored.snapshot;
            if !result.data.as_ref().is_some_and(|data| matches(request, data) && data.meta().data_revision == current.usage.data_revision && data.meta().price_revision == current.usage.price_revision) { return Ok(empty()); }
            Ok(result)
        })
    }
}
#[cfg(test)]
mod tests;
