//! An owning rebuild registers necessary candidate inputs without publishing consumption.
use super::*;
use crate::{batch, jobs};
use token_pulse_core::{
    jobs::JobScope,
    protocol::{JobKind, JobState},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileCandidateRegistration {
    pub generation_id: String,
    pub job_id: String,
    pub checkpoint_revision: i64,
    pub after_offset: i64,
    pub materialized: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistrationReceipt {
    pub after_offset: i64,
    pub registered_rows: usize,
    pub complete: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProposedSession {
    pub session_key: String,
    pub provider_session_id: String,
    pub parent_provider_id: Option<String>,
    pub created_at_ms: Option<i64>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplacementInput {
    pub generation_id: String,
    pub base: FrozenFile,
    pub context_json: String,
    pub after_offset: i64,
    pub sessions: Vec<ProposedSession>,
}
// The same selection is used before planning, throughout replay and immediately before commit.
// It deliberately does not use `owner`: replay has already frozen its ledger manifest.
pub(crate) fn frozen_inputs(
    tx: &Transaction<'_>,
    job_id: &str,
) -> StoreResult<Vec<ReplacementInput>> {
    let job = jobs::load(tx, job_id)?;
    let mut q = tx.prepare(
        "SELECT generation_id FROM file_rebuild_candidates WHERE job_id=?1 ORDER BY generation_id",
    )?;
    let generations = q
        .query_map([job_id], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(q);
    let mut result = Vec::new();
    for generation in generations {
        let c = load(tx, &generation)?;
        fresh(tx, &c)?;
        let r = registration(tx, &generation)?;
        let last: i64 = tx.query_row("SELECT COALESCE(MAX(byte_offset),-1) FROM file_candidate_observations WHERE generation_id=?1", [&generation], |r| r.get(0))?;
        if !permitted(&job, &c.base.source_id)
            || !matches!(
                job.job.state,
                JobState::Running | JobState::Validating | JobState::Publishing
            )
            || c.state != "claimed"
            || !r.materialized
            || r.after_offset != last
            || r.checkpoint_revision != c.checkpoint.checkpoint_revision
            || c.checkpoint.committed_offset != c.checkpoint.observed_size
            || c.checkpoint.context.oversized_line.is_some()
        {
            return Err(ErrorCode::CandidateObsolete.into());
        }
        let mut q = tx.prepare("SELECT session_key,provider_session_id,parent_provider_id,created_at_ms FROM file_rebuild_sessions WHERE generation_id=?1 ORDER BY session_key")?;
        let sessions = q
            .query_map([&generation], |r| {
                Ok(ProposedSession {
                    session_key: r.get(0)?,
                    provider_session_id: r.get(1)?,
                    parent_provider_id: r.get(2)?,
                    created_at_ms: r.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        result.push(ReplacementInput {
            generation_id: generation,
            base: c.base,
            context_json: serde_json::to_string(&c.checkpoint.context)?,
            after_offset: r.after_offset,
            sessions,
        });
    }
    Ok(result)
}
fn registration(tx: &Transaction<'_>, generation: &str) -> StoreResult<FileCandidateRegistration> {
    tx.query_row("SELECT generation_id,job_id,checkpoint_revision,after_offset,materialized FROM file_rebuild_candidates WHERE generation_id=?1",[generation],|r|Ok(FileCandidateRegistration{generation_id:r.get(0)?,job_id:r.get(1)?,checkpoint_revision:r.get(2)?,after_offset:r.get(3)?,materialized:r.get(4)?})).optional()?.ok_or(ErrorCode::InvalidQuery.into())
}
pub(super) fn permitted(job: &jobs::StoredJob, source: &str) -> bool {
    job.job.kind == JobKind::Rebuild
        && match &job.request.scope {
            JobScope::All {} => true,
            JobScope::Sources { source_ids } => source_ids.iter().any(|s| s == source),
            JobScope::Sessions { .. } => false,
        }
}
fn owner(
    tx: &Transaction<'_>,
    generation: &str,
    job_id: &str,
) -> StoreResult<(FileReadCandidate, FileCandidateRegistration)> {
    let c = load(tx, generation)?;
    fresh(tx, &c)?;
    let r = registration(tx, generation)?;
    if r.job_id != job_id
        || c.state != "claimed"
        || r.checkpoint_revision != c.checkpoint.checkpoint_revision
        || c.checkpoint.committed_offset != c.checkpoint.observed_size
        || c.checkpoint.context.oversized_line.is_some()
    {
        return Err(ErrorCode::RevisionConflict.into());
    }
    let job = jobs::load(tx, job_id)?;
    if job.job.state == JobState::Cancelling {
        return Err(ErrorCode::JobCancelled.into());
    }
    if !permitted(&job, &c.base.source_id)
        || job.job.state != JobState::Running
        || !job.checkpoint.candidate_ledger_ids.is_empty()
        || crate::rebuild::has_manifest(tx, job_id)?
    {
        return Err(ErrorCode::RevisionConflict.into());
    }
    Ok((c, r))
}
fn header(tx: &Transaction<'_>, generation: &str, o: &ObservationWrite) -> StoreResult<()> {
    let NormalizedObservation::SessionMetadata {
        provider_session_id,
        metadata,
        created_at_ms,
        ..
    } = &o.record
    else {
        return Ok(());
    };
    let key = o
        .session_key
        .as_deref()
        .ok_or(ErrorCode::CheckpointConflict)?;
    validate_request_id(key)?;
    validate_request_id(provider_session_id)?;
    if let Some(parent) = &metadata.parent_provider_id {
        validate_request_id(parent)?;
    }
    if let Some(time) = created_at_ms {
        EpochMs::new(*time)?;
    }
    let existing: Option<(String, Option<String>)> = tx
        .query_row(
            "SELECT provider,provider_session_id FROM sessions WHERE session_key=?1",
            [key],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    if let Some((provider, id)) = existing {
        if provider != "codex" || id.as_deref() != Some(provider_session_id) {
            return Err(ErrorCode::CheckpointConflict.into());
        }
    } else {
        tx.execute("INSERT INTO sessions(session_key,provider,provider_session_id,identity_status,parent_provider_id,created_at_ms) VALUES(?1,'codex',?2,'candidate',?3,?4)",params![key,provider_session_id,metadata.parent_provider_id,created_at_ms])?;
    }
    let prior:Option<(String,Option<String>,Option<i64>)>=tx.query_row("SELECT provider_session_id,parent_provider_id,created_at_ms FROM file_rebuild_sessions WHERE generation_id=?1 AND session_key=?2",params![generation,key],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
    if let Some(prior) = prior {
        if prior
            != (
                provider_session_id.clone(),
                metadata.parent_provider_id.clone(),
                *created_at_ms,
            )
        {
            return Err(ErrorCode::CheckpointConflict.into());
        }
    } else {
        tx.execute(
            "INSERT INTO file_rebuild_sessions VALUES(?1,?2,?3,?4,?5)",
            params![
                generation,
                key,
                provider_session_id,
                metadata.parent_provider_id,
                created_at_ms
            ],
        )?;
    }
    Ok(())
}
/// Terminal work releases only the owning claim; old physical pointers and ledgers remain intact.
pub(crate) fn fail_owned(
    tx: &Transaction<'_>,
    job: Option<&str>,
    code: ErrorCode,
    at: i64,
) -> StoreResult<()> {
    let mut q=tx.prepare("SELECT c.generation_id FROM file_read_candidates c JOIN file_rebuild_candidates r ON r.generation_id=c.generation_id JOIN jobs j ON j.job_id=r.job_id WHERE c.state IN ('reading','ready','claimed') AND ((?1 IS NOT NULL AND r.job_id=?1) OR (?1 IS NULL AND j.state IN ('cancelled','failed','interrupted')))")?;
    let ids = q
        .query_map([job], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(q);
    for id in ids {
        tx.execute("UPDATE file_read_candidates SET state='failed',error_code=?1,updated_at_ms=?2 WHERE generation_id=?3",params![code.to_string(),at,id])?;
        tx.execute("UPDATE file_generations SET state='invalid' WHERE file_generation_id=?1 AND state='candidate'",[id])?;
    }
    Ok(())
}
pub(crate) fn has_owned(tx: &Transaction<'_>, job: &str) -> StoreResult<bool> {
    Ok(tx.query_row("SELECT EXISTS(SELECT 1 FROM file_rebuild_candidates r JOIN file_read_candidates c ON c.generation_id=r.generation_id WHERE r.job_id=?1 AND c.state IN ('reading','ready','claimed'))",[job],|r|r.get(0))?)
}
fn claim_in_tx(
    tx: &Transaction<'_>,
    generation: &str,
    job_id: &str,
    expected_revision: i64,
    at_ms: i64,
) -> StoreResult<FileCandidateRegistration> {
    let c = load(tx, generation)?;
    fresh(tx, &c)?;
    let job = jobs::load(tx, job_id)?;
    if job.job.state == JobState::Cancelling {
        return Err(ErrorCode::JobCancelled.into());
    }
    if !permitted(&job, &c.base.source_id)
        || !matches!(job.job.state, JobState::Queued | JobState::Running)
        || !job.checkpoint.candidate_ledger_ids.is_empty()
        || crate::rebuild::has_manifest(tx, job_id)?
        || expected_revision != c.checkpoint.checkpoint_revision
        || c.checkpoint.committed_offset != c.checkpoint.observed_size
        || c.checkpoint.context.oversized_line.is_some()
    {
        return Err(ErrorCode::RevisionConflict.into());
    }
    if c.state == "claimed" {
        let r = registration(tx, generation)?;
        if r.job_id != job_id || r.checkpoint_revision != expected_revision {
            return Err(ErrorCode::RevisionConflict.into());
        }
        return Ok(r);
    }
    if c.state != "ready" {
        return Err(ErrorCode::RevisionConflict.into());
    }
    let existing: Option<String> = tx
        .query_row(
            "SELECT job_id FROM file_rebuild_candidates WHERE generation_id=?1",
            [generation],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(owner) = existing {
        let r = registration(tx, generation)?;
        if owner != job_id
            || !job.checkpoint.reread_sources
            || r.materialized
            || r.after_offset != -1
        {
            return Err(ErrorCode::RevisionConflict.into());
        }
        tx.execute("UPDATE file_rebuild_candidates SET checkpoint_revision=?1,updated_at_ms=?2 WHERE generation_id=?3",params![expected_revision,at_ms,generation])?;
    } else {
        tx.execute("INSERT INTO file_rebuild_candidates(generation_id,job_id,checkpoint_revision,created_at_ms,updated_at_ms) VALUES(?1,?2,?3,?4,?4)",params![generation,job_id,expected_revision,at_ms])?;
    }
    tx.execute(
        "UPDATE file_read_candidates SET state='claimed',updated_at_ms=?1 WHERE generation_id=?2",
        params![at_ms, generation],
    )?;
    registration(tx, generation)
}
impl Database {
    /// Queue and claim commit together, so an executor cannot see an unassociated request.
    pub fn enqueue_file_candidate_rebuild(
        &self,
        generation: String,
        expected_revision: i64,
        at_ms: i64,
    ) -> StoreResult<token_pulse_core::protocol::Job> {
        use sha2::{Digest, Sha256};
        validate_request_id(&generation)?;
        EpochMs::new(at_ms)?;
        if expected_revision < 0 {
            return Err(ErrorCode::InvalidQuery.into());
        }
        self.write(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let c = load(&tx, &generation)?;
            let job_id = format!(
                "replacement-job-{:x}",
                Sha256::digest(serde_json::to_vec(&(&generation, expected_revision))?)
            );
            let mut request = token_pulse_core::jobs::JobRequest {
                kind: JobKind::Rebuild,
                scope: JobScope::Sources {
                    source_ids: vec![c.base.source_id],
                },
                request_key: job_id.clone(),
            };
            request.validate()?;
            let job = jobs::create_in_tx(&tx, &job_id, &request, at_ms)?;
            if token_pulse_core::jobs::finished(job.state) {
                let r = registration(&tx, &generation)?;
                if r.job_id != job_id
                    || r.checkpoint_revision != expected_revision
                    || !matches!(c.state.as_str(), "failed" | "published")
                {
                    return Err(ErrorCode::RevisionConflict.into());
                }
            } else {
                claim_in_tx(&tx, &generation, &job_id, expected_revision, at_ms)?;
            }
            tx.commit()?;
            Ok(job)
        })
    }
    /// Claims a structurally complete generation for an existing scoped rebuild. No file is opened.
    pub fn claim_file_candidate_for_rebuild(
        &self,
        generation: String,
        job_id: String,
        expected_revision: i64,
        at_ms: i64,
    ) -> StoreResult<FileCandidateRegistration> {
        validate_request_id(&generation)?;
        validate_request_id(&job_id)?;
        EpochMs::new(at_ms)?;
        if expected_revision < 0 {
            return Err(ErrorCode::InvalidQuery.into());
        }
        self.write(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let result = claim_in_tx(&tx, &generation, &job_id, expected_revision, at_ms)?;
            tx.commit()?;
            Ok(result)
        })
    }
    pub fn rebuild_file_candidates(
        &self,
        job_id: &str,
    ) -> StoreResult<Vec<FileCandidateRegistration>> {
        validate_request_id(job_id)?;
        self.snapshot(|tx,_| {
            let mut q=tx.prepare("SELECT generation_id FROM file_rebuild_candidates WHERE job_id=?1 ORDER BY generation_id")?;
            let ids=q.query_map([job_id],|r|r.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
            ids.iter().map(|id|registration(tx,id)).collect()
        })
    }
    /// Registers at most 128 necessary records / 16 MiB with a durable compare-and-swap cursor.
    /// It does not create an active ledger, events, baseline, or advance the physical checkpoint.
    pub fn register_file_candidate_inputs(
        &self,
        generation: String,
        job_id: String,
        expected_after: i64,
        at_ms: i64,
    ) -> StoreResult<RegistrationReceipt> {
        validate_request_id(&generation)?;
        validate_request_id(&job_id)?;
        EpochMs::new(at_ms)?;
        if expected_after < -1 {
            return Err(ErrorCode::InvalidQuery.into());
        }
        self.write(move|conn| {
            let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let (c,r)=owner(&tx,&generation,&job_id)?;
            if r.after_offset!=expected_after {return Err(ErrorCode::CheckpointConflict.into());}
            if r.materialized {tx.commit()?;return Ok(RegistrationReceipt {after_offset:expected_after,registered_rows:0,complete:true});}
            let mut q=tx.prepare("SELECT observation_id,session_key,normalized_json,payload_fingerprint,byte_offset,byte_end FROM file_candidate_observations WHERE generation_id=?1 AND byte_offset>?2 ORDER BY byte_offset LIMIT 128")?;
            let mut rows=q.query(params![generation,expected_after])?;
            let mut selected=vec![];let mut bytes=0usize;
            while let Some(row)=rows.next()? {
                let observation_id:String=row.get(0)?;
                let session_key:Option<String>=row.get(1)?;
                let encoded:String=row.get(2)?;
                let payload_fingerprint:String=row.get(3)?;
                let next=bytes.checked_add(encoded.len()+observation_id.len()+session_key.as_ref().map_or(0,String::len)+payload_fingerprint.len()+4096).ok_or(ErrorCode::NumericOverflow)?;
                if next>16*1024*1024 {if selected.is_empty(){return Err(ErrorCode::InvalidQuery.into());}break;}
                selected.push((ObservationWrite {observation_id,session_key,record:serde_json::from_str(&encoded)?,payload_fingerprint},row.get::<_,i64>(4)?,row.get::<_,i64>(5)?));bytes=next;
            }
            drop(rows);drop(q);
            let mut after=expected_after;
            for (o,offset,end) in &selected {
                validate_request_id(&o.observation_id)?;
                let (p,_)=position(&o.record);
                if p.byte_offset!=*offset as u64 || p.byte_end!=*end as u64 {return Err(ErrorCode::CheckpointConflict.into());}
                header(&tx,&generation,o)?;
                if let Some(session)=&o.session_key {
                    validate_request_id(session)?;
                    if !tx.query_row("SELECT EXISTS(SELECT 1 FROM file_rebuild_sessions WHERE generation_id=?1 AND session_key=?2)",params![generation,session],|r|r.get::<_,bool>(0))? {return Err(ErrorCode::CheckpointConflict.into());}
                }
                batch::write_observation(&tx,o,&generation,0,c.checkpoint.committed_offset)?;
                after = *offset;
            }
            let more:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM file_candidate_observations WHERE generation_id=?1 AND byte_offset>?2)",params![generation,after],|r|r.get(0))?;
            tx.execute("UPDATE file_rebuild_candidates SET after_offset=?1,materialized=?2,updated_at_ms=?3 WHERE generation_id=?4",params![after,!more,at_ms,generation])?;
            tx.commit()?;Ok(RegistrationReceipt {after_offset:after,registered_rows:selected.len(),complete:!more})
        })
    }
}

#[cfg(test)]
mod tests;
