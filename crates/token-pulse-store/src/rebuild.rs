//! Candidates stay invisible until a validated dependency group is published in one transaction.
mod canonical;
use crate::{
    Database, ErrorCode, StoreResult,
    batch::{
        self, ContextWrite, DerivedRecords, EventWrite, PendingWrite, ProvenanceWrite, StreamWrite,
    },
    jobs,
    rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params},
};
pub use canonical::{CanonicalReplayPlan, PhysicalReplaySequence};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use token_pulse_core::{
    domain::{ACCOUNTING_VERSION, ContentAnchor, PARSER_VERSION},
    jobs::JobScope,
    numeric::{DecimalInt, EpochMs},
    protocol::{JobKind, JobState},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileInput {
    pub file_id: String,
    pub generation_id: String,
    pub current_generation_id: Option<String>,
    pub committed_offset: i64,
    pub checkpoint_revision: i64,
    pub observed_size: i64,
    pub identity_json: String,
    pub anchors_json: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LedgerInput {
    pub session_key: String,
    pub old_ledger_id: String,
    pub candidate_ledger_id: String,
    pub provider: String,
    pub provider_session_id: Option<String>,
    pub parent_key: Option<String>,
    pub parent_provider_id: Option<String>,
    pub created_at_ms: Option<i64>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RebuildManifest {
    pub version: u32,
    pub job_id: String,
    pub parser_version: String,
    pub accounting_version: String,
    pub files: Vec<FileInput>,
    pub ledgers: Vec<LedgerInput>,
}
#[derive(Serialize)]
pub struct CandidateBatch {
    pub job_id: String,
    pub events: Vec<EventWrite>,
    pub streams: Vec<StreamWrite>,
    pub provenance: Vec<ProvenanceWrite>,
    pub pending: Vec<PendingWrite>,
    pub contexts: Vec<ContextWrite>,
}
pub struct ReplayRecord {
    pub observation_id: String,
    pub record: token_pulse_core::domain::NormalizedObservation,
}
pub struct ReplayPath {
    pub path: String,
    pub source_root: String,
    pub source_enabled: bool,
}
fn json<T: serde::de::DeserializeOwned>(s: &str) -> StoreResult<T> {
    serde_json::from_str(s).map_err(|_| ErrorCode::DbCorrupt.into())
}
fn ids(tx: &Transaction<'_>, sql: &str, value: &str) -> StoreResult<Vec<String>> {
    let mut s = tx.prepare(sql)?;
    Ok(s.query_map([value], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?)
}
pub(crate) fn dependency_closure(
    tx: &Transaction<'_>,
    scope: &JobScope,
) -> StoreResult<BTreeSet<String>> {
    let mut selected = BTreeSet::new();
    match scope {
        JobScope::All {} => {
            let mut s = tx.prepare("SELECT session_key FROM sessions")?;
            for row in s.query_map([], |r| r.get::<_, String>(0))? {
                selected.insert(row?);
            }
        }
        JobScope::Sessions { session_keys } => {
            for key in session_keys {
                if !tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM sessions WHERE session_key=?1)",
                    [key],
                    |r| r.get::<_, bool>(0),
                )? {
                    return Err(ErrorCode::InvalidQuery.into());
                }
                selected.insert(key.clone());
            }
        }
        JobScope::Sources { source_ids } => {
            for source in source_ids {
                if !tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM sources WHERE source_id=?1)",
                    [source],
                    |r| r.get::<_, bool>(0),
                )? {
                    return Err(ErrorCode::InvalidQuery.into());
                }
                selected.extend(ids(tx,"SELECT DISTINCT b.session_key FROM file_session_bindings b JOIN file_generations g ON b.file_generation_id=g.file_generation_id JOIN source_files f ON f.file_id=g.file_id WHERE f.source_id=?1",source)?);
            }
        }
    }
    let mut frontier = selected.iter().cloned().collect::<Vec<_>>();
    while let Some(key) = frontier.pop() {
        if selected.len() > 32768 {
            return Err(ErrorCode::InvalidQuery.into());
        }
        let neighbors = ids(
            tx,
            "SELECT DISTINCT other.session_key FROM sessions seed JOIN sessions other ON other.provider=seed.provider AND (other.session_key=seed.parent_key OR other.parent_key=seed.session_key OR (seed.provider_session_id IS NOT NULL AND (other.provider_session_id=seed.provider_session_id OR other.parent_provider_id=seed.provider_session_id)) OR (seed.parent_provider_id IS NOT NULL AND other.provider_session_id=seed.parent_provider_id)) WHERE seed.session_key=?1",
            &key,
        )?;
        for other in neighbors {
            if selected.insert(other.clone()) {
                frontier.push(other);
            }
        }
    }
    Ok(selected)
}
fn input_file(tx: &Transaction<'_>, generation: &str) -> StoreResult<FileInput> {
    Ok(tx.query_row("SELECT f.file_id,g.file_generation_id,f.current_generation_id,g.committed_offset,g.checkpoint_revision,g.observed_size,g.identity_json,g.anchor_json FROM file_generations g JOIN source_files f ON f.file_id=g.file_id WHERE g.file_generation_id=?1",[generation],|r|Ok(FileInput{file_id:r.get(0)?,generation_id:r.get(1)?,current_generation_id:r.get(2)?,committed_offset:r.get(3)?,checkpoint_revision:r.get(4)?,observed_size:r.get(5)?,identity_json:r.get(6)?,anchors_json:r.get(7)?}))?)
}
fn manifest(tx: &Transaction<'_>, job_id: &str) -> StoreResult<RebuildManifest> {
    let value:String=tx.query_row("SELECT input_manifest_json FROM ledger_generations WHERE ledger_id=(SELECT json_extract(resume_json,'$.candidate_ledger_ids[0]') FROM jobs WHERE job_id=?1)",[job_id],|r|r.get(0))?;
    let m: RebuildManifest = json(&value)?;
    if m.version != 1
        || m.job_id != job_id
        || m.parser_version != PARSER_VERSION
        || m.accounting_version != ACCOUNTING_VERSION
    {
        return Err(ErrorCode::CandidateObsolete.into());
    }
    Ok(m)
}
fn fresh(tx: &Transaction<'_>, m: &RebuildManifest) -> StoreResult<()> {
    let expected_sessions = m
        .ledgers
        .iter()
        .map(|l| l.session_key.clone())
        .collect::<BTreeSet<_>>();
    let actual_sessions = dependency_closure(
        tx,
        &JobScope::Sessions {
            session_keys: expected_sessions.iter().cloned().collect(),
        },
    )?;
    if actual_sessions != expected_sessions {
        return Err(ErrorCode::CandidateObsolete.into());
    }
    let mut actual_files = BTreeSet::new();
    for session in &actual_sessions {
        actual_files.extend(ids(
            tx,
            "SELECT file_generation_id FROM file_session_bindings WHERE session_key=?1",
            session,
        )?);
    }
    if actual_files != m.files.iter().map(|f| f.generation_id.clone()).collect() {
        return Err(ErrorCode::CandidateObsolete.into());
    }
    for expected in &m.files {
        if &input_file(tx, &expected.generation_id)? != expected {
            return Err(ErrorCode::CandidateObsolete.into());
        }
    }
    for ledger in &m.ledgers {
        if &input_ledger(tx, &ledger.session_key, ledger.candidate_ledger_id.clone())? != ledger {
            return Err(ErrorCode::CandidateObsolete.into());
        }
    }
    Ok(())
}
fn input_ledger(
    tx: &Transaction<'_>,
    key: &str,
    candidate_ledger_id: String,
) -> StoreResult<LedgerInput> {
    Ok(tx.query_row("SELECT active_ledger_id,provider,provider_session_id,parent_key,parent_provider_id,created_at_ms FROM sessions WHERE session_key=?1",[key],|r|Ok(LedgerInput{session_key:key.into(),old_ledger_id:r.get(0)?,candidate_ledger_id,provider:r.get(1)?,provider_session_id:r.get(2)?,parent_key:r.get(3)?,parent_provider_id:r.get(4)?,created_at_ms:r.get(5)?}))?)
}
fn require_state(tx: &Transaction<'_>, id: &str, expected: JobState) -> StoreResult<()> {
    let j = jobs::load(tx, id)?;
    if j.job.state != expected {
        return Err(if j.job.state == JobState::Cancelling {
            ErrorCode::JobCancelled
        } else {
            ErrorCode::RevisionConflict
        }
        .into());
    }
    Ok(())
}
impl Database {
    pub fn rebuild_has_targets(&self, scope: &JobScope) -> StoreResult<bool> {
        self.snapshot(|tx, _| Ok(!dependency_closure(tx, scope)?.is_empty()))
    }
    pub fn rebuild_file_path(&self, file_id: &str) -> StoreResult<ReplayPath> {
        self.snapshot(|tx,_|Ok(tx.query_row("SELECT f.canonical_path,s.root_path,s.enabled FROM source_files f JOIN sources s ON s.source_id=f.source_id WHERE f.file_id=?1",[file_id],|r|Ok(ReplayPath{path:r.get(0)?,source_root:r.get(1)?,source_enabled:r.get(2)?}))?))
    }
    pub fn replay_records(
        &self,
        job_id: &str,
        session: &str,
        after: Option<(&str, i64)>,
    ) -> StoreResult<Vec<ReplayRecord>> {
        self.snapshot(|tx,_| {
            let m=manifest(tx,job_id)?;fresh(tx,&m)?;
            if !m.ledgers.iter().any(|l|l.session_key==session) {return Err(ErrorCode::InvalidQuery.into());}
            let (generation,offset)=after.unwrap_or(("",-1));
            let mut s=tx.prepare("SELECT observation_id,normalized_json,file_generation_id,byte_end FROM observations WHERE session_key=?1 AND (file_generation_id>?2 OR (file_generation_id=?2 AND byte_offset>?3)) ORDER BY file_generation_id,byte_offset LIMIT 256")?;
            let mut rows=s.query(params![session,generation,offset])?;let mut result=vec![];let mut size=0usize;
            while let Some(row)=rows.next()? {
                let encoded:String=row.get(1)?;let next=size.checked_add(encoded.len()).ok_or(ErrorCode::NumericOverflow)?;
                if next>16*1024*1024 {if result.is_empty() {return Err(ErrorCode::InvalidQuery.into());}break;}
                let generation_id:String=row.get(2)?;let end:i64=row.get(3)?;
                if !m.files.iter().any(|f|f.generation_id==generation_id && end<=f.committed_offset) {return Err(ErrorCode::CandidateObsolete.into());}
                result.push(ReplayRecord{observation_id:row.get(0)?,record:json(&encoded)?});size=next;
            }
            Ok(result)
        })
    }
    pub fn replay_observation_ids(
        &self,
        job_id: &str,
        ids: &[String],
    ) -> StoreResult<Vec<ReplayRecord>> {
        if ids.is_empty() || ids.len() > 256 {
            return Err(ErrorCode::InvalidQuery.into());
        }
        self.snapshot(|tx,_| {
            let m=manifest(tx,job_id)?;fresh(tx,&m)?;
            let mut s=tx.prepare("SELECT o.observation_id,o.normalized_json,o.file_generation_id,o.byte_end,o.session_key FROM json_each(?1) requested JOIN observations o ON o.observation_id=requested.value ORDER BY CAST(requested.key AS INTEGER)")?;
            let mut rows=s.query([serde_json::to_string(ids)?])?;let mut result=vec![];let mut size=0usize;
            while let Some(row)=rows.next()? {
                let encoded:String=row.get(1)?;let next=size.checked_add(encoded.len()).ok_or(ErrorCode::NumericOverflow)?;
                if next>16*1024*1024 {if result.is_empty(){return Err(ErrorCode::InvalidQuery.into());}break;}
                let generation:String=row.get(2)?;let end:i64=row.get(3)?;let session:String=row.get(4)?;
                if !m.ledgers.iter().any(|l|l.session_key==session) || !m.files.iter().any(|f|f.generation_id==generation && end<=f.committed_offset){return Err(ErrorCode::CandidateObsolete.into());}
                result.push(ReplayRecord{observation_id:row.get(0)?,record:json(&encoded)?});size=next;
            }
            if result.is_empty(){return Err(ErrorCode::InvalidQuery.into());}Ok(result)
        })
    }
    pub fn active_session_event_count(&self, session: &str) -> StoreResult<i64> {
        self.snapshot(|tx, _| {
            Ok(tx.query_row(
                "SELECT COUNT(*) FROM active_usage_events WHERE session_key=?1",
                [session],
                |r| r.get(0),
            )?)
        })
    }
    pub fn prepare_rebuild(&self, job_id: String, at_ms: i64) -> StoreResult<RebuildManifest> {
        EpochMs::new(at_ms)?;
        self.write(move|conn| {let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            require_state(&tx,&job_id,JobState::Running)?;let job=jobs::load(&tx,&job_id)?;
            if job.job.kind!=JobKind::Rebuild || !job.checkpoint.candidate_ledger_ids.is_empty() {return Err(ErrorCode::InvalidQuery.into());}
            let closure=dependency_closure(&tx,&job.request.scope)?;
            if closure.is_empty() {return Err(ErrorCode::InvalidQuery.into());}
            let mut files=BTreeMap::new();let mut ledgers=vec![];
            for key in closure {
                let (parser,accounting):(String,String)=tx.query_row("SELECT l.parser_version,l.accounting_version FROM sessions s JOIN ledger_generations l ON l.ledger_id=s.active_ledger_id WHERE s.session_key=?1",[&key],|r|Ok((r.get(0)?,r.get(1)?)))?;
                if parser!=PARSER_VERSION || (accounting!=ACCOUNTING_VERSION && !token_pulse_core::domain::can_upgrade_accounting_version(&accounting)) {return Err(ErrorCode::UnsupportedFormat.into());}
                if tx.query_row("SELECT EXISTS(SELECT 1 FROM ledger_generations WHERE session_key=?1 AND state='candidate')",[&key],|r|r.get::<_,bool>(0))? {return Err(ErrorCode::RevisionConflict.into());}
                let candidate=format!("candidate-{:x}",Sha256::digest(serde_json::to_vec(&(&job_id,&key))?));
                for generation in ids(&tx,"SELECT file_generation_id FROM file_session_bindings WHERE session_key=?1",&key)? {files.insert(generation.clone(),input_file(&tx,&generation)?);}
                ledgers.push(input_ledger(&tx,&key,candidate)?);
            }
            let m=RebuildManifest{version:1,job_id:job_id.clone(),parser_version:PARSER_VERSION.into(),accounting_version:ACCOUNTING_VERSION.into(),files:files.into_values().collect(),ledgers};
            let encoded=serde_json::to_string(&m)?;if encoded.len()>16*1024*1024 {return Err(ErrorCode::InvalidQuery.into());}
            for file in &m.files {let _:Vec<ContentAnchor>=json(&file.anchors_json)?;}
            let reference=serde_json::to_string(&serde_json::json!({"version":1,"manifest_owner":m.ledgers[0].candidate_ledger_id}))?;
            for (index,l) in m.ledgers.iter().enumerate() {tx.execute("INSERT INTO ledger_generations(ledger_id,session_key,state,parser_version,accounting_version,base_data_revision,created_at_ms,input_manifest_json) VALUES(?1,?2,'candidate',?3,?4,(SELECT data_revision FROM app_state),?5,?6)",params![l.candidate_ledger_id,l.session_key,PARSER_VERSION,ACCOUNTING_VERSION,at_ms,if index==0 {&encoded} else {&reference}])?;}
            let mut checkpoint=job.checkpoint;checkpoint.candidate_ledger_ids=m.ledgers.iter().map(|l|l.candidate_ledger_id.clone()).collect();
            tx.execute("UPDATE jobs SET resume_json=?1,updated_at_ms=?2 WHERE job_id=?3",params![serde_json::to_string(&checkpoint)?,at_ms,job_id])?;
            tx.commit()?;Ok(m)
        })
    }
    pub fn get_rebuild_manifest(&self, job_id: &str) -> StoreResult<RebuildManifest> {
        self.snapshot(|tx, _| manifest(tx, job_id))
    }
    pub fn stage_candidate_batch(&self, batch: CandidateBatch) -> StoreResult<()> {
        let rows = batch.events.len()
            + batch.streams.len()
            + batch.provenance.len()
            + batch.pending.len()
            + batch.contexts.len();
        if rows > 2500
            || batch.events.len() > 500
            || batch.pending.len() > 500
            || batch.contexts.len() > 500
        {
            return Err(ErrorCode::InvalidQuery.into());
        }
        if serde_json::to_vec(&batch)?.len() > 16 * 1024 * 1024 {
            return Err(ErrorCode::InvalidQuery.into());
        }
        self.write(move|conn| {let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;require_state(&tx,&batch.job_id,JobState::Running)?;
            let m=manifest(&tx,&batch.job_id)?;fresh(&tx,&m)?;
            let sealed:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM file_usage_cursors c JOIN json_each((SELECT resume_json FROM jobs WHERE job_id=?1),'$.candidate_ledger_ids') owned ON owned.value=c.ledger_id)",[&batch.job_id],|r|r.get(0))?;
            if sealed{return Err(ErrorCode::RevisionConflict.into());}
            let allowed=m.ledgers.iter().map(|l|l.candidate_ledger_id.as_str()).collect::<HashSet<_>>();
            for ledger in batch.events.iter().map(|e|&e.ledger_id).chain(batch.streams.iter().map(|e|&e.ledger_id)).chain(batch.pending.iter().map(|e|&e.ledger_id)).chain(batch.contexts.iter().map(|e|&e.ledger_id)) {
                if !allowed.contains(ledger.as_str()) {return Err(ErrorCode::InvalidQuery.into());}
            }
            for reference in batch.events.iter().map(|e|&e.origin_observation_id).chain(batch.streams.iter().map(|e|&e.observation_id)).chain(batch.pending.iter().map(|e|&e.observation_id)).chain(batch.contexts.iter().map(|e|&e.observation_id)).chain(batch.provenance.iter().map(|e|&e.observation_id)) {
                let (generation,end,kind):(String,i64,String)=tx.query_row("SELECT file_generation_id,byte_end,kind FROM observations WHERE observation_id=?1",[reference],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
                if kind!="usage" || !m.files.iter().any(|f|f.generation_id==generation && end<=f.committed_offset) {return Err(ErrorCode::InvalidQuery.into());}
            }
            let mut unique=HashSet::new();for stream in &batch.streams {if !unique.insert((&stream.ledger_id,&stream.stream_key,&stream.episode_id)) {return Err(ErrorCode::InvalidQuery.into());}
                let revision:Option<i64>=tx.query_row("SELECT state_revision FROM stream_states WHERE ledger_id=?1 AND stream_key=?2 AND episode_id=?3",params![stream.ledger_id,stream.stream_key,stream.episode_id],|r|r.get(0)).optional()?;
                if revision!=stream.expected_state_revision {return Err(ErrorCode::RevisionConflict.into());}
            }
            batch::write_derived(&tx,DerivedRecords{events:&batch.events,streams:&batch.streams,provenance:&batch.provenance,pending:&batch.pending,contexts:&batch.contexts},&allowed)?;tx.commit()?;Ok(())
        })
    }
    pub fn fail_rebuild(&self, job_id: String, code: ErrorCode, at_ms: i64) -> StoreResult<()> {
        EpochMs::new(at_ms)?;
        self.write(move|conn| {let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;let job=jobs::load(&tx,&job_id)?;
            if token_pulse_core::jobs::finished(job.job.state) {return Err(ErrorCode::RevisionConflict.into());}
            for id in job.checkpoint.candidate_ledger_ids {tx.execute("UPDATE ledger_generations SET state='failed' WHERE ledger_id=?1 AND state='candidate'",[id])?;}
            let state=if code==ErrorCode::JobCancelled {"cancelled"} else if code==ErrorCode::JobInterrupted {"interrupted"} else {"failed"};
            tx.execute("UPDATE jobs SET state=?1,error_code=?2,updated_at_ms=?3 WHERE job_id=?4",params![state,code.to_string(),at_ms,job_id])?;tx.commit()?;Ok(())
        })
    }
    pub fn validate_candidate(&self, job_id: &str) -> StoreResult<()> {
        self.snapshot(|tx, _| {
            require_state(tx, job_id, JobState::Validating)?;
            let m = manifest(tx, job_id)?;
            fresh(tx, &m)?;
            validate(tx, &m)
        })
    }
    pub fn publish_candidate(&self, job_id: String, at_ms: i64) -> StoreResult<i64> {
        EpochMs::new(at_ms)?;
        self.write(move |conn| publish(conn, &job_id, at_ms, || Ok(())))
    }
}
fn validate(tx: &Transaction<'_>, m: &RebuildManifest) -> StoreResult<()> {
    for ledger in &m.ledgers {
        let state: String = tx.query_row(
            "SELECT state FROM ledger_generations WHERE ledger_id=?1",
            [&ledger.candidate_ledger_id],
            |r| r.get(0),
        )?;
        if state != "candidate" {
            return Err(ErrorCode::CandidateObsolete.into());
        }
        if canonical::alias_target(tx, &m.job_id, &ledger.session_key)?.is_some() {
            let nonempty:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM usage_events WHERE ledger_id=?1 UNION ALL SELECT 1 FROM pending_usage WHERE ledger_id=?1 UNION ALL SELECT 1 FROM stream_states WHERE ledger_id=?1 UNION ALL SELECT 1 FROM context_snapshots WHERE ledger_id=?1)",[&ledger.candidate_ledger_id],|r|r.get(0))?;
            if nonempty {
                return Err(ErrorCode::InvalidUsage.into());
            }
            continue;
        }
        let proof_count:Option<String>=tx.query_row("SELECT json_extract(evidence_json,'$.record_count') FROM candidate_session_aliases WHERE job_id=?1 AND canonical_session_key=?2 LIMIT 1",params![m.job_id,ledger.session_key],|r|r.get(0)).optional()?;
        if let Some(expected) = proof_count {
            let expected: i64 = expected.parse().map_err(|_| ErrorCode::DbCorrupt)?;
            let (count,min,max):(i64,Option<i64>,Option<i64>)=tx.query_row("SELECT COUNT(*),MIN(ordinal),MAX(ordinal) FROM canonical_usage_sequence WHERE ledger_id=?1",[&ledger.candidate_ledger_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
            let sealed:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM file_usage_cursors WHERE ledger_id=?1 AND state='aligned')",[&ledger.candidate_ledger_id],|r|r.get(0))?;
            if count != expected || min != Some(0) || max != Some(expected - 1) || !sealed {
                return Err(ErrorCode::InvalidUsage.into());
            }
        }
        let missing:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM observations o WHERE (o.session_key=?1 OR o.session_key IN (SELECT alias_session_key FROM candidate_session_aliases WHERE job_id=?3 AND canonical_session_key=?1)) AND o.kind='usage' AND NOT EXISTS(SELECT 1 FROM usage_events e WHERE e.ledger_id=?2 AND e.origin_observation_id=o.observation_id) AND NOT EXISTS(SELECT 1 FROM pending_usage p WHERE p.ledger_id=?2 AND p.observation_id=o.observation_id))",params![ledger.session_key,ledger.candidate_ledger_id,m.job_id],|r|r.get(0))?;
        let overlap:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM usage_events e JOIN pending_usage p ON p.observation_id=e.origin_observation_id AND p.ledger_id=e.ledger_id WHERE e.ledger_id=?1 AND p.kind<>'unattributed')",[&ledger.candidate_ledger_id],|r|r.get(0))?;
        if missing || overlap {
            return Err(ErrorCode::InvalidUsage.into());
        }
        // Exact aggregate validation also rejects an overflow before any active pointer changes.
        let _: Option<String> = tx.query_row(
            "SELECT sum_token_decimal(total_tokens) FROM usage_events WHERE ledger_id=?1",
            [&ledger.candidate_ledger_id],
            |r| r.get(0),
        )?;
    }
    Ok(())
}
fn publish(
    conn: &mut rusqlite::Connection,
    job_id: &str,
    at_ms: i64,
    mut before_commit: impl FnMut() -> StoreResult<()>,
) -> StoreResult<i64> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    require_state(&tx, job_id, JobState::Publishing)?;
    let m = manifest(&tx, job_id)?;
    fresh(&tx, &m)?;
    validate(&tx, &m)?;
    let revision: i64 = tx.query_row("SELECT data_revision FROM app_state", [], |r| r.get(0))?;
    let next = revision.checked_add(1).ok_or(ErrorCode::NumericOverflow)?;
    for ledger in &m.ledgers {
        let total = |id: &str| -> StoreResult<(i64, Option<String>)> {
            Ok(tx.query_row("SELECT COUNT(*),sum_token_decimal(total_tokens) FROM usage_events WHERE ledger_id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?)))?)
        };
        let (old_count, old_total) = total(&ledger.old_ledger_id)?;
        let (new_count, new_total) = total(&ledger.candidate_ledger_id)?;
        tx.execute(
            "UPDATE ledger_generations SET state='retired' WHERE ledger_id=?1 AND state='active'",
            [&ledger.old_ledger_id],
        )?;
        tx.execute("UPDATE ledger_generations SET state='active',activated_at_ms=?1 WHERE ledger_id=?2 AND state='candidate'",params![at_ms,ledger.candidate_ledger_id])?;
        tx.execute("UPDATE sessions SET active_ledger_id=?1,last_activity_ms=(SELECT MAX(occurred_at_ms) FROM usage_events WHERE ledger_id=?1) WHERE session_key=?2",params![ledger.candidate_ledger_id,ledger.session_key])?;
        let difference = serde_json::json!({"old_event_count":old_count.to_string(),"new_event_count":new_count.to_string(),"old_total_tokens":old_total,"new_total_tokens":new_total});
        let audit = format!(
            "audit-{:x}",
            Sha256::digest(serde_json::to_vec(&(job_id, &ledger.session_key))?)
        );
        tx.execute("INSERT INTO rebuild_audits(audit_id,job_id,session_key,old_ledger_id,new_ledger_id,reason,difference_json,committed_data_revision,created_at_ms) VALUES(?1,?2,?3,?4,?5,'rebuild',?6,?7,?8)",params![audit,job_id,ledger.session_key,ledger.old_ledger_id,ledger.candidate_ledger_id,serde_json::to_string(&difference)?,next,at_ms])?;
    }
    canonical::publish_aliases(&tx, &m)?;
    tx.execute("UPDATE app_state SET data_revision=?1", [next])?;
    let job = jobs::load(&tx, job_id)?;
    let mut progress: token_pulse_core::jobs::JobProgress = json(&tx.query_row(
        "SELECT progress_json FROM jobs WHERE job_id=?1",
        [job_id],
        |r| r.get::<_, String>(0),
    )?)?;
    progress.phase = "complete".into();
    progress.accepted_events = DecimalInt::from_nonnegative(m.ledgers.iter().try_fold(
        0i128,
        |n, l| -> StoreResult<i128> {
            let count: i64 = tx.query_row(
                "SELECT COUNT(*) FROM usage_events WHERE ledger_id=?1",
                [&l.candidate_ledger_id],
                |r| r.get(0),
            )?;
            n.checked_add(i128::from(count))
                .ok_or(ErrorCode::NumericOverflow.into())
        },
    )?)?;
    progress.pending_observations = DecimalInt::from_nonnegative(m.ledgers.iter().try_fold(
        0i128,
        |n, l| -> StoreResult<i128> {
            let count: i64 = tx.query_row(
                "SELECT COUNT(*) FROM pending_usage WHERE ledger_id=?1 AND kind='pending'",
                [&l.candidate_ledger_id],
                |r| r.get(0),
            )?;
            n.checked_add(i128::from(count))
                .ok_or(ErrorCode::NumericOverflow.into())
        },
    )?)?;
    tx.execute("UPDATE jobs SET state='succeeded',error_code=NULL,progress_json=?1,updated_at_ms=?2 WHERE job_id=?3",params![serde_json::to_string(&progress)?,at_ms,job.job.job_id])?;
    before_commit()?;
    tx.commit()?;
    Ok(next)
}

#[cfg(test)]
mod tests;
