use crate::{Database, ErrorCode, StoreResult};
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use token_pulse_core::domain::{ACCOUNTING_VERSION, PARSER_VERSION, ReaderContext};

#[derive(Debug, Clone)]
pub struct SessionRegistration {
    pub session_key: String,
    pub provider_session_id: Option<String>,
    pub parent_key: Option<String>,
    pub parent_provider_id: Option<String>,
    pub created_at_ms: Option<i64>,
    pub ledger_id: String,
    pub registered_at_ms: i64,
}
#[derive(Debug, Clone)]
pub struct FileRegistration {
    pub file_id: String,
    pub source_id: String,
    pub canonical_path: String,
    pub file_identity: Option<String>,
    pub file_generation_id: String,
    pub observed_size: i64,
    pub created_at_ms: i64,
    pub reader_context: ReaderContext,
}
impl Database {
    pub fn ensure_session(&self, session: SessionRegistration) -> StoreResult<String> {
        if session.session_key.is_empty() || session.ledger_id.is_empty() {
            return Err(ErrorCode::InvalidQuery.into());
        }
        self.write(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let existing = tx.query_row("SELECT provider_session_id,active_ledger_id FROM sessions WHERE session_key=?1", [&session.session_key], |r| Ok((r.get::<_,Option<String>>(0)?, r.get::<_,Option<String>>(1)?))).optional()?;
            if let Some((provider_id, ledger)) = existing {
                if provider_id != session.provider_session_id { return Err(ErrorCode::CheckpointConflict.into()); }
                return ledger.ok_or(ErrorCode::CheckpointConflict.into());
            }
            tx.execute("INSERT INTO sessions(session_key,provider,provider_session_id,identity_status,parent_key,parent_provider_id,created_at_ms) VALUES(?1,'codex',?2,'confirmed',?3,?4,?5)", params![session.session_key,session.provider_session_id,session.parent_key,session.parent_provider_id,session.created_at_ms])?;
            tx.execute("INSERT INTO ledger_generations(ledger_id,session_key,state,parser_version,accounting_version,base_data_revision,created_at_ms,activated_at_ms,input_manifest_json) VALUES(?1,?2,'active',?3,?4,(SELECT data_revision FROM app_state),?5,?5,'{}')", params![session.ledger_id,session.session_key,PARSER_VERSION,ACCOUNTING_VERSION,session.registered_at_ms])?;
            tx.execute("UPDATE sessions SET active_ledger_id=?1 WHERE session_key=?2", params![session.ledger_id,session.session_key])?;
            tx.commit()?; Ok(session.ledger_id)
        })
    }
    pub fn register_file(&self, file: FileRegistration) -> StoreResult<()> {
        if file.observed_size < 0 || file.file_id.is_empty() || file.file_generation_id.is_empty() {
            return Err(ErrorCode::InvalidQuery.into());
        }
        self.write(move |conn| {
            let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            tx.execute("INSERT INTO source_files(file_id,source_id,canonical_path,file_identity,status,last_seen_at_ms) VALUES(?1,?2,?3,?4,'present',?5)", params![file.file_id,file.source_id,file.canonical_path,file.file_identity,file.created_at_ms])?;
            tx.execute("INSERT INTO file_generations(file_generation_id,file_id,state,identity_json,observed_size,anchor_json,reader_context_json,parser_version,created_at_ms) VALUES(?1,?2,'current',?3,?4,'[]',?5,?6,?7)", params![file.file_generation_id,file.file_id,serde_json::to_string(&file.file_identity)?,file.observed_size,serde_json::to_string(&file.reader_context)?,PARSER_VERSION,file.created_at_ms])?;
            tx.execute("UPDATE source_files SET current_generation_id=?1 WHERE file_id=?2", params![file.file_generation_id,file.file_id])?;
            tx.commit()?; Ok(())
        })
    }
}
