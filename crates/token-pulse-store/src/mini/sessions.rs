//! All registered canonical sessions, including sessions without confirmed consumption.
use crate::{
    Database, ErrorCode, StoreResult,
    leases::{LeaseHandle, cursor::QueryBinding},
};
use rusqlite::{params_from_iter, types::Value};
use serde::{Deserialize, Serialize};
use token_pulse_core::{
    mini::*,
    numeric::{DecimalInt, EpochMs},
    protocol::SnapshotMeta,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Position {
    session_key: String,
    generated_at_ms: EpochMs,
}
impl Database {
    pub fn close_mini_sessions(
        &self,
        owner: &str,
        request: &MiniSessionsRequest,
    ) -> StoreResult<()> {
        request.validate()?;
        let cursor = request.cursor.as_ref().ok_or(ErrorCode::InvalidQuery)?;
        let binding = QueryBinding::new(owner, &("mini_sessions", &request.query))?;
        match self.leases().resolve_cursor::<Position>(cursor, &binding) {
            Ok((handle, _)) => match self.leases().release(&handle, &binding) {
                Ok(()) => Ok(()),
                Err(e) if e.code == ErrorCode::SnapshotExpired => Ok(()),
                Err(e) => Err(e),
            },
            Err(e) if e.code == ErrorCode::SnapshotExpired => Ok(()),
            Err(e) => Err(e),
        }
    }
    pub fn mini_sessions(
        &self,
        owner: &str,
        request: &MiniSessionsRequest,
        at: EpochMs,
    ) -> StoreResult<MiniSessionsPage> {
        request.validate()?;
        let binding = QueryBinding::new(owner, &("mini_sessions", &request.query))?;
        let (handle, position) = match &request.cursor {
            Some(cursor) => {
                let (handle, position) =
                    self.leases().resolve_cursor::<Position>(cursor, &binding)?;
                (handle, Some(position))
            }
            None => (self.leases().open(&binding)?, None),
        };
        let query = request.query.clone();
        let rows = self.leases().read(&handle, &binding, move |tx, revision| {
            let (settings, _) = crate::settings::read_stored(tx, revision.settings)?;
            let timezone = settings.preferences.display_timezone.ok_or(ErrorCode::InvalidQuery)?;
            let mut values = vec![Value::Text(query.search)];
            let continuation = if let Some(position) = &position {
                values.push(Value::Text(position.session_key.clone())); " AND s.session_key COLLATE BINARY > ?"
            } else { "" };
            values.push(Value::Integer(i64::from(query.page_size) + 1));
            let sql = format!("SELECT s.session_key,COALESCE(s.provider_session_id,s.session_key) AS label FROM sessions s WHERE NOT EXISTS(SELECT 1 FROM session_aliases a WHERE a.alias_session_key=s.session_key AND a.canonical_session_key<>s.session_key) AND (usage_search_contains(COALESCE(s.provider_session_id,s.session_key),?)){continuation} ORDER BY s.session_key COLLATE BINARY LIMIT ?");
            let mut statement = tx.prepare(&sql)?;
            let options = statement.query_map(params_from_iter(values), |row| Ok(MiniSessionOption { session_key: row.get(0)?, display_name: row.get(1)? }))?.collect::<Result<Vec<_>,_>>()?;
            let id = snapshot_id(handle);
            Ok(MiniSessionsPage {
                meta: SnapshotMeta { snapshot_id: id, data_revision: DecimalInt::from_nonnegative(revision.data.into())?, price_revision: DecimalInt::from_nonnegative(revision.price.into())?, generated_at_ms: position.map_or(at, |p| p.generated_at_ms), parser_versions: vec![], accounting_versions: vec![], display_timezone: timezone },
                options, next_cursor: None,
            })
        });
        let mut page = match rows {
            Ok(page) => page,
            Err(error) => {
                let _ = self.leases().release(&handle, &binding);
                return Err(error);
            }
        };
        let more = page.options.len() > usize::from(request.query.page_size);
        page.options.truncate(usize::from(request.query.page_size));
        if more {
            let position = Position {
                session_key: page
                    .options
                    .last()
                    .ok_or(ErrorCode::DbCorrupt)?
                    .session_key
                    .clone(),
                generated_at_ms: page.meta.generated_at_ms,
            };
            match self.leases().issue_cursor(&handle, &binding, &position) {
                Ok(cursor) => page.next_cursor = Some(cursor),
                Err(error) => {
                    let _ = self.leases().release(&handle, &binding);
                    return Err(error);
                }
            }
        } else {
            let _ = self.leases().release(&handle, &binding);
        }
        Ok(page)
    }
}
fn snapshot_id(handle: LeaseHandle) -> String {
    format!(
        "query-{}",
        handle
            .snapshot_id
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    )
}

#[cfg(test)]
mod tests;
