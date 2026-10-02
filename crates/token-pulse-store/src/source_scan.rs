//! Directory enumeration plus explicit current-generation read confirmations; never source writes.
use crate::{Database, ErrorCode, StoreResult};
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};
use token_pulse_core::{numeric::EpochMs, protocol::validate_request_id};

#[derive(Debug, Clone)]
pub struct ScanHandle {
    pub source_id: String,
    pub revision: i64,
}

// Reuse exactly this predicate for publication and snapshot coverage. A ready flag alone is not proof.
pub(crate) const BAD_ENTRY: &str = "e.file_generation_id IS NULL OR g.file_generation_id IS NULL OR g.state<>'current' OR f.current_generation_id<>e.file_generation_id OR f.source_id<>e.source_id OR f.canonical_path<>e.canonical_path OR g.checkpoint_revision<>e.checkpoint_revision OR g.observed_size<>e.upper_bound OR g.committed_offset<>e.upper_bound";
fn text(value: &str) -> StoreResult<()> {
    if value.is_empty() || value.len() > 32768 || value.chars().any(char::is_control) {
        return Err(ErrorCode::InvalidQuery.into());
    }
    Ok(())
}
fn fresh(tx: &Transaction<'_>, handle: &ScanHandle) -> StoreResult<()> {
    let (revision,valid): (i64,bool)=tx.query_row("SELECT ss.scan_revision,s.enabled=1 AND s.root_path=ss.source_root FROM source_scan_state ss JOIN sources s USING(source_id) WHERE ss.source_id=?1",[&handle.source_id],|r|Ok((r.get(0)?,r.get(1)?))).optional()?.ok_or(ErrorCode::CheckpointConflict)?;
    if revision != handle.revision {
        return Err(ErrorCode::CheckpointConflict.into());
    }
    if !valid {
        return Err(ErrorCode::PermissionDenied.into());
    }
    Ok(())
}
fn refresh(tx: &Transaction<'_>, handle: &ScanHandle, at: i64) -> StoreResult<()> {
    let ready:bool=tx.query_row(&format!("SELECT discovery_complete=1 AND invalidated=0 AND issue_code IS NULL AND NOT EXISTS(SELECT 1 FROM source_scan_files e LEFT JOIN source_files f ON f.file_id=e.file_id LEFT JOIN file_generations g ON g.file_generation_id=e.file_generation_id WHERE e.source_id=ss.source_id AND e.scan_revision=ss.scan_revision AND ({BAD_ENTRY})) FROM source_scan_state ss WHERE ss.source_id=?1 AND ss.scan_revision=?2"),params![handle.source_id,handle.revision],|r|r.get(0))?;
    tx.execute("UPDATE source_scan_state SET state=CASE WHEN ?1 THEN 'ready' WHEN discovery_complete=0 THEN 'scanning' ELSE 'incomplete' END,completed_at_ms=CASE WHEN ?1 THEN ?2 ELSE NULL END WHERE source_id=?3 AND scan_revision=?4",params![ready,at,handle.source_id,handle.revision])?;
    if ready {
        // A complete directory proof can identify missing old files; consumption facts stay intact.
        tx.execute("UPDATE source_files SET status=CASE WHEN EXISTS(SELECT 1 FROM source_scan_files e WHERE e.source_id=?1 AND e.scan_revision=?2 AND e.file_id=source_files.file_id) THEN 'present' ELSE 'missing' END WHERE source_id=?1",params![handle.source_id,handle.revision])?;
    }
    Ok(())
}
impl Database {
    pub fn begin_source_scan(
        &self,
        source_id: String,
        expected_root: String,
        at_ms: i64,
    ) -> StoreResult<ScanHandle> {
        validate_request_id(&source_id)?;
        text(&expected_root)?;
        EpochMs::new(at_ms)?;
        self.write(move|conn| {
            let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            if !tx.query_row("SELECT EXISTS(SELECT 1 FROM sources WHERE source_id=?1 AND enabled=1 AND root_path=?2)",params![source_id,expected_root],|r|r.get::<_,bool>(0))? {return Err(ErrorCode::PermissionDenied.into())}
            let prior:Option<i64>=tx.query_row("SELECT scan_revision FROM source_scan_state WHERE source_id=?1",[&source_id],|r|r.get(0)).optional()?;
            let revision=prior.unwrap_or(0).checked_add(1).ok_or(ErrorCode::NumericOverflow)?;
            tx.execute("DELETE FROM source_scan_files WHERE source_id=?1",[&source_id])?;
            tx.execute("INSERT INTO source_scan_state(source_id,scan_revision,source_root,state,started_at_ms) VALUES(?1,?2,?3,'scanning',?4) ON CONFLICT(source_id) DO UPDATE SET scan_revision=excluded.scan_revision,source_root=excluded.source_root,state='scanning',discovery_complete=0,invalidated=0,issue_code=NULL,started_at_ms=excluded.started_at_ms,completed_at_ms=NULL",params![source_id,revision,expected_root,at_ms])?;
            tx.commit()?;Ok(ScanHandle {source_id,revision})
        })
    }
    pub fn record_source_scan_files(
        &self,
        handle: ScanHandle,
        files: Vec<(String, i64)>,
    ) -> StoreResult<()> {
        if files.len() > 128 {
            return Err(ErrorCode::InvalidQuery.into());
        }
        for (path, size) in &files {
            text(path)?;
            if *size < 0 {
                return Err(ErrorCode::NumericOverflow.into());
            }
        }
        self.write(move|conn| {let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;fresh(&tx,&handle)?;
            let complete:bool=tx.query_row("SELECT discovery_complete FROM source_scan_state WHERE source_id=?1",[&handle.source_id],|r|r.get(0))?;
            if complete {return Err(ErrorCode::CheckpointConflict.into())}
            for (path,size) in files {
                tx.execute("INSERT INTO source_scan_files(source_id,scan_revision,canonical_path,upper_bound,file_id) VALUES(?1,?2,?3,?4,(SELECT file_id FROM source_files WHERE source_id=?1 AND canonical_path=?3)) ON CONFLICT(source_id,scan_revision,canonical_path) DO UPDATE SET upper_bound=excluded.upper_bound,file_generation_id=NULL,checkpoint_revision=NULL,checked_at_ms=NULL",params![handle.source_id,handle.revision,path,size])?;
            }
            tx.commit()?;Ok(())
        })
    }
    pub fn finish_source_scan(
        &self,
        handle: ScanHandle,
        issue: Option<ErrorCode>,
        at_ms: i64,
    ) -> StoreResult<()> {
        EpochMs::new(at_ms)?;
        self.write(move|conn| {let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;fresh(&tx,&handle)?;
            let code=issue.map(serde_json::to_value).transpose()?;
            tx.execute("UPDATE source_scan_state SET discovery_complete=1,issue_code=?1 WHERE source_id=?2 AND scan_revision=?3",params![code.as_ref().and_then(|v|v.as_str()),handle.source_id,handle.revision])?;
            refresh(&tx,&handle,at_ms)?;tx.commit()?;Ok(())
        })
    }
    /// False means the path was outside this enumeration; only a new full scan may restore proof.
    pub fn confirm_source_scan_file(
        &self,
        handle: ScanHandle,
        path: String,
        generation: String,
        checkpoint_revision: i64,
        at_ms: i64,
    ) -> StoreResult<bool> {
        text(&path)?;
        text(&generation)?;
        EpochMs::new(at_ms)?;
        if checkpoint_revision < 0 {
            return Err(ErrorCode::InvalidQuery.into());
        }
        self.write(move|conn| {let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;fresh(&tx,&handle)?;
            let file:Option<(String,i64)>=tx.query_row("SELECT f.file_id,g.observed_size FROM file_generations g JOIN source_files f ON f.current_generation_id=g.file_generation_id WHERE f.source_id=?1 AND f.canonical_path=?2 AND g.file_generation_id=?3 AND g.checkpoint_revision=?4 AND g.state='current'",params![handle.source_id,path,generation,checkpoint_revision],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
            let (file_id,upper)=file.ok_or(ErrorCode::CheckpointConflict)?;
            let found=tx.execute("UPDATE source_scan_files SET file_id=?1,file_generation_id=?2,checkpoint_revision=?3,upper_bound=?4,checked_at_ms=?5 WHERE source_id=?6 AND scan_revision=?7 AND canonical_path=?8",params![file_id,generation,checkpoint_revision,upper,at_ms,handle.source_id,handle.revision,path])?==1;
            if !found {tx.execute("UPDATE source_scan_state SET invalidated=1 WHERE source_id=?1",[&handle.source_id])?;}
            refresh(&tx,&handle,at_ms)?;tx.commit()?;Ok(found)
        })
    }
    /// Invalidate a known file. Unknown membership requires directory reconciliation.
    pub fn invalidate_source_scan_file(
        &self,
        source_id: String,
        path: String,
    ) -> StoreResult<bool> {
        text(&path)?;
        self.write(move|conn| {let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM source_scan_state WHERE source_id=?1)",[&source_id],|r|r.get(0))?;
            if !exists {tx.commit()?;return Ok(true)}
            let known=tx.execute("UPDATE source_scan_files SET file_generation_id=NULL,checkpoint_revision=NULL,checked_at_ms=NULL WHERE source_id=?1 AND canonical_path=?2",params![source_id,path])?==1;
            tx.execute("UPDATE source_scan_state SET state=CASE WHEN discovery_complete=0 THEN 'scanning' ELSE 'incomplete' END,invalidated=CASE WHEN ?1 THEN invalidated ELSE 1 END,completed_at_ms=NULL WHERE source_id=?2",params![known,source_id])?;
            tx.commit()?;Ok(!known)
        })
    }
    pub fn interrupt_source_scans(&self) -> StoreResult<()> {
        self.write(|conn| {conn.execute("UPDATE source_scan_state SET state='interrupted',invalidated=1,completed_at_ms=NULL",[])?;Ok(())})
    }
}

#[cfg(test)]
mod tests;
