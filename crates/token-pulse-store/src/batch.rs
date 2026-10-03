//! A prepared accounting batch becomes durable in exactly one transaction.
use crate::{Database, ErrorCode, StoreResult};
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use token_pulse_core::domain::{
    ACCOUNTING_VERSION, ContentAnchor, EffectiveMetadata, NormalizedObservation,
    ObservationQuality, PARSER_VERSION, ReaderContext, UsageVector,
};

#[derive(Debug, Clone)]
pub struct LedgerExpectation {
    pub session_key: String,
    pub ledger_id: String,
}
#[derive(Debug, Clone)]
pub struct ObservationWrite {
    pub observation_id: String,
    pub session_key: Option<String>,
    pub record: NormalizedObservation,
    pub payload_fingerprint: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct EventWrite {
    pub event_id: String,
    pub ledger_id: String,
    pub origin_observation_id: String,
    pub occurred_at_ms: i64,
    pub semantic_key: Option<String>,
    pub episode_id: String,
    pub model: Option<String>,
    pub project_id: Option<String>,
    pub turn_id: Option<String>,
    pub usage: UsageVector,
    pub calculation_method: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct StreamWrite {
    pub ledger_id: String,
    pub stream_key: String,
    pub episode_id: String,
    pub baseline: UsageVector,
    pub observation_id: String,
    pub quality: ObservationQuality,
    pub expected_state_revision: Option<i64>,
}
#[derive(Debug, Clone, Serialize)]
pub struct ProvenanceWrite {
    pub event_id: String,
    pub observation_id: String,
    pub relation: String,
}
#[derive(Debug, Clone, Serialize, Default)]
pub struct PendingEvidence {
    pub candidate_stream_keys: Vec<String>,
    pub parent_session_key: Option<String>,
    pub related_observation_ids: Vec<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct PendingWrite {
    pub pending_id: String,
    pub ledger_id: String,
    pub observation_id: String,
    pub quality: ObservationQuality,
    pub reason_code: String,
    pub vector: Option<UsageVector>,
    pub evidence: PendingEvidence,
}
#[derive(Debug, Clone, Serialize)]
pub struct ContextWrite {
    pub context_id: String,
    pub ledger_id: String,
    pub observation_id: String,
    pub observed_at_ms: i64,
    pub model: Option<String>,
    pub usage: UsageVector,
    pub model_context_window: Option<i64>,
    pub quality: ObservationQuality,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticMetadata {
    pub parser_version: Option<String>,
    pub expected_revision: Option<i64>,
    pub actual_revision: Option<i64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticWrite {
    pub diagnostic_id: String,
    pub source_id: Option<String>,
    pub file_generation_id: Option<String>,
    pub byte_offset: Option<i64>,
    pub session_key: Option<String>,
    pub code: ErrorCode,
    pub severity: String,
    pub metadata: DiagnosticMetadata,
    pub dedup_key: String,
    pub observed_at_ms: i64,
}
#[derive(Debug, Clone)]
pub enum CanonicalStep {
    Append {
        observation_id: String,
    },
    Copy {
        observation_id: String,
        origin_observation_id: String,
    },
}
#[derive(Debug, Clone)]
pub struct CanonicalProgressWrite {
    pub ledger_id: String,
    pub expected_cursor: i64,
    pub expected_length: i64,
    pub steps: Vec<CanonicalStep>,
    pub requires_rebuild: bool,
}
#[derive(Debug, Clone)]
pub struct WriteBatch {
    pub file_generation_id: String,
    pub expected_offset: i64,
    pub expected_checkpoint_revision: i64,
    pub next_offset: i64,
    pub observed_size: i64,
    pub anchors: Vec<ContentAnchor>,
    pub reader_context: ReaderContext,
    pub ledgers: Vec<LedgerExpectation>,
    pub observations: Vec<ObservationWrite>,
    pub events: Vec<EventWrite>,
    pub streams: Vec<StreamWrite>,
    pub provenance: Vec<ProvenanceWrite>,
    pub pending: Vec<PendingWrite>,
    pub contexts: Vec<ContextWrite>,
    pub diagnostics: Vec<DiagnosticWrite>,
    pub canonical: Vec<CanonicalProgressWrite>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommitReceipt {
    pub data_revision: i64,
    pub checkpoint_revision: i64,
    pub usage_changed: bool,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum CommitStage {
    Observations,
    Events,
    Streams,
    Checkpoint,
    BeforeCommit,
    AfterCommit,
}

impl Database {
    pub fn commit(&self, batch: WriteBatch) -> StoreResult<CommitReceipt> {
        self.write(move |conn| commit_batch(conn, batch, |_| Ok(())))
    }
}

fn session_for_ledger(tx: &Transaction<'_>, ledger: &str) -> StoreResult<String> {
    Ok(tx.query_row(
        "SELECT session_key FROM ledger_generations WHERE ledger_id=?1",
        [ledger],
        |r| r.get(0),
    )?)
}
pub(crate) fn same_session(
    tx: &Transaction<'_>,
    ledger: &str,
    observation: &str,
) -> StoreResult<()> {
    let session: String = tx.query_row(
        "SELECT session_key FROM observations WHERE observation_id=?1",
        [observation],
        |r| r.get(0),
    )?;
    let owner = session_for_ledger(tx, ledger)?;
    if session != owner && !tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM candidate_session_aliases a JOIN jobs j ON j.job_id=a.job_id JOIN ledger_generations l ON l.ledger_id=?1 JOIN json_each(j.resume_json,'$.candidate_ledger_ids') owned ON owned.value=l.ledger_id WHERE a.alias_session_key=?2 AND a.canonical_session_key=?3 AND j.state='running' AND l.state='candidate' AND l.session_key=a.canonical_session_key)",
        params![ledger, session, owner], |r|r.get::<_,bool>(0),
    )? {
        return Err(ErrorCode::CheckpointConflict.into());
    }
    Ok(())
}
fn project(tx: &Transaction<'_>, metadata: &EffectiveMetadata) -> StoreResult<Option<String>> {
    let Some(cwd) = &metadata.cwd else {
        return Ok(None);
    };
    // Windows drive/UNC paths compare case-insensitively. Unix WSL cwd preserves case.
    let canonical = if cwd.as_bytes().get(1) == Some(&b':') || cwd.starts_with("\\\\") {
        cwd.replace('\\', "/").to_lowercase()
    } else {
        cwd.clone()
    };
    let found = tx
        .query_row(
            "SELECT project_id FROM projects WHERE canonical_cwd=?1",
            [&canonical],
            |r| r.get(0),
        )
        .optional()?;
    if found.is_some() {
        return Ok(found);
    }
    use sha2::{Digest, Sha256};
    let id = format!("project-{:x}", Sha256::digest(canonical.as_bytes()));
    let name = cwd
        .trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(cwd);
    tx.execute("INSERT INTO projects(project_id,canonical_cwd,display_name,normalization_version) VALUES(?1,?2,?3,1)", params![id,canonical,name])?;
    Ok(Some(id))
}
pub(crate) fn write_observation(
    tx: &Transaction<'_>,
    observation: &ObservationWrite,
    generation: &str,
    lower: i64,
    upper: i64,
) -> StoreResult<()> {
    let (position, kind, time, metadata, record_session) = match &observation.record {
        NormalizedObservation::SessionMetadata {
            physical_position,
            created_at_ms,
            metadata,
            ..
        } => (
            physical_position,
            "session_meta",
            *created_at_ms,
            metadata,
            observation.session_key.as_deref(),
        ),
        NormalizedObservation::TurnMetadata {
            physical_position,
            session_key,
            metadata,
        } => (
            physical_position,
            "turn_meta",
            None,
            metadata,
            Some(session_key.as_str()),
        ),
        NormalizedObservation::Usage(u) => (
            &u.physical_position,
            "usage",
            u.event_time_ms,
            &u.effective_metadata,
            Some(u.session_key.as_str()),
        ),
        NormalizedObservation::Context {
            physical_position,
            session_key,
            observed_at_ms,
            metadata,
            ..
        } => (
            physical_position,
            "context",
            *observed_at_ms,
            metadata,
            Some(session_key.as_str()),
        ),
    };
    position.validate()?;
    if position.file_generation_id != generation
        || position.byte_offset < (lower as u64)
        || position.byte_end > (upper as u64)
        || observation.session_key.as_deref() != record_session
    {
        return Err(ErrorCode::CheckpointConflict.into());
    }
    let project_id = project(tx, metadata)?;
    let (stable, stream) = match &observation.record {
        NormalizedObservation::Usage(u) => (
            u.request_identity
                .as_ref()
                .map(serde_json::to_string)
                .transpose()?,
            u.stream_hint.clone(),
        ),
        _ => (None, None),
    };
    tx.execute("INSERT INTO observations(observation_id,file_generation_id,byte_offset,byte_end,session_key,kind,observed_at_ms,stable_record_id,turn_id,stream_hint,model,project_id,normalized_json,payload_fingerprint,format_version) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",params![observation.observation_id,generation,position.byte_offset as i64,position.byte_end as i64,observation.session_key,kind,time,stable,metadata.turn_id,stream,metadata.model,project_id,serde_json::to_string(&observation.record)?,observation.payload_fingerprint,PARSER_VERSION])?;
    if let Some(session) = &observation.session_key {
        tx.execute("INSERT INTO file_session_bindings(file_generation_id,session_key,first_offset,identity_evidence) VALUES(?1,?2,?3,'adapter') ON CONFLICT(file_generation_id,session_key) DO NOTHING",params![generation,session,position.byte_offset as i64])?;
    }
    Ok(())
}

fn commit_batch(
    conn: &mut rusqlite::Connection,
    batch: WriteBatch,
    mut at: impl FnMut(CommitStage) -> StoreResult<()>,
) -> StoreResult<CommitReceipt> {
    if batch.observations.len() > 500
        || batch.anchors.iter().any(|a| {
            a.byte_offset
                .checked_add(u64::from(a.byte_length))
                .is_none_or(|end| end > batch.next_offset.max(0) as u64)
                || a.sha256.len() != 64
                || !a
                    .sha256
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        })
    {
        return Err(ErrorCode::InvalidQuery.into());
    }
    let necessary_bytes =
        batch
            .observations
            .iter()
            .try_fold(0usize, |sum, o| -> StoreResult<usize> {
                let length = serde_json::to_vec(&o.record)?
                    .len()
                    .checked_add(o.payload_fingerprint.len())
                    .ok_or(ErrorCode::NumericOverflow)?;
                sum.checked_add(length)
                    .ok_or_else(|| ErrorCode::NumericOverflow.into())
            })?;
    if necessary_bytes > 16 * 1024 * 1024 {
        return Err(ErrorCode::InvalidQuery.into());
    }
    if batch.expected_offset < 0
        || batch.next_offset < batch.expected_offset
        || batch.next_offset > batch.observed_size
        || batch.expected_checkpoint_revision < 0
    {
        return Err(ErrorCode::InvalidQuery.into());
    }
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let frozen: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM file_read_candidates c JOIN file_rebuild_candidates r ON r.generation_id=c.generation_id JOIN jobs j ON j.job_id=r.job_id WHERE json_extract(c.base_json,'$.generation_id')=?1 AND c.state IN ('reading','ready','claimed') AND j.state IN ('running','validating','publishing','cancelling'))",[&batch.file_generation_id],|r|r.get(0))?;
    if frozen {
        return Err(ErrorCode::CheckpointConflict.into());
    }
    let checkpoint:Option<(i64,i64)>=tx.query_row("SELECT g.committed_offset,g.checkpoint_revision FROM file_generations g JOIN source_files f ON f.current_generation_id=g.file_generation_id WHERE g.file_generation_id=?1 AND g.state='current'", [&batch.file_generation_id], |r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    if checkpoint != Some((batch.expected_offset, batch.expected_checkpoint_revision)) {
        return Err(ErrorCode::CheckpointConflict.into());
    }
    for expected in &batch.ledgers {
        let active:Option<(String,String,String)>=tx.query_row("SELECT s.active_ledger_id,g.parser_version,g.accounting_version FROM sessions s JOIN ledger_generations g ON s.active_ledger_id=g.ledger_id WHERE s.session_key=?1 AND g.state='active'", [&expected.session_key], |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        if active.as_ref().map(|v| v.0.as_str()) != Some(expected.ledger_id.as_str()) {
            return Err(ErrorCode::CheckpointConflict.into());
        }
        if active.is_some_and(|(_, parser, accounting)| {
            parser != PARSER_VERSION || accounting != ACCOUNTING_VERSION
        }) {
            return Err(ErrorCode::CandidateObsolete.into());
        }
    }
    if batch.observations.is_empty()
        && batch.events.is_empty()
        && batch.streams.is_empty()
        && batch.provenance.is_empty()
        && batch.pending.is_empty()
        && batch.contexts.is_empty()
        && batch.diagnostics.is_empty()
        && batch.canonical.is_empty()
        && batch.next_offset == batch.expected_offset
    {
        let (size,anchors,context):(i64,String,String)=tx.query_row("SELECT observed_size,anchor_json,reader_context_json FROM file_generations WHERE file_generation_id=?1",[&batch.file_generation_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
        let anchors: Vec<ContentAnchor> = serde_json::from_str(&anchors)?;
        let context: ReaderContext = serde_json::from_str(&context)?;
        if size == batch.observed_size
            && anchors == batch.anchors
            && context == batch.reader_context
        {
            let data_revision: i64 =
                tx.query_row("SELECT data_revision FROM app_state", [], |r| r.get(0))?;
            tx.commit()?;
            return Ok(CommitReceipt {
                data_revision,
                checkpoint_revision: batch.expected_checkpoint_revision,
                usage_changed: false,
            });
        }
    }
    let allowed: std::collections::HashSet<&str> =
        batch.ledgers.iter().map(|l| l.ledger_id.as_str()).collect();
    for ledger in batch
        .events
        .iter()
        .map(|v| &v.ledger_id)
        .chain(batch.streams.iter().map(|v| &v.ledger_id))
        .chain(batch.pending.iter().map(|v| &v.ledger_id))
        .chain(batch.contexts.iter().map(|v| &v.ledger_id))
    {
        if !allowed.contains(ledger.as_str()) {
            return Err(ErrorCode::CheckpointConflict.into());
        }
    }
    for stream in &batch.streams {
        let revision:Option<i64>=tx.query_row("SELECT state_revision FROM stream_states WHERE ledger_id=?1 AND stream_key=?2 AND episode_id=?3",params![stream.ledger_id,stream.stream_key,stream.episode_id],|r|r.get(0)).optional()?;
        if revision != stream.expected_state_revision {
            return Err(ErrorCode::CheckpointConflict.into());
        }
    }
    let mut stream_keys = std::collections::HashSet::new();
    if batch
        .streams
        .iter()
        .any(|s| !stream_keys.insert((&s.ledger_id, &s.stream_key, &s.episode_id)))
    {
        return Err(ErrorCode::InvalidQuery.into());
    }
    for observation in &batch.observations {
        if let Some(session) = &observation.session_key {
            if !batch
                .ledgers
                .iter()
                .any(|ledger| &ledger.session_key == session)
            {
                return Err(ErrorCode::CheckpointConflict.into());
            }
        }
        write_observation(
            &tx,
            observation,
            &batch.file_generation_id,
            batch.expected_offset,
            batch.next_offset,
        )?;
    }
    at(CommitStage::Observations)?;
    write_derived_with_hook(
        &tx,
        DerivedRecords {
            events: &batch.events,
            streams: &batch.streams,
            provenance: &batch.provenance,
            pending: &batch.pending,
            contexts: &batch.contexts,
        },
        &allowed,
        &mut at,
    )?;
    super::canonical_progress::write(&tx, &batch, &allowed)?;
    for diagnostic in &batch.diagnostics {
        tx.execute("INSERT INTO diagnostics(diagnostic_id,source_id,file_generation_id,byte_offset,session_key,code,severity,metadata_json,dedup_key,first_seen_at_ms,last_seen_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?10) ON CONFLICT(dedup_key) DO UPDATE SET occurrences=occurrences+1,last_seen_at_ms=excluded.last_seen_at_ms",params![diagnostic.diagnostic_id,diagnostic.source_id,diagnostic.file_generation_id,diagnostic.byte_offset,diagnostic.session_key,diagnostic.code.to_string(),diagnostic.severity,serde_json::to_string(&diagnostic.metadata)?,diagnostic.dedup_key,diagnostic.observed_at_ms])?;
    }
    at(CommitStage::Streams)?;
    let next_checkpoint = batch
        .expected_checkpoint_revision
        .checked_add(1)
        .ok_or(ErrorCode::NumericOverflow)?;
    tx.execute("UPDATE file_generations SET committed_offset=?1,checkpoint_revision=?2,observed_size=?3,anchor_json=?4,reader_context_json=?5 WHERE file_generation_id=?6",params![batch.next_offset,next_checkpoint,batch.observed_size,serde_json::to_string(&batch.anchors)?,serde_json::to_string(&batch.reader_context)?,batch.file_generation_id])?;
    at(CommitStage::Checkpoint)?;
    let changed = !batch.observations.is_empty()
        || !batch.events.is_empty()
        || !batch.provenance.is_empty()
        || !batch.pending.is_empty()
        || !batch.contexts.is_empty()
        || !batch.diagnostics.is_empty()
        || !batch.canonical.is_empty();
    let mut revision: i64 = tx.query_row(
        "SELECT data_revision FROM app_state WHERE singleton=1",
        [],
        |r| r.get(0),
    )?;
    if changed {
        revision = revision.checked_add(1).ok_or(ErrorCode::NumericOverflow)?;
        tx.execute(
            "UPDATE app_state SET data_revision=?1 WHERE singleton=1",
            [revision],
        )?;
    }
    at(CommitStage::BeforeCommit)?;
    tx.commit()?;
    at(CommitStage::AfterCommit)?;
    Ok(CommitReceipt {
        data_revision: revision,
        checkpoint_revision: next_checkpoint,
        usage_changed: changed,
    })
}

#[cfg(test)]
mod cache_write_tests;
#[cfg(test)]
pub(crate) mod tests;

pub(crate) struct DerivedRecords<'a> {
    pub events: &'a [EventWrite],
    pub streams: &'a [StreamWrite],
    pub provenance: &'a [ProvenanceWrite],
    pub pending: &'a [PendingWrite],
    pub contexts: &'a [ContextWrite],
}
fn write_derived_with_hook(
    tx: &Transaction<'_>,
    derived: DerivedRecords<'_>,
    allowed: &std::collections::HashSet<&str>,
    mut at: impl FnMut(CommitStage) -> StoreResult<()>,
) -> StoreResult<()> {
    for event in derived.events {
        same_session(tx, &event.ledger_id, &event.origin_observation_id)?;
        let total = event
            .usage
            .validated_total()?
            .ok_or(ErrorCode::InvalidUsage)?;
        if total == 0 {
            return Err(ErrorCode::InvalidUsage.into());
        }
        let (observed_model, observed_project, observed_time): (
            Option<String>,
            Option<String>,
            Option<i64>,
        ) = tx.query_row(
            "SELECT model,project_id,observed_at_ms FROM observations WHERE observation_id=?1",
            [&event.origin_observation_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        if observed_model != event.model
            || (event.project_id.is_some() && observed_project != event.project_id)
            || observed_time != Some(event.occurred_at_ms)
        {
            return Err(ErrorCode::CheckpointConflict.into());
        }
        tx.execute("INSERT INTO usage_events(event_id,ledger_id,origin_observation_id,occurred_at_ms,semantic_key,episode_id,model,project_id,turn_id,input_tokens_total,cached_input_tokens,output_tokens_total,reasoning_output_tokens,source_total_tokens,total_tokens,calculation_method,quality_json,cache_write_input_tokens) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,'[\"confirmed\"]',?17)",params![event.event_id,event.ledger_id,event.origin_observation_id,event.occurred_at_ms,event.semantic_key,event.episode_id,event.model,observed_project,event.turn_id,event.usage.input_total,event.usage.cached_input,event.usage.output_total,event.usage.reasoning_output,event.usage.reported_total,total,event.calculation_method,event.usage.cache_write_input])?;
        tx.execute(
            "INSERT INTO event_provenance VALUES(?1,?2,'origin')",
            params![event.event_id, event.origin_observation_id],
        )?;
        tx.execute("UPDATE sessions SET last_activity_ms=MAX(COALESCE(last_activity_ms,?1),?1) WHERE active_ledger_id=?2",params![event.occurred_at_ms,event.ledger_id])?;
    }
    for provenance in derived.provenance {
        let ledger: String = tx.query_row(
            "SELECT ledger_id FROM usage_events WHERE event_id=?1",
            [&provenance.event_id],
            |r| r.get(0),
        )?;
        if !allowed.contains(ledger.as_str()) {
            return Err(ErrorCode::CheckpointConflict.into());
        }
        same_session(tx, &ledger, &provenance.observation_id)?;
        tx.execute("INSERT INTO event_provenance(event_id,observation_id,relation) VALUES(?1,?2,?3) ON CONFLICT(event_id,observation_id) DO NOTHING",params![provenance.event_id,provenance.observation_id,provenance.relation])?;
    }
    for pending in derived.pending {
        same_session(tx, &pending.ledger_id, &pending.observation_id)?;
        let kind = match pending.quality {
            ObservationQuality::Pending => "pending",
            ObservationQuality::Inherited => "inherited",
            ObservationQuality::Duplicate => "duplicate",
            ObservationQuality::Unattributed => "unattributed",
            ObservationQuality::Confirmed => return Err(ErrorCode::InvalidQuery.into()),
        };
        tx.execute(
            "INSERT INTO pending_usage VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![
                pending.pending_id,
                pending.ledger_id,
                pending.observation_id,
                kind,
                pending.reason_code,
                pending
                    .vector
                    .map(|v| serde_json::to_string(&v))
                    .transpose()?,
                serde_json::to_string(&pending.evidence)?
            ],
        )?;
    }
    for context in derived.contexts {
        same_session(tx, &context.ledger_id, &context.observation_id)?;
        let total = context.usage.validated_total()?;
        tx.execute(
            "INSERT INTO context_snapshots VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                context.context_id,
                context.ledger_id,
                context.observation_id,
                context.observed_at_ms,
                context.model,
                total,
                context.model_context_window,
                serde_json::to_string(&context.usage)?,
                serde_json::to_string(&context.quality)?
            ],
        )?;
    }
    at(CommitStage::Events)?;
    for stream in derived.streams {
        stream.baseline.validated_total()?;
        same_session(tx, &stream.ledger_id, &stream.observation_id)?;
        let next_revision = stream
            .expected_state_revision
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(ErrorCode::NumericOverflow)?;
        tx.execute("INSERT INTO stream_states(ledger_id,stream_key,episode_id,baseline_json,last_observation_id,lineage_quality,state_revision) VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(ledger_id,stream_key,episode_id) DO UPDATE SET baseline_json=excluded.baseline_json,last_observation_id=excluded.last_observation_id,lineage_quality=excluded.lineage_quality,state_revision=excluded.state_revision",params![stream.ledger_id,stream.stream_key,stream.episode_id,serde_json::to_string(&stream.baseline)?,stream.observation_id,serde_json::to_string(&stream.quality)?,next_revision])?;
        tx.execute("INSERT INTO stream_frontiers(ledger_id,stream_key,episode_id) VALUES(?1,?2,?3) ON CONFLICT(ledger_id,stream_key) DO UPDATE SET episode_id=excluded.episode_id WHERE (SELECT o.rowid FROM observations o WHERE o.observation_id=?4)>=(SELECT o.rowid FROM stream_states st JOIN observations o ON o.observation_id=st.last_observation_id WHERE st.ledger_id=stream_frontiers.ledger_id AND st.stream_key=stream_frontiers.stream_key AND st.episode_id=stream_frontiers.episode_id)",params![stream.ledger_id,stream.stream_key,stream.episode_id,stream.observation_id])?;
    }
    Ok(())
}

pub(crate) fn write_derived(
    tx: &Transaction<'_>,
    derived: DerivedRecords<'_>,
    allowed: &std::collections::HashSet<&str>,
) -> StoreResult<()> {
    write_derived_with_hook(tx, derived, allowed, |_| Ok(()))
}
