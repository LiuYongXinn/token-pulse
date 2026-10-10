//! Optional Codex index metadata, independent of usage records and ledger generations.
use crate::{Database, ErrorCode, StoreResult};
use rusqlite::{TransactionBehavior, params};
use token_pulse_core::{numeric::EpochMs, protocol::validate_request_id};

#[derive(Debug, Clone)]
pub struct SessionTitle {
    pub provider_session_id: String,
    pub title: String,
    pub updated_at_ms: i64,
}
impl SessionTitle {
    pub fn validate(&self) -> StoreResult<()> {
        validate_request_id(&self.provider_session_id)?;
        EpochMs::new(self.updated_at_ms)?;
        if self.title.trim().is_empty()
            || self.title.len() > 4096
            || self.title.chars().any(char::is_control)
        {
            return Err(ErrorCode::InvalidQuery.into());
        }
        Ok(())
    }
}
impl Database {
    /// Title metadata failures are independent of usage ingestion and visible in source diagnostics.
    pub fn record_title_index_issue(
        &self,
        source_id: String,
        expected_root: String,
        issue: Option<(ErrorCode, Option<u64>)>,
        at: i64,
    ) -> StoreResult<()> {
        validate_request_id(&source_id)?;
        if issue.is_some_and(|(code, _)| {
            !matches!(
                code,
                ErrorCode::TitleIndexInvalid | ErrorCode::TitleIndexUnreadable
            )
        }) {
            return Err(ErrorCode::InvalidQuery.into());
        }
        self.write(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            if !tx.query_row("SELECT EXISTS(SELECT 1 FROM sources WHERE source_id=?1 AND enabled=1 AND root_path=?2)", params![source_id,expected_root], |r| r.get::<_,bool>(0))? { return Err(ErrorCode::PermissionDenied.into()); }
            let key = format!("title-index:{source_id}");
            let changed = match issue {
                Some((code, offset)) => {
                    let offset = offset.map(i64::try_from).transpose().map_err(|_| ErrorCode::NumericOverflow)?;
                    tx.execute("INSERT INTO diagnostics(diagnostic_id,source_id,byte_offset,code,severity,metadata_json,dedup_key,first_seen_at_ms,last_seen_at_ms) VALUES(?1,?2,?3,?4,'warning','{}',?1,?5,?5) ON CONFLICT(dedup_key) DO UPDATE SET code=excluded.code,byte_offset=excluded.byte_offset,last_seen_at_ms=excluded.last_seen_at_ms,resolved_at_ms=NULL WHERE diagnostics.code<>excluded.code OR diagnostics.byte_offset IS NOT excluded.byte_offset OR diagnostics.resolved_at_ms IS NOT NULL", params![key,source_id,offset,code.to_string(),at])? > 0
                }
                None => tx.execute("UPDATE diagnostics SET resolved_at_ms=?2 WHERE dedup_key=?1 AND resolved_at_ms IS NULL", params![key,at])? > 0,
            };
            if changed {
                let revision: i64 = tx.query_row("SELECT usage_view_revision FROM app_state WHERE singleton=1", [], |r| r.get(0))?;
                tx.execute("UPDATE app_state SET usage_view_revision=?1 WHERE singleton=1", [revision.checked_add(1).ok_or(ErrorCode::NumericOverflow)?])?;
            }
            tx.commit()?; Ok(())
        })
    }
    /// A missing/partial index never erases previously collected titles. Newer renames win.
    pub fn sync_session_titles(
        &self,
        source_id: String,
        expected_root: String,
        titles: Vec<SessionTitle>,
    ) -> StoreResult<bool> {
        validate_request_id(&source_id)?;
        for title in &titles {
            title.validate()?;
        }
        self.write(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            if !tx.query_row("SELECT EXISTS(SELECT 1 FROM sources WHERE source_id=?1 AND enabled=1 AND root_path=?2)", params![source_id,expected_root], |r| r.get::<_,bool>(0))? {
                return Err(ErrorCode::PermissionDenied.into());
            }
            let mut changed = false;
            {
                let mut upsert = tx.prepare("INSERT INTO session_titles(source_id,provider_session_id,title,updated_at_ms) VALUES(?1,?2,?3,?4) ON CONFLICT(source_id,provider_session_id) DO UPDATE SET title=excluded.title,updated_at_ms=excluded.updated_at_ms WHERE excluded.updated_at_ms>=session_titles.updated_at_ms AND (excluded.title<>session_titles.title OR excluded.updated_at_ms<>session_titles.updated_at_ms)")?;
                for title in titles {
                    changed |= upsert.execute(params![source_id,title.provider_session_id,title.title,title.updated_at_ms])? > 0;
                }
            }
            tx.commit()?;
            Ok(changed)
        })
    }
}
