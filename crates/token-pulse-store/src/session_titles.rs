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
