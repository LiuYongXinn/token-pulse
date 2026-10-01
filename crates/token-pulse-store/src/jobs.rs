//! Persistent job state machine. All competing worker/cancellation changes use the Writer.
use crate::{
    Database, ErrorCode, StoreResult,
    rusqlite::{OptionalExtension, TransactionBehavior, params},
};
use rusqlite::Connection;
use token_pulse_core::{
    error::AppError,
    jobs::*,
    numeric::EpochMs,
    protocol::{Job, JobKind, JobState, validate_request_id},
};

pub struct StoredJob {
    pub job: Job,
    pub request: JobRequest,
    pub checkpoint: JobCheckpoint,
}
pub struct JobAdvance {
    pub expected: JobState,
    pub next: JobState,
    pub progress: JobProgress,
    pub checkpoint: JobCheckpoint,
    pub error: Option<ErrorCode>,
    pub at_ms: i64,
}
fn text<T: serde::Serialize>(value: &T) -> StoreResult<String> {
    Ok(serde_json::to_value(value)?
        .as_str()
        .ok_or(ErrorCode::InvalidQuery)?
        .to_owned())
}
fn decoded<T: serde::de::DeserializeOwned>(value: &str) -> StoreResult<T> {
    serde_json::from_str(value).map_err(|_| ErrorCode::DbCorrupt.into())
}
pub(crate) fn load(conn: &Connection, id: &str) -> StoreResult<StoredJob> {
    let row=conn.query_row("SELECT kind,state,request_key,scope_json,progress_json,resume_json,error_code,created_at_ms,updated_at_ms FROM jobs WHERE job_id=?1",[id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?,r.get::<_,String>(5)?,r.get::<_,Option<String>>(6)?,r.get::<_,i64>(7)?,r.get::<_,i64>(8)?))).optional()?.ok_or(ErrorCode::InvalidQuery)?;
    let (kind, state, key, scope, progress, checkpoint, error, created, updated) = row;
    let kind: JobKind = decoded(&serde_json::to_string(&kind)?)?;
    let state: JobState = decoded(&serde_json::to_string(&state)?)?;
    let progress: JobProgress = decoded(&progress)?;
    progress
        .validate_after(&JobProgress::default())
        .map_err(|_| ErrorCode::DbCorrupt)?;
    let checkpoint: JobCheckpoint = decoded(&checkpoint)?;
    checkpoint.validate().map_err(|_| ErrorCode::DbCorrupt)?;
    let mut request = JobRequest {
        kind,
        scope: decoded(&scope)?,
        request_key: key,
    };
    request.validate().map_err(|_| ErrorCode::DbCorrupt)?;
    let error = error
        .map(|e| decoded(&serde_json::to_string(&e)?))
        .transpose()?
        .map(|code| {
            let mut e = AppError::new(code, format!("job:{id}"));
            e.job_id = Some(id.into());
            e
        });
    Ok(StoredJob {
        job: Job {
            job_id: id.into(),
            kind,
            state,
            phase: progress.phase,
            discovered_files: progress.discovered_files,
            discovery_complete: progress.discovery_complete,
            processed_files: progress.processed_files,
            processed_bytes: progress.processed_bytes,
            accepted_events: progress.accepted_events,
            pending_observations: progress.pending_observations,
            can_cancel: can_cancel(state),
            error,
            created_at_ms: EpochMs::new(created)?,
            updated_at_ms: EpochMs::new(updated)?,
        },
        request,
        checkpoint,
    })
}
impl Database {
    pub fn create_job(&self, id: String, mut request: JobRequest, at_ms: i64) -> StoreResult<Job> {
        validate_request_id(&id)?;
        request.validate()?;
        EpochMs::new(at_ms)?;
        self.write(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let job = create_in_tx(&tx, &id, &request, at_ms)?;
            tx.commit()?;
            Ok(job)
        })
    }
    pub fn get_job(&self, id: &str) -> StoreResult<StoredJob> {
        validate_request_id(id)?;
        self.snapshot(|tx, _| load(tx, id))
    }
    pub fn next_queued_rebuild(&self) -> StoreResult<Option<Job>> {
        self.snapshot(|tx,_| {let id:Option<String>=tx.query_row("SELECT job_id FROM jobs WHERE state='queued' AND kind='rebuild' ORDER BY created_at_ms,job_id LIMIT 1",[],|r|r.get(0)).optional()?;id.map(|id|load(tx,&id).map(|j|j.job)).transpose()})
    }
    pub fn list_jobs(&self, limit: u32) -> StoreResult<Vec<Job>> {
        if limit == 0 || limit > 100 {
            return Err(ErrorCode::InvalidQuery.into());
        }
        self.snapshot(|tx, _| {
            let mut s = tx.prepare(
                "SELECT job_id FROM jobs ORDER BY created_at_ms DESC,job_id DESC LIMIT ?1",
            )?;
            let ids = s
                .query_map([limit], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            ids.iter().map(|id| load(tx, id).map(|j| j.job)).collect()
        })
    }
    pub fn cancel_job(&self, id: String, at_ms: i64) -> StoreResult<CancelJobResult> {
        validate_request_id(&id)?;
        EpochMs::new(at_ms)?;
        self.write(move|conn| {let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;let old=load(&tx,&id)?.job;
            if old.state==JobState::Publishing {return Ok(CancelJobResult::TooLate);}
            if finished(old.state) {return Ok(CancelJobResult::AlreadyFinished);}
            let next=if old.state==JobState::Queued {JobState::Cancelled} else {JobState::Cancelling};
            tx.execute("UPDATE jobs SET state=?1,cancel_requested=1,updated_at_ms=?2,error_code=?3 WHERE job_id=?4",params![text(&next)?,at_ms,if next==JobState::Cancelled {Some(text(&ErrorCode::JobCancelled)?)} else {None},id])?;
            tx.commit()?;Ok(CancelJobResult::Accepted)
        })
    }
    /// Expected state prevents a late worker write from erasing a pending cancellation.
    pub fn advance_job(&self, id: String, change: JobAdvance) -> StoreResult<Job> {
        let JobAdvance {
            expected,
            next,
            progress,
            checkpoint,
            error,
            at_ms,
        } = change;
        validate_request_id(&id)?;
        EpochMs::new(at_ms)?;
        checkpoint.validate()?;
        if !transition_allowed(expected, next) || ((next == JobState::Failed) != error.is_some()) {
            return Err(ErrorCode::InvalidQuery.into());
        }
        self.write(move|conn| {let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;let old=load(&tx,&id)?;
            if old.job.state!=expected {return Err(if old.job.state==JobState::Cancelling {ErrorCode::JobCancelled} else {ErrorCode::RevisionConflict}.into());}
            let previous:JobProgress=decoded(&tx.query_row("SELECT progress_json FROM jobs WHERE job_id=?1",[&id],|r|r.get::<_,String>(0))?)?;progress.validate_after(&previous)?;
            if checkpoint.batch_position.value()<old.checkpoint.batch_position.value() {return Err(ErrorCode::RevisionConflict.into());}
            let error=error.or(if next==JobState::Cancelled {Some(ErrorCode::JobCancelled)} else {None});
            tx.execute("UPDATE jobs SET state=?1,progress_json=?2,resume_json=?3,error_code=?4,updated_at_ms=?5 WHERE job_id=?6",params![text(&next)?,serde_json::to_string(&progress)?,serde_json::to_string(&checkpoint)?,error.map(|e|text(&e)).transpose()?,at_ms,id])?;
            let result=load(&tx,&id)?.job;tx.commit()?;Ok(result)
        })
    }
    /// Each safe batch persists progress without moving the lifecycle phase.
    pub fn checkpoint_job(
        &self,
        id: String,
        expected: JobState,
        progress: JobProgress,
        checkpoint: JobCheckpoint,
        at_ms: i64,
    ) -> StoreResult<()> {
        validate_request_id(&id)?;
        EpochMs::new(at_ms)?;
        checkpoint.validate()?;
        if !matches!(expected, JobState::Running | JobState::Validating) {
            return Err(ErrorCode::InvalidQuery.into());
        }
        self.write(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let old = load(&tx, &id)?;
            if old.job.state != expected {
                return Err(if old.job.state == JobState::Cancelling {
                    ErrorCode::JobCancelled
                } else {
                    ErrorCode::RevisionConflict
                }
                .into());
            }
            let previous: JobProgress = decoded(&tx.query_row(
                "SELECT progress_json FROM jobs WHERE job_id=?1",
                [&id],
                |r| r.get::<_, String>(0),
            )?)?;
            progress.validate_after(&previous)?;
            if checkpoint.batch_position.value() < old.checkpoint.batch_position.value() {
                return Err(ErrorCode::RevisionConflict.into());
            }
            tx.execute(
                "UPDATE jobs SET progress_json=?1,resume_json=?2,updated_at_ms=?3 WHERE job_id=?4",
                params![
                    serde_json::to_string(&progress)?,
                    serde_json::to_string(&checkpoint)?,
                    at_ms,
                    id
                ],
            )?;
            tx.commit()?;
            Ok(())
        })
    }
    /// Called once before any runtime worker starts. No input is assumed resumable without revalidation.
    pub fn interrupt_unfinished_jobs(&self, at_ms: i64) -> StoreResult<u64> {
        EpochMs::new(at_ms)?;
        self.write(move|conn| {let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let count=tx.execute("UPDATE jobs SET state='interrupted',error_code='JOB_INTERRUPTED',updated_at_ms=?1 WHERE state IN ('queued','running','validating','publishing','cancelling')",[at_ms])?;
            tx.execute("UPDATE ledger_generations SET state='failed' WHERE state='candidate'",[])?;
            tx.execute("UPDATE source_scan_runs SET state='interrupted',finished_at_ms=?1 WHERE state='running'",[at_ms])?;
            tx.commit()?;Ok(count as u64)
        })
    }
}
pub(crate) fn create_in_tx(
    tx: &rusqlite::Transaction<'_>,
    id: &str,
    request: &JobRequest,
    at_ms: i64,
) -> StoreResult<Job> {
    let existing: Option<String> = tx
        .query_row(
            "SELECT job_id FROM jobs WHERE request_key=?1",
            [&request.request_key],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(existing) = existing {
        let old = load(tx, &existing)?;
        if old.request != *request {
            return Err(ErrorCode::RequestKeyConflict.into());
        }
        return Ok(old.job);
    }
    let pending:i64=tx.query_row("SELECT COUNT(*) FROM jobs WHERE state IN ('queued','running','validating','publishing','cancelling')",[],|r|r.get(0))?;
    if pending >= 32 {
        return Err(ErrorCode::InvalidQuery.into());
    }
    tx.execute("INSERT INTO jobs(job_id,kind,state,request_key,scope_json,progress_json,resume_json,created_at_ms,updated_at_ms) VALUES(?1,?2,'queued',?3,?4,?5,?6,?7,?7)",params![id,text(&request.kind)?,request.request_key,serde_json::to_string(&request.scope)?,serde_json::to_string(&JobProgress::default())?,serde_json::to_string(&JobCheckpoint::default())?,at_ms])?;
    Ok(load(tx, id)?.job)
}
