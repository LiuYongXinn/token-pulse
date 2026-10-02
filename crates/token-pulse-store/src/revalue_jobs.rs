//! Durable price work, independent of collection jobs and consumption checkpoints.
use crate::{Database, ErrorCode, StoreResult};
use rusqlite::{
    Connection, OptionalExtension, Transaction, TransactionBehavior, params, types::Value,
};
use sha2::{Digest, Sha256};
use token_pulse_core::{
    jobs::{CancelJobResult, JobScope},
    numeric::{DecimalInt, EpochMs},
    pricing::revalue::*,
    protocol::{PriceBasis, validate_request_id},
};

pub struct StoredRevalue {
    pub job: PriceRevalueJob,
    pub request: PriceRevalueRequest,
}
pub struct RevalueLedger {
    pub ledger_id: String,
    pub event_count: u64,
}
struct Planned {
    ledger: String,
    count: i64,
    evidence: i64,
    parser: String,
    accounting: String,
    cached: bool,
}
fn state_text(state: PriceRevalueState) -> StoreResult<String> {
    Ok(serde_json::to_value(state)?
        .as_str()
        .ok_or(ErrorCode::InvalidQuery)?
        .into())
}
fn integer(value: i64) -> StoreResult<DecimalInt> {
    Ok(DecimalInt::from_nonnegative(value.into())?)
}
fn load(conn: &Connection, id: &str) -> StoreResult<StoredRevalue> {
    let (request,status,state,automatic,price):(String,String,String,bool,i64)=conn.query_row("SELECT request_json,status_json,state,automatic,price_revision FROM price_revalue_jobs WHERE job_id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional()?.ok_or(ErrorCode::InvalidQuery)?;
    let mut request: PriceRevalueRequest =
        serde_json::from_str(&request).map_err(|_| ErrorCode::DbCorrupt)?;
    request.validate().map_err(|_| ErrorCode::DbCorrupt)?;
    let job: PriceRevalueJob = serde_json::from_str(&status).map_err(|_| ErrorCode::DbCorrupt)?;
    if job.job_id != id
        || state_text(job.state)? != state
        || job.automatic != automatic
        || job.price_revision.value() != i128::from(price)
        || job.price_revision != request.expected_price_revision
        || serde_json::to_value(&job.basis)? != serde_json::to_value(&request.basis)?
        || job.can_cancel != job.state.can_cancel()
        || job.completed_ledgers.value() > job.total_ledgers.value()
        || job.processed_events.value() > job.total_events.value()
    {
        return Err(ErrorCode::DbCorrupt.into());
    }
    Ok(StoredRevalue { job, request })
}
fn save(tx: &Transaction<'_>, job: &PriceRevalueJob) -> StoreResult<()> {
    tx.execute(
        "UPDATE price_revalue_jobs SET status_json=?1,state=?2,updated_at_ms=?3 WHERE job_id=?4",
        params![
            serde_json::to_string(job)?,
            state_text(job.state)?,
            job.updated_at_ms.value(),
            job.job_id
        ],
    )?;
    Ok(())
}
fn plan(
    tx: &Transaction<'_>,
    scope: &JobScope,
    price: i64,
    basis: &PriceBasis,
) -> StoreResult<Vec<Planned>> {
    let (mode, specified) = match basis {
        PriceBasis::EventTime {} => ("event_time", None),
        PriceBasis::SpecifiedTime { specified_at_ms } => {
            ("specified_time", Some(specified_at_ms.value()))
        }
    };
    let mut values = vec![
        Value::Integer(price),
        Value::Text(mode.into()),
        specified.map(Value::Integer).unwrap_or(Value::Null),
        Value::Integer(crate::valuation::CACHE_VERSION),
    ];
    let mut condition = String::new();
    let selected = match scope {
        JobScope::All {} => None,
        JobScope::Sources { source_ids } => Some(("sources", "source_id", source_ids)),
        JobScope::Sessions { session_keys } => Some(("sessions", "session_key", session_keys)),
    };
    if let Some((table, column, ids)) = selected {
        for id in ids {
            if !tx.query_row(
                &format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE {column}=?1)"),
                [id],
                |r| r.get::<_, bool>(0),
            )? {
                return Err(ErrorCode::InvalidQuery.into());
            }
        }
        let placeholders = ids
            .iter()
            .map(|id| {
                values.push(Value::Text(id.clone()));
                "?"
            })
            .collect::<Vec<_>>()
            .join(",");
        condition = match scope {
            JobScope::Sources { .. } => format!(
                " AND EXISTS(SELECT 1 FROM active_usage_events e JOIN event_provenance ep ON ep.event_id=e.event_id JOIN observations o ON o.observation_id=ep.observation_id JOIN file_generations fg ON fg.file_generation_id=o.file_generation_id JOIN source_files sf ON sf.file_id=fg.file_id WHERE e.ledger_id=l.ledger_id AND sf.source_id IN ({placeholders}))"
            ),
            _ => format!(" AND s.session_key IN ({placeholders})"),
        };
    }
    let sql = format!(
        "SELECT l.ledger_id,v.revision,l.parser_version,l.accounting_version,(SELECT COUNT(*) FROM active_usage_events e WHERE e.ledger_id=l.ledger_id),EXISTS(SELECT 1 FROM valuation_cache_sets cs JOIN valuation_sets vs USING(valuation_set_id) WHERE cs.ledger_id=l.ledger_id AND cs.evidence_revision=v.revision AND cs.parser_version=l.parser_version AND cs.accounting_version=l.accounting_version AND vs.price_revision=?1 AND vs.mode=?2 AND vs.specified_at_ms IS ?3 AND cs.cache_version=?4 AND vs.state='ready' AND cs.published_at_ms IS NOT NULL AND cs.content_sha256 IS NOT NULL) FROM ledger_generations l JOIN sessions s ON s.active_ledger_id=l.ledger_id JOIN ledger_usage_versions v ON v.ledger_id=l.ledger_id WHERE l.state='active'{condition} ORDER BY l.ledger_id COLLATE BINARY"
    );
    let mut statement = tx.prepare(&sql)?;
    Ok(statement
        .query_map(rusqlite::params_from_iter(values), |r| {
            Ok(Planned {
                ledger: r.get(0)?,
                evidence: r.get(1)?,
                parser: r.get(2)?,
                accounting: r.get(3)?,
                count: r.get(4)?,
                cached: r.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?)
}
fn totals(tx: &Transaction<'_>, job: &mut PriceRevalueJob) -> StoreResult<()> {
    let (ledgers,completed,events,processed):(i64,i64,Option<String>,Option<String>)=tx.query_row("SELECT COUNT(*),COALESCE(SUM(completed),0),sum_token_decimal(event_count),sum_token_decimal(processed_count) FROM price_revalue_plan WHERE job_id=?1",[&job.job_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?;
    job.total_ledgers = integer(ledgers)?;
    job.completed_ledgers = integer(completed)?;
    job.total_events = DecimalInt::parse(events.as_deref().unwrap_or("0"))?;
    job.processed_events = DecimalInt::parse(processed.as_deref().unwrap_or("0"))?;
    Ok(())
}
fn create(
    tx: &Transaction<'_>,
    id: &str,
    request: &PriceRevalueRequest,
    automatic: bool,
    planned: &[Planned],
    at: EpochMs,
) -> StoreResult<PriceRevalueJob> {
    if tx.query_row(
        "SELECT COUNT(*) FROM price_revalue_jobs WHERE state IN ('queued','running','cancelling')",
        [],
        |r| r.get::<_, i64>(0),
    )? >= 32
    {
        return Err(ErrorCode::InvalidQuery.into());
    }
    let mut job = PriceRevalueJob {
        job_id: id.into(),
        state: PriceRevalueState::Queued,
        automatic,
        price_revision: request.expected_price_revision.clone(),
        basis: request.basis.clone(),
        total_ledgers: integer(0)?,
        completed_ledgers: integer(0)?,
        total_events: integer(0)?,
        processed_events: integer(0)?,
        can_cancel: true,
        error: None,
        created_at_ms: at,
        updated_at_ms: at,
    };
    let price =
        i64::try_from(job.price_revision.value()).map_err(|_| ErrorCode::NumericOverflow)?;
    tx.execute("INSERT INTO price_revalue_jobs(job_id,request_key,request_json,status_json,state,automatic,price_revision,created_at_ms,updated_at_ms) VALUES(?1,?2,?3,?4,'queued',?5,?6,?7,?7)",params![id,request.request_key,serde_json::to_string(request)?,serde_json::to_string(&job)?,automatic,price,at.value()])?;
    for (position, p) in planned.iter().enumerate() {
        tx.execute("INSERT INTO price_revalue_plan(job_id,position,ledger_id,event_count) VALUES(?1,?2,?3,?4)",params![id,i64::try_from(position).map_err(|_|ErrorCode::NumericOverflow)?,p.ledger,p.count])?;
    }
    totals(tx, &mut job)?;
    save(tx, &job)?;
    Ok(job)
}
impl Database {
    pub fn create_price_revalue_job(
        &self,
        id: String,
        mut request: PriceRevalueRequest,
        at_ms: i64,
    ) -> StoreResult<PriceRevalueJob> {
        validate_request_id(&id)?;
        request.validate()?;
        let at = EpochMs::new(at_ms)?;
        self.write(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let old: Option<String> = tx
                .query_row(
                    "SELECT job_id FROM price_revalue_jobs WHERE request_key=?1",
                    [&request.request_key],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(old) = old {
                let old = load(&tx, &old)?;
                if serde_json::to_value(old.request)? != serde_json::to_value(&request)? {
                    return Err(ErrorCode::RequestKeyConflict.into());
                }
                tx.commit()?;
                return Ok(old.job);
            }
            let price: i64 = tx.query_row(
                "SELECT price_revision FROM app_state WHERE singleton=1",
                [],
                |r| r.get(0),
            )?;
            if i128::from(price) != request.expected_price_revision.value() {
                return Err(ErrorCode::RevisionConflict.into());
            }
            let planned = plan(&tx, &request.scope, price, &request.basis)?;
            let job = create(&tx, &id, &request, false, &planned, at)?;
            tx.commit()?;
            Ok(job)
        })
    }
    /// Automatic request identity excludes cache readiness, so cancellation cannot restart itself.
    pub fn enqueue_automatic_price_revalue(
        &self,
        at_ms: i64,
    ) -> StoreResult<Option<PriceRevalueJob>> {
        let at = EpochMs::new(at_ms)?;
        self.write(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let price: i64 = tx.query_row(
                "SELECT price_revision FROM app_state WHERE singleton=1",
                [],
                |r| r.get(0),
            )?;
            let basis = PriceBasis::EventTime {};
            let scope = JobScope::All {};
            let all = plan(&tx, &scope, price, &basis)?;
            let identity: Vec<_> = all
                .iter()
                .map(|p| (&p.ledger, p.evidence, &p.parser, &p.accounting, p.count))
                .collect();
            let key = format!(
                "auto-revalue:{:x}",
                Sha256::digest(serde_json::to_vec(&(
                    crate::valuation::CACHE_VERSION,
                    price,
                    identity
                ))?)
            );
            let pending: Vec<_> = all
                .into_iter()
                .filter(|p| p.count > 0 && !p.cached)
                .collect();
            if pending.is_empty() {
                tx.commit()?;
                return Ok(None);
            }
            let old: Option<String> = tx
                .query_row(
                    "SELECT job_id FROM price_revalue_jobs WHERE request_key=?1",
                    [&key],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(old) = old {
                let stored = load(&tx, &old)?;
                if stored.job.state != PriceRevalueState::Interrupted {
                    tx.commit()?;
                    return Ok(None);
                }
                tx.execute("DELETE FROM price_revalue_jobs WHERE job_id=?1", [&old])?;
                let job = create(&tx, &old, &stored.request, true, &pending, at)?;
                tx.commit()?;
                return Ok(Some(job));
            }
            let request = PriceRevalueRequest {
                scope,
                basis,
                expected_price_revision: integer(price)?,
                request_key: key.clone(),
            };
            let job = create(&tx, &key, &request, true, &pending, at)?;
            tx.commit()?;
            Ok(Some(job))
        })
    }
    pub fn get_price_revalue_job(&self, id: &str) -> StoreResult<StoredRevalue> {
        validate_request_id(id)?;
        self.snapshot(|tx, _| load(tx, id))
    }
    pub fn claim_price_revalue_job(&self, at_ms: i64) -> StoreResult<Option<StoredRevalue>> {
        let at = EpochMs::new(at_ms)?;
        self.write(move |conn| {
            let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            if tx.query_row("SELECT EXISTS(SELECT 1 FROM price_revalue_jobs WHERE state IN ('running','cancelling'))",[],|r|r.get::<_,bool>(0))? {tx.commit()?;return Ok(None)}
            let id:Option<String>=tx.query_row("SELECT job_id FROM price_revalue_jobs WHERE state='queued' ORDER BY automatic,created_at_ms,job_id LIMIT 1",[],|r|r.get(0)).optional()?;
            let Some(id)=id else {tx.commit()?;return Ok(None)};let mut stored=load(&tx,&id)?;
            stored.job.state=PriceRevalueState::Running;stored.job.updated_at_ms=at;save(&tx,&stored.job)?;tx.commit()?;Ok(Some(stored))
        })
    }
    pub fn price_revalue_plan(&self, id: &str) -> StoreResult<Vec<RevalueLedger>> {
        validate_request_id(id)?;
        self.snapshot(|tx,_| {load(tx,id)?;let mut stmt=tx.prepare("SELECT ledger_id,event_count FROM price_revalue_plan WHERE job_id=?1 ORDER BY position")?;
            let rows=stmt.query_map([id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,i64>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
            rows.into_iter().map(|(ledger_id,count)|Ok(RevalueLedger {ledger_id,event_count:u64::try_from(count).map_err(|_|ErrorCode::DbCorrupt)?})).collect()
        })
    }
    pub fn progress_price_revalue(
        &self,
        id: String,
        ledger: String,
        done: u64,
        total: u64,
        completed: bool,
        at_ms: i64,
    ) -> StoreResult<()> {
        validate_request_id(&id)?;
        if done > total || completed && done != total {
            return Err(ErrorCode::InvalidQuery.into());
        }
        let done = i64::try_from(done).map_err(|_| ErrorCode::NumericOverflow)?;
        let total = i64::try_from(total).map_err(|_| ErrorCode::NumericOverflow)?;
        let at = EpochMs::new(at_ms)?;
        self.write(move |conn| {let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;let mut stored=load(&tx,&id)?;
            if stored.job.state!=PriceRevalueState::Running {return Err(if stored.job.state==PriceRevalueState::Cancelling {ErrorCode::JobCancelled} else {ErrorCode::RevisionConflict}.into())}
            if tx.execute("UPDATE price_revalue_plan SET event_count=?1,processed_count=?2,completed=?3 WHERE job_id=?4 AND ledger_id=?5 AND completed=0 AND processed_count<=?2",params![total,done,completed,id,ledger])?!=1 {return Err(ErrorCode::RevisionConflict.into())}
            totals(&tx,&mut stored.job)?;stored.job.updated_at_ms=at;save(&tx,&stored.job)?;tx.commit()?;Ok(())
        })
    }
    pub fn cancel_price_revalue_job(&self, id: String, at_ms: i64) -> StoreResult<CancelJobResult> {
        validate_request_id(&id)?;
        let at = EpochMs::new(at_ms)?;
        self.write(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let mut stored = load(&tx, &id)?;
            if stored.job.state.finished() {
                tx.commit()?;
                return Ok(CancelJobResult::AlreadyFinished);
            }
            stored.job.state = if stored.job.state == PriceRevalueState::Queued {
                PriceRevalueState::Cancelled
            } else {
                PriceRevalueState::Cancelling
            };
            stored.job.can_cancel = stored.job.state.can_cancel();
            stored.job.error = if stored.job.state == PriceRevalueState::Cancelled {
                Some(ErrorCode::JobCancelled)
            } else {
                None
            };
            stored.job.updated_at_ms = at;
            save(&tx, &stored.job)?;
            tx.commit()?;
            Ok(CancelJobResult::Accepted)
        })
    }
    pub fn finish_price_revalue_job(
        &self,
        id: String,
        error: Option<ErrorCode>,
        at_ms: i64,
    ) -> StoreResult<PriceRevalueJob> {
        validate_request_id(&id)?;
        let at = EpochMs::new(at_ms)?;
        self.write(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let mut stored = load(&tx, &id)?;
            if !matches!(
                stored.job.state,
                PriceRevalueState::Running | PriceRevalueState::Cancelling
            ) {
                return Err(ErrorCode::RevisionConflict.into());
            }
            let error = if stored.job.state == PriceRevalueState::Cancelling {
                Some(ErrorCode::JobCancelled)
            } else {
                error
            };
            if error.is_none()
                && (stored.job.completed_ledgers != stored.job.total_ledgers
                    || stored.job.processed_events != stored.job.total_events)
            {
                return Err(ErrorCode::InvalidQuery.into());
            }
            stored.job.state = match error {
                None => PriceRevalueState::Succeeded,
                Some(ErrorCode::JobCancelled) => PriceRevalueState::Cancelled,
                Some(ErrorCode::JobInterrupted) => PriceRevalueState::Interrupted,
                Some(_) => PriceRevalueState::Failed,
            };
            stored.job.error = error;
            stored.job.can_cancel = false;
            stored.job.updated_at_ms = at;
            save(&tx, &stored.job)?;
            tx.commit()?;
            Ok(stored.job)
        })
    }
    pub fn interrupt_price_revalue_jobs(&self, at_ms: i64) -> StoreResult<usize> {
        let at = EpochMs::new(at_ms)?;
        self.write(move |conn| {let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let mut stmt=tx.prepare("SELECT job_id FROM price_revalue_jobs WHERE state IN ('queued','running','cancelling')")?;
            let ids=stmt.query_map([],|r|r.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;drop(stmt);
            for id in &ids {let mut stored=load(&tx,id)?;stored.job.state=if stored.job.state==PriceRevalueState::Cancelling {PriceRevalueState::Cancelled} else {PriceRevalueState::Interrupted};stored.job.error=Some(if stored.job.state==PriceRevalueState::Cancelled {ErrorCode::JobCancelled} else {ErrorCode::JobInterrupted});stored.job.can_cancel=false;stored.job.updated_at_ms=at;save(&tx,&stored.job)?;}
            tx.commit()?;Ok(ids.len())
        })
    }
    pub fn price_revalue_status(&self) -> StoreResult<PriceRevalueStatus> {
        self.snapshot(|tx,revision| {
            let active:Option<String>=tx.query_row("SELECT job_id FROM price_revalue_jobs WHERE state IN ('queued','running','cancelling') ORDER BY CASE state WHEN 'running' THEN 0 WHEN 'cancelling' THEN 1 ELSE 2 END,automatic,created_at_ms,job_id LIMIT 1",[],|r|r.get(0)).optional()?;
            let latest:Option<String>=tx.query_row("SELECT job_id FROM price_revalue_jobs ORDER BY updated_at_ms DESC,job_id DESC LIMIT 1",[],|r|r.get(0)).optional()?;
            let pending=plan(tx,&JobScope::All {},revision.price,&PriceBasis::EventTime {})?.into_iter().filter(|p|p.count>0&&!p.cached).count();
            Ok(PriceRevalueStatus {current_price_revision:integer(revision.price)?,active_job:active.map(|id|load(tx,&id).map(|s|s.job)).transpose()?,latest_job:latest.map(|id|load(tx,&id).map(|s|s.job)).transpose()?,uncached_ledgers:DecimalInt::from_nonnegative(i128::try_from(pending).map_err(|_|ErrorCode::NumericOverflow)?)?})
        })
    }
}

#[cfg(test)]
mod tests;
