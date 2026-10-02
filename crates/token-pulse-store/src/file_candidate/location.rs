//! Move an observed replacement's logical file, without importing its bytes as unrelated history.
use super::*;
use std::path::{Component, Path};

pub struct FileCandidateRelocation {
    pub generation_id: String,
    pub expected_checkpoint_revision: i64,
    pub expected_path: String,
    pub path: String,
    pub identity: String,
    pub at_ms: i64,
}
fn path_in_source(root: &str, path: &str) -> bool {
    let path = Path::new(path);
    path.is_absolute()
        && !path.components().any(|c| matches!(c, Component::ParentDir))
        && path
            .strip_prefix(root)
            .ok()
            .and_then(|p| p.components().next())
            .is_some_and(
                |c| matches!(c,Component::Normal(s) if s=="sessions" || s=="archived_sessions"),
            )
        && path
            .extension()
            .is_some_and(|s| s.eq_ignore_ascii_case("jsonl"))
}
impl Database {
    /// Includes failed candidates whose old input is still current: a job can observe a move first.
    /// Different logical owners of one identity are ambiguous, never permission to merge files.
    pub fn file_candidate_at_identity(
        &self,
        source: &str,
        identity: &str,
    ) -> StoreResult<Option<FileReadCandidate>> {
        self.snapshot(|tx,_| {
            let mut q=tx.prepare("SELECT c.generation_id,c.file_id FROM file_read_candidates c JOIN file_generations g ON g.file_generation_id=c.generation_id JOIN source_files f ON f.file_id=c.file_id JOIN sources s ON s.source_id=f.source_id WHERE f.source_id=?1 AND s.enabled=1 AND json_extract(g.identity_json,'$')=?2 AND c.state IN ('reading','ready','claimed','failed') AND f.current_generation_id=json_extract(c.base_json,'$.generation_id') AND f.canonical_path=json_extract(c.base_json,'$.path') AND s.root_path=json_extract(c.base_json,'$.source_root') ORDER BY CASE WHEN c.state='failed' THEN 1 ELSE 0 END,c.updated_at_ms DESC,c.generation_id DESC")?;
            let mut rows=q.query(params![source,identity])?;
            let mut selected=None;let mut owner=None;
            while let Some(row)=rows.next()? {
                let generation:String=row.get(0)?;let file:String=row.get(1)?;
                if owner.as_ref().is_some_and(|owner|owner!=&file) {return Err(ErrorCode::AmbiguousUsage.into());}
                owner=Some(file.clone());
                if selected.is_none() {let c=load(tx,&generation)?;if frozen(tx,&file)?==c.base {selected=Some(c);}}
            }
            Ok(selected)
        })
    }
    /// The reader first validates the actual rollout path/identity and disappearance of the old path.
    /// CAS preserves read progress; a claimed move fails the owning group before relocating.
    pub fn relocate_file_candidate(&self, request: FileCandidateRelocation) -> StoreResult<()> {
        validate_request_id(&request.generation_id)?;
        EpochMs::new(request.at_ms)?;
        if request.expected_checkpoint_revision < 0
            || request.path.len() > 32768
            || request.path.chars().any(char::is_control)
        {
            return Err(ErrorCode::InvalidQuery.into());
        }
        self.write(move|conn| {
            let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;let mut c=load(&tx,&request.generation_id)?;
            if !c.base.enabled || !matches!(c.state.as_str(),"reading"|"ready"|"claimed"|"failed")
                || c.base!=frozen(&tx,&c.checkpoint.file_id)? || c.base.path!=request.expected_path
                || c.checkpoint.checkpoint_revision!=request.expected_checkpoint_revision
                || c.checkpoint.file_identity.as_deref()!=Some(&request.identity)
            {return Err(ErrorCode::CandidateObsolete.into());}
            if !path_in_source(&c.base.source_root,&request.path) || request.path==request.expected_path {return Err(ErrorCode::InvalidQuery.into());}
            if tx.query_row("SELECT EXISTS(SELECT 1 FROM source_files WHERE source_id=?1 AND canonical_path=?2 AND file_id<>?3)",params![c.base.source_id,request.path,c.checkpoint.file_id],|r|r.get::<_,bool>(0))? {return Err(ErrorCode::RevisionConflict.into());}
            if c.state=="claimed" {
                fresh(&tx,&c)?;
                let job:String=tx.query_row("SELECT job_id FROM file_rebuild_candidates WHERE generation_id=?1",[&request.generation_id],|r|r.get(0))?;
                crate::rebuild::fail_in_tx(&tx,&job,ErrorCode::CandidateObsolete,request.at_ms)?;
            } else if matches!(c.state.as_str(),"reading"|"ready") {
                fresh(&tx,&c)?;
                let revision=c.checkpoint.checkpoint_revision.checked_add(1).ok_or(ErrorCode::NumericOverflow)?;
                c.base.path=request.path.clone();
                tx.execute("UPDATE file_read_candidates SET base_json=?1,updated_at_ms=?2 WHERE generation_id=?3",params![serde_json::to_string(&c.base)?,request.at_ms,request.generation_id])?;
                tx.execute("UPDATE file_generations SET checkpoint_revision=?1 WHERE file_generation_id=?2",params![revision,request.generation_id])?;
            }
            tx.execute("UPDATE source_files SET canonical_path=?1,status='correction_pending',last_seen_at_ms=?2 WHERE file_id=?3",params![request.path,request.at_ms,c.checkpoint.file_id])?;
            tx.execute("UPDATE source_scan_files SET file_generation_id=NULL,checkpoint_revision=NULL,checked_at_ms=NULL WHERE source_id=?1 AND (file_id=?2 OR canonical_path=?3)",params![c.base.source_id,c.checkpoint.file_id,request.path])?;
            tx.execute("UPDATE source_scan_state SET invalidated=1,state=CASE WHEN discovery_complete=0 THEN 'scanning' ELSE 'incomplete' END,completed_at_ms=NULL WHERE source_id=?1",[c.base.source_id])?;
            tx.commit()?;Ok(())
        })
    }
}
#[cfg(test)]
mod tests;
