//! Replacement reads are isolated from active observations, baselines and checkpoints.
use crate::{
    Database, ErrorCode, StoreResult,
    batch::{DiagnosticWrite, ObservationWrite},
    collection::FileCheckpoint,
};
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use token_pulse_core::{
    domain::{
        ContentAnchor, NormalizedObservation, PARSER_VERSION, PhysicalPosition, ReaderContext,
    },
    numeric::EpochMs,
    protocol::validate_request_id,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenFile {
    pub source_id: String,
    pub source_root: String,
    pub path: String,
    pub enabled: bool,
    pub generation_id: String,
    pub generation_state: String,
    pub file_identity: Option<String>,
    pub committed_offset: i64,
    pub checkpoint_revision: i64,
    pub observed_size: i64,
    pub identity_json: String,
    pub anchors_json: String,
    pub context_json: String,
    pub parser_version: String,
}
pub struct FileReadCandidate {
    pub base: FrozenFile,
    pub checkpoint: FileCheckpoint,
    pub state: String,
}
pub struct BeginFileCandidate {
    pub generation_id: String,
    pub file_id: String,
    pub expected_generation_id: String,
    pub expected_checkpoint_revision: i64,
    pub identity: String,
    pub observed_size: i64,
    pub at_ms: i64,
}
pub struct FileCandidateBatch {
    pub generation_id: String,
    pub expected_offset: i64,
    pub expected_checkpoint_revision: i64,
    pub next_offset: i64,
    pub observed_size: i64,
    pub anchors: Vec<ContentAnchor>,
    pub context: ReaderContext,
    pub observations: Vec<ObservationWrite>,
    pub diagnostics: Vec<DiagnosticWrite>,
    pub at_ms: i64,
}
fn frozen(tx: &Transaction<'_>, file: &str) -> StoreResult<FrozenFile> {
    Ok(tx.query_row("SELECT f.source_id,s.root_path,f.canonical_path,s.enabled,g.file_generation_id,g.state,f.file_identity,g.committed_offset,g.checkpoint_revision,g.observed_size,g.identity_json,g.anchor_json,g.reader_context_json,g.parser_version FROM source_files f JOIN sources s USING(source_id) JOIN file_generations g ON g.file_generation_id=f.current_generation_id WHERE f.file_id=?1",[file],|r|Ok(FrozenFile {source_id:r.get(0)?,source_root:r.get(1)?,path:r.get(2)?,enabled:r.get(3)?,generation_id:r.get(4)?,generation_state:r.get(5)?,file_identity:r.get(6)?,committed_offset:r.get(7)?,checkpoint_revision:r.get(8)?,observed_size:r.get(9)?,identity_json:r.get(10)?,anchors_json:r.get(11)?,context_json:r.get(12)?,parser_version:r.get(13)?}))?)
}
fn load(tx: &Transaction<'_>, generation: &str) -> StoreResult<FileReadCandidate> {
    let (base,file,state,identity,size,offset,revision,anchors,context):(String,String,String,String,i64,i64,i64,String,String)=tx.query_row("SELECT c.base_json,c.file_id,c.state,g.identity_json,g.observed_size,g.committed_offset,g.checkpoint_revision,g.anchor_json,g.reader_context_json FROM file_read_candidates c JOIN file_generations g ON g.file_generation_id=c.generation_id WHERE c.generation_id=?1",[generation],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?,r.get(8)?)))?;
    let base: FrozenFile = serde_json::from_str(&base)?;
    Ok(FileReadCandidate {
        checkpoint: FileCheckpoint {
            file_id: file,
            source_id: base.source_id.clone(),
            file_generation_id: generation.into(),
            file_identity: serde_json::from_str(&identity)?,
            observed_size: size,
            committed_offset: offset,
            checkpoint_revision: revision,
            anchors: serde_json::from_str(&anchors)?,
            context: serde_json::from_str(&context)?,
        },
        base,
        state,
    })
}
fn fresh(tx: &Transaction<'_>, candidate: &FileReadCandidate) -> StoreResult<()> {
    if frozen(tx, &candidate.checkpoint.file_id)? != candidate.base {
        return Err(ErrorCode::CandidateObsolete.into());
    }
    let valid:bool=tx.query_row("SELECT state='candidate' AND parser_version=?2 FROM file_generations WHERE file_generation_id=?1",params![candidate.checkpoint.file_generation_id,PARSER_VERSION],|r|r.get(0))?;
    if !valid {
        return Err(ErrorCode::CandidateObsolete.into());
    }
    Ok(())
}
fn position(record: &NormalizedObservation) -> (&PhysicalPosition, Option<&str>) {
    match record {
        NormalizedObservation::SessionMetadata {
            physical_position, ..
        } => (physical_position, None),
        NormalizedObservation::TurnMetadata {
            physical_position,
            session_key,
            ..
        }
        | NormalizedObservation::Context {
            physical_position,
            session_key,
            ..
        } => (physical_position, Some(session_key)),
        NormalizedObservation::Usage(u) => (&u.physical_position, Some(&u.session_key)),
    }
}
fn anchors_valid(anchors: &[ContentAnchor], upper: u64) -> bool {
    anchors.iter().all(|a| {
        a.byte_length > 0
            && a.byte_offset
                .checked_add(u64::from(a.byte_length))
                .is_some_and(|end| end <= upper)
            && a.sha256.len() == 64
            && a.sha256
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    })
}
impl Database {
    pub fn begin_file_read_candidate(
        &self,
        request: BeginFileCandidate,
    ) -> StoreResult<FileReadCandidate> {
        validate_request_id(&request.generation_id)?;
        validate_request_id(&request.file_id)?;
        validate_request_id(&request.expected_generation_id)?;
        EpochMs::new(request.at_ms)?;
        if request.identity.is_empty()
            || request.identity.len() > 4096
            || request.identity.chars().any(char::is_control)
            || request.generation_id == request.expected_generation_id
            || request.observed_size < 0
            || request.expected_checkpoint_revision < 0
        {
            return Err(ErrorCode::InvalidQuery.into());
        }
        self.write(move|conn| {let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let base=frozen(&tx,&request.file_id)?;
            if !base.enabled {return Err(ErrorCode::PermissionDenied.into());}
            if base.generation_id!=request.expected_generation_id||base.checkpoint_revision!=request.expected_checkpoint_revision||base.generation_state!="current" {return Err(ErrorCode::CheckpointConflict.into());}
            if base.parser_version!=PARSER_VERSION {return Err(ErrorCode::UnsupportedFormat.into());}
            let existing:Option<(String,i64)>=tx.query_row("SELECT file_id,initial_size FROM file_read_candidates WHERE generation_id=?1",[&request.generation_id],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
            if let Some((file,size))=existing {
                let candidate=load(&tx,&request.generation_id)?;fresh(&tx,&candidate)?;
                if file!=request.file_id||size!=request.observed_size||candidate.checkpoint.file_identity.as_deref()!=Some(&request.identity)||!matches!(candidate.state.as_str(),"reading"|"ready") {return Err(ErrorCode::RevisionConflict.into());}
                tx.commit()?;return Ok(candidate);
            }
            if tx.query_row("SELECT EXISTS(SELECT 1 FROM file_read_candidates WHERE file_id=?1 AND state IN ('reading','ready','claimed'))",[&request.file_id],|r|r.get::<_,bool>(0))? {return Err(ErrorCode::RevisionConflict.into());}
            tx.execute("INSERT INTO file_generations(file_generation_id,file_id,state,identity_json,observed_size,anchor_json,reader_context_json,parser_version,created_at_ms) VALUES(?1,?2,'candidate',?3,?4,'[]',?5,?6,?7)",params![request.generation_id,request.file_id,serde_json::to_string(&Some(&request.identity))?,request.observed_size,serde_json::to_string(&ReaderContext::default())?,PARSER_VERSION,request.at_ms])?;
            tx.execute("INSERT INTO file_read_candidates(generation_id,file_id,base_json,initial_size,state,created_at_ms,updated_at_ms) VALUES(?1,?2,?3,?4,'reading',?5,?5)",params![request.generation_id,request.file_id,serde_json::to_string(&base)?,request.observed_size,request.at_ms])?;
            tx.execute("UPDATE source_files SET status='correction_pending' WHERE file_id=?1",[&request.file_id])?;
            tx.execute("UPDATE source_scan_files SET file_generation_id=NULL,checkpoint_revision=NULL,checked_at_ms=NULL WHERE source_id=?1 AND canonical_path=?2",params![base.source_id,base.path])?;
            tx.execute("UPDATE source_scan_state SET state='incomplete',completed_at_ms=NULL WHERE source_id=?1",[&base.source_id])?;
            let candidate=load(&tx,&request.generation_id)?;tx.commit()?;Ok(candidate)
        })
    }
    pub fn file_read_candidate(&self, generation: &str) -> StoreResult<FileReadCandidate> {
        self.snapshot(|tx, _| load(tx, generation))
    }
    pub fn active_file_read_candidate(&self, file: &str) -> StoreResult<Option<FileReadCandidate>> {
        self.snapshot(|tx,_| {
            let id:Option<String>=tx.query_row("SELECT generation_id FROM file_read_candidates WHERE file_id=?1 AND state IN ('reading','ready','claimed')",[file],|r|r.get(0)).optional()?;
            id.map(|id|load(tx,&id)).transpose()
        })
    }
    pub fn validate_file_read_candidate(&self, generation: &str) -> StoreResult<()> {
        self.snapshot(|tx, _| fresh(tx, &load(tx, generation)?))
    }
    pub fn reopen_file_read_candidate(
        &self,
        generation: String,
        expected_revision: i64,
        at_ms: i64,
    ) -> StoreResult<()> {
        EpochMs::new(at_ms)?;
        self.write(move|conn| {let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let c=load(&tx,&generation)?;fresh(&tx,&c)?;
            if c.state!="ready"||c.checkpoint.checkpoint_revision!=expected_revision {return Err(ErrorCode::RevisionConflict.into());}
            tx.execute("UPDATE file_read_candidates SET state='reading',updated_at_ms=?1 WHERE generation_id=?2",params![at_ms,generation])?;tx.commit()?;Ok(())
        })
    }
    pub fn stage_file_candidate_batch(&self, batch: FileCandidateBatch) -> StoreResult<i64> {
        EpochMs::new(batch.at_ms)?;
        if batch.expected_offset < 0
            || batch.expected_checkpoint_revision < 0
            || batch.next_offset < batch.expected_offset
            || batch.next_offset > batch.observed_size
            || batch.observations.len() > 500
            || batch.diagnostics.len() > 500
            || !anchors_valid(&batch.anchors, batch.next_offset as u64)
        {
            return Err(ErrorCode::InvalidQuery.into());
        }
        if let Some(skip) = &batch.context.oversized_line {
            if skip.start_offset != batch.next_offset as u64
                || skip.scan_offset < skip.start_offset
                || skip.scan_offset > batch.observed_size as u64
                || !anchors_valid(&skip.anchors, skip.scan_offset)
            {
                return Err(ErrorCode::InvalidQuery.into());
            }
        }
        let mut bytes =
            serde_json::to_vec(&(&batch.anchors, &batch.context, &batch.diagnostics))?.len();
        let mut previous = batch.expected_offset as u64;
        for o in &batch.observations {
            validate_request_id(&o.observation_id)?;
            let (p, session) = position(&o.record);
            p.validate()?;
            if p.file_generation_id != batch.generation_id
                || p.byte_offset < previous
                || p.byte_end > batch.next_offset as u64
                || session.is_some_and(|s| Some(s) != o.session_key.as_deref())
            {
                return Err(ErrorCode::CheckpointConflict.into());
            }
            previous = p.byte_end;
            bytes = bytes
                .checked_add(
                    serde_json::to_vec(&(
                        &o.record,
                        &o.session_key,
                        &o.observation_id,
                        &o.payload_fingerprint,
                    ))?
                    .len(),
                )
                .ok_or(ErrorCode::NumericOverflow)?;
        }
        if bytes > 16 * 1024 * 1024 {
            return Err(ErrorCode::InvalidQuery.into());
        }
        self.write(move|conn| {let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let candidate=load(&tx,&batch.generation_id)?;fresh(&tx,&candidate)?;
            if candidate.state!="reading" {return Err(ErrorCode::RevisionConflict.into());}
            if batch.observed_size<candidate.checkpoint.observed_size {return Err(ErrorCode::CandidateObsolete.into());}
            if (candidate.checkpoint.committed_offset,candidate.checkpoint.checkpoint_revision)!=(batch.expected_offset,batch.expected_checkpoint_revision) {return Err(ErrorCode::CheckpointConflict.into());}
            for o in batch.observations {
                let (p,_)=position(&o.record);
                tx.execute("INSERT INTO file_candidate_observations(generation_id,observation_id,byte_offset,byte_end,session_key,normalized_json,payload_fingerprint) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![batch.generation_id,o.observation_id,p.byte_offset as i64,p.byte_end as i64,o.session_key,serde_json::to_string(&o.record)?,o.payload_fingerprint])?;
            }
            for d in batch.diagnostics {
                validate_request_id(&d.diagnostic_id)?;EpochMs::new(d.observed_at_ms)?;
                if d.file_generation_id.as_deref()!=Some(&batch.generation_id)||d.source_id.as_deref().is_some_and(|s|s!=candidate.base.source_id)||d.byte_offset.is_some_and(|p|p<batch.expected_offset||p>batch.next_offset)||!matches!(d.severity.as_str(),"info"|"warning"|"error") {return Err(ErrorCode::InvalidQuery.into());}
                tx.execute("INSERT INTO file_candidate_diagnostics(generation_id,diagnostic_id,byte_offset,diagnostic_json) VALUES(?1,?2,?3,?4)",params![batch.generation_id,d.diagnostic_id,d.byte_offset,serde_json::to_string(&d)?])?;
            }
            let revision=batch.expected_checkpoint_revision.checked_add(1).ok_or(ErrorCode::NumericOverflow)?;
            tx.execute("UPDATE file_generations SET committed_offset=?1,observed_size=?2,checkpoint_revision=?3,anchor_json=?4,reader_context_json=?5 WHERE file_generation_id=?6",params![batch.next_offset,batch.observed_size,revision,serde_json::to_string(&batch.anchors)?,serde_json::to_string(&batch.context)?,batch.generation_id])?;
            tx.execute("UPDATE file_read_candidates SET updated_at_ms=?1 WHERE generation_id=?2",params![batch.at_ms,batch.generation_id])?;
            tx.commit()?;Ok(revision)
        })
    }
    /// Structural EOF only. Physical verification and ledger validation are still required to publish.
    pub fn seal_file_read_candidate(
        &self,
        generation: String,
        expected_revision: i64,
        at_ms: i64,
    ) -> StoreResult<()> {
        EpochMs::new(at_ms)?;
        self.write(move|conn| {let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let candidate=load(&tx,&generation)?;fresh(&tx,&candidate)?;
            if candidate.state!="reading"||candidate.checkpoint.checkpoint_revision!=expected_revision||candidate.checkpoint.committed_offset!=candidate.checkpoint.observed_size||candidate.checkpoint.context.oversized_line.is_some() {return Err(ErrorCode::CheckpointConflict.into());}
            tx.execute("UPDATE file_read_candidates SET state='ready',updated_at_ms=?1 WHERE generation_id=?2",params![at_ms,generation])?;tx.commit()?;Ok(())
        })
    }
    pub fn fail_file_read_candidate(
        &self,
        generation: String,
        code: ErrorCode,
        at_ms: i64,
    ) -> StoreResult<()> {
        EpochMs::new(at_ms)?;
        self.write(move|conn| {let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let candidate=load(&tx,&generation)?;
            if !matches!(candidate.state.as_str(),"reading"|"ready") {return Err(ErrorCode::RevisionConflict.into());}
            tx.execute("UPDATE file_read_candidates SET state='failed',error_code=?1,updated_at_ms=?2 WHERE generation_id=?3",params![serde_json::to_value(code)?.as_str(),at_ms,generation])?;
            tx.execute("UPDATE file_generations SET state='invalid' WHERE file_generation_id=?1 AND state='candidate'",[generation])?;
            tx.commit()?;Ok(())
        })
    }
}
#[cfg(test)]
mod tests;
