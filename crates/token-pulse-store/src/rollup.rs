//! Rebuildable UTC projections. Facts remain authoritative until a complete
//! candidate is validated and published against the same ledger evidence.
use crate::{Database, ErrorCode, StoreResult, aggregate::VectorAccumulator};
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::atomic::{AtomicBool, Ordering},
};
use token_pulse_core::{
    domain::UsageVector,
    numeric::{DecimalInt, EpochMs},
};

pub const CACHE_VERSION: i64 = 1;
const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_COHORTS: usize = 32768;
const MAX_BATCH_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug)]
pub struct RollupResult {
    pub set_id: String,
    pub evidence_revision: i64,
    pub cohort_count: usize,
    pub already_ready: bool,
}
#[derive(Clone, Serialize)]
struct Input {
    ledger_id: String,
    revision: i64,
    parser: String,
    accounting: String,
    set_id: String,
}
#[derive(Serialize)]
struct Cohort {
    provider: Option<String>,
    model: Option<String>,
    project: Option<String>,
    sources: Vec<String>,
}
struct Accumulator {
    cohort: Cohort,
    sum: VectorAccumulator,
    known_turn_events: i64,
    turns: BTreeSet<String>,
}
#[derive(Serialize)]
struct PreparedRow {
    hour: i64,
    cohort_key: String,
    provider: Option<String>,
    model: Option<String>,
    project: Option<String>,
    sources_json: String,
    sums_json: String,
    event_count: i64,
    known_turn_events: i64,
    turns: Vec<String>,
}
struct Candidate {
    input: Input,
    rows: Vec<PreparedRow>,
    digest: String,
}
enum Prepared {
    Ready(RollupResult),
    Candidate(Candidate),
}
fn digest<T: Serialize>(value: &T) -> StoreResult<String> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(value)?)))
}
fn stopped(stop: &AtomicBool) -> StoreResult<()> {
    if stop.load(Ordering::Acquire) {
        Err(ErrorCode::JobInterrupted.into())
    } else {
        Ok(())
    }
}
fn load_input(tx: &Transaction<'_>, ledger: &str) -> StoreResult<Input> {
    let (revision,parser,accounting):(i64,String,String)=tx.query_row("SELECT v.revision,l.parser_version,l.accounting_version FROM ledger_usage_versions v JOIN ledger_generations l ON l.ledger_id=v.ledger_id JOIN sessions s ON s.active_ledger_id=l.ledger_id WHERE l.ledger_id=?1 AND l.state='active'",[ledger],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?.ok_or(ErrorCode::CandidateObsolete)?;
    let set_id = format!(
        "rollup:{}",
        digest(&(CACHE_VERSION, ledger, revision, &parser, &accounting))?
    );
    Ok(Input {
        ledger_id: ledger.into(),
        revision,
        parser,
        accounting,
        set_id,
    })
}
fn fresh(tx: &Transaction<'_>, input: &Input, building: bool) -> StoreResult<()> {
    let current = load_input(tx, &input.ledger_id)?;
    if current.set_id != input.set_id {
        return Err(ErrorCode::CandidateObsolete.into());
    }
    if building {
        let state: Option<String> = tx.query_row("SELECT state FROM usage_rollup_sets WHERE set_id=?1 AND ledger_id=?2 AND evidence_revision=?3 AND cache_version=?4 AND parser_version=?5 AND accounting_version=?6",params![input.set_id,input.ledger_id,input.revision,CACHE_VERSION,input.parser,input.accounting],|r|r.get(0)).optional()?;
        if state.as_deref() != Some("building") {
            return Err(ErrorCode::CandidateObsolete.into());
        }
    }
    Ok(())
}
fn content_digest<'a>(
    input: &Input,
    rows: impl Iterator<Item = &'a PreparedRow>,
) -> StoreResult<String> {
    let mut hash = Sha256::new();
    hash.update(serde_json::to_vec(&(CACHE_VERSION, input))?);
    let mut retained_bytes = 0usize;
    for row in rows {
        let bytes = serde_json::to_vec(row)?;
        retained_bytes = retained_bytes
            .checked_add(bytes.len())
            .ok_or(ErrorCode::NumericOverflow)?;
        if retained_bytes > MAX_BYTES {
            return Err(ErrorCode::InvalidQuery.into());
        }
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn prepare(tx: &Transaction<'_>, ledger: &str, stop: &AtomicBool) -> StoreResult<Prepared> {
    let input = load_input(tx, ledger)?;
    let ready: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM usage_rollup_sets WHERE set_id=?1 AND state='ready')",
        [&input.set_id],
        |r| r.get(0),
    )?;
    if ready {
        let count: i64 = tx.query_row(
            "SELECT COUNT(*) FROM utc_hour_usage_rollups WHERE set_id=?1",
            [&input.set_id],
            |r| r.get(0),
        )?;
        return Ok(Prepared::Ready(RollupResult {
            set_id: input.set_id,
            evidence_revision: input.revision,
            cohort_count: usize::try_from(count).map_err(|_| ErrorCode::NumericOverflow)?,
            already_ready: true,
        }));
    }
    let expected: (Option<String>, i64) = tx.query_row(
        "SELECT sum_token_decimal(total_tokens),COUNT(*) FROM usage_events WHERE ledger_id=?1",
        [ledger],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let mut groups = BTreeMap::<(i64, String), Accumulator>::new();
    let mut bytes = 0usize;
    let mut count = 0i64;
    let mut statement=tx.prepare("SELECT e.occurred_at_ms,e.model,json_extract(o.normalized_json,'$.effective_metadata.provider'),e.project_id,e.turn_id,e.input_tokens_total,e.cached_input_tokens,e.output_tokens_total,e.reasoning_output_tokens,e.total_tokens,(SELECT json_group_array(source_id) FROM (SELECT DISTINCT sf.source_id FROM event_provenance ep JOIN observations po ON po.observation_id=ep.observation_id JOIN file_generations fg ON fg.file_generation_id=po.file_generation_id JOIN source_files sf ON sf.file_id=fg.file_id WHERE ep.event_id=e.event_id ORDER BY sf.source_id COLLATE BINARY)) FROM usage_events e JOIN observations o ON o.observation_id=e.origin_observation_id WHERE e.ledger_id=?1 ORDER BY e.occurred_at_ms,e.event_id")?;
    let mut records = statement.query([ledger])?;
    while let Some(row) = records.next()? {
        if count % 256 == 0 {
            stopped(stop)?;
        }
        let time: i64 = row.get(0)?;
        let hour = time
            .div_euclid(3_600_000)
            .checked_mul(3_600_000)
            .ok_or(ErrorCode::NumericOverflow)?;
        let model: Option<String> = row.get(1)?;
        let provider: Option<String> = if model.is_some() { row.get(2)? } else { None };
        let project: Option<String> = row.get(3)?;
        let sources: Vec<String> = serde_json::from_str(&row.get::<_, String>(10)?)?;
        let cohort = Cohort {
            provider,
            model,
            project,
            sources,
        };
        let key = digest(&cohort)?;
        if !groups.contains_key(&(hour, key.clone())) {
            bytes = bytes
                .checked_add(serde_json::to_vec(&cohort)?.len() + 2048)
                .ok_or(ErrorCode::NumericOverflow)?;
            if bytes > MAX_BYTES || groups.len() >= MAX_COHORTS {
                return Err(ErrorCode::InvalidQuery.into());
            }
            groups.insert(
                (hour, key.clone()),
                Accumulator {
                    cohort,
                    sum: VectorAccumulator::default(),
                    known_turn_events: 0,
                    turns: BTreeSet::new(),
                },
            );
        }
        let group = groups.get_mut(&(hour, key)).ok_or(ErrorCode::DbCorrupt)?;
        let usage = UsageVector {
            input_total: row.get(5)?,
            cached_input: row.get(6)?,
            output_total: row.get(7)?,
            reasoning_output: row.get(8)?,
            reported_total: Some(row.get(9)?),
        };
        group.sum.add(usage)?;
        if let Some(turn) = row.get::<_, Option<String>>(4)?.filter(|s| !s.is_empty()) {
            group.known_turn_events = group
                .known_turn_events
                .checked_add(1)
                .ok_or(ErrorCode::NumericOverflow)?;
            if !group.turns.contains(&turn) {
                bytes = bytes
                    .checked_add(turn.len() + 128)
                    .ok_or(ErrorCode::NumericOverflow)?;
                if bytes > MAX_BYTES {
                    return Err(ErrorCode::InvalidQuery.into());
                }
                group.turns.insert(turn);
            }
        }
        count = count.checked_add(1).ok_or(ErrorCode::NumericOverflow)?;
    }
    let mut rows = Vec::with_capacity(groups.len());
    let mut total = 0i128;
    for ((hour, cohort_key), group) in groups {
        let sums = group.sum.finish()?;
        total = total
            .checked_add(sums.total.value())
            .ok_or(ErrorCode::NumericOverflow)?;
        rows.push(PreparedRow {
            hour,
            cohort_key,
            provider: group.cohort.provider,
            model: group.cohort.model,
            project: group.cohort.project,
            sources_json: serde_json::to_string(&group.cohort.sources)?,
            sums_json: serde_json::to_string(&sums)?,
            event_count: i64::try_from(group.sum.event_count())
                .map_err(|_| ErrorCode::NumericOverflow)?,
            known_turn_events: group.known_turn_events,
            turns: group.turns.into_iter().collect(),
        });
    }
    let expected_total = expected
        .0
        .map(|s| DecimalInt::parse(&s).map(|v| v.value()))
        .transpose()?
        .unwrap_or(0);
    if total != expected_total || count != expected.1 {
        return Err(ErrorCode::DbCorrupt.into());
    }
    let digest = content_digest(&input, rows.iter())?;
    Ok(Prepared::Candidate(Candidate {
        input,
        rows,
        digest,
    }))
}

fn begin(tx: &Transaction<'_>, input: &Input, at: i64) -> StoreResult<()> {
    fresh(tx, input, false)?;
    let state: Option<String> = tx
        .query_row(
            "SELECT state FROM usage_rollup_sets WHERE set_id=?1",
            [&input.set_id],
            |r| r.get(0),
        )
        .optional()?;
    match state.as_deref() {
        Some("building" | "ready") => return Err(ErrorCode::RevisionConflict.into()),
        Some(_) => {
            tx.execute(
                "DELETE FROM utc_hour_usage_rollups WHERE set_id=?1",
                [&input.set_id],
            )?;
            tx.execute("UPDATE usage_rollup_sets SET state='building',created_at_ms=?2,published_at_ms=NULL WHERE set_id=?1",params![input.set_id,at])?;
        }
        None => {
            tx.execute(
                "INSERT INTO usage_rollup_sets VALUES(?1,?2,?3,?4,?5,?6,'building',?7,NULL)",
                params![
                    input.set_id,
                    input.ledger_id,
                    input.revision,
                    CACHE_VERSION,
                    input.parser,
                    input.accounting,
                    at
                ],
            )?;
        }
    }
    Ok(())
}
fn stage_row(tx: &Transaction<'_>, input: &Input, row: &PreparedRow) -> StoreResult<()> {
    tx.execute(
        "INSERT INTO utc_hour_usage_rollups VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
        params![
            input.set_id,
            row.hour,
            row.cohort_key,
            row.provider,
            row.model,
            row.project,
            row.sources_json,
            row.sums_json,
            row.event_count,
            row.known_turn_events
        ],
    )?;
    Ok(())
}
fn persisted_digest(tx: &Transaction<'_>, input: &Input) -> StoreResult<String> {
    let mut hash = Sha256::new();
    hash.update(serde_json::to_vec(&(CACHE_VERSION, input))?);
    let mut statement=tx.prepare("SELECT hour_start_ms,cohort_key,model_provider,model,project_id,source_ids_json,token_sums_json,usage_event_count,known_turn_event_count FROM utc_hour_usage_rollups WHERE set_id=?1 ORDER BY hour_start_ms,cohort_key COLLATE BINARY")?;
    let mut turn_statement=tx.prepare("SELECT turn_id FROM utc_hour_rollup_turns WHERE set_id=?1 AND hour_start_ms=?2 AND cohort_key=?3 ORDER BY turn_id COLLATE BINARY")?;
    let mut rows = statement.query([&input.set_id])?;
    let mut bytes = 0usize;
    let mut count = 0usize;
    while let Some(row) = rows.next()? {
        let hour: i64 = row.get(0)?;
        let cohort_key: String = row.get(1)?;
        let turns = turn_statement
            .query_map(params![input.set_id, hour, cohort_key], |r| {
                r.get::<_, String>(0)
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let data = PreparedRow {
            hour,
            cohort_key,
            provider: row.get(2)?,
            model: row.get(3)?,
            project: row.get(4)?,
            sources_json: row.get(5)?,
            sums_json: row.get(6)?,
            event_count: row.get(7)?,
            known_turn_events: row.get(8)?,
            turns,
        };
        let encoded = serde_json::to_vec(&data)?;
        bytes = bytes
            .checked_add(encoded.len())
            .ok_or(ErrorCode::NumericOverflow)?;
        count += 1;
        if bytes > MAX_BYTES || count > MAX_COHORTS {
            return Err(ErrorCode::InvalidQuery.into());
        }
        hash.update((encoded.len() as u64).to_le_bytes());
        hash.update(encoded);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn publish(tx: &Transaction<'_>, input: &Input, expected: &str, at_ms: i64) -> StoreResult<()> {
    fresh(tx, input, true)?;
    if persisted_digest(tx, input)? != expected {
        return Err(ErrorCode::DbCorrupt.into());
    }
    tx.execute(
        "UPDATE usage_rollup_sets SET state='ready',published_at_ms=?2 WHERE set_id=?1",
        params![input.set_id, at_ms],
    )?;
    Ok(())
}

pub fn ready_set(tx: &Transaction<'_>, ledger: &str) -> StoreResult<Option<String>> {
    let input = load_input(tx, ledger)?;
    tx.query_row("SELECT set_id FROM usage_rollup_sets WHERE set_id=?1 AND ledger_id=?2 AND evidence_revision=?3 AND cache_version=?4 AND parser_version=?5 AND accounting_version=?6 AND state='ready'",params![input.set_id,input.ledger_id,input.revision,CACHE_VERSION,input.parser,input.accounting],|r|r.get(0)).optional().map_err(Into::into)
}

impl Database {
    pub fn build_hourly_rollup(&self, ledger: &str, at_ms: i64) -> StoreResult<RollupResult> {
        self.build_hourly_rollup_interruptible(ledger, at_ms, &AtomicBool::new(false))
    }
    pub fn build_hourly_rollup_interruptible(
        &self,
        ledger: &str,
        at_ms: i64,
        stop: &AtomicBool,
    ) -> StoreResult<RollupResult> {
        EpochMs::new(at_ms)?;
        stopped(stop)?;
        if ledger.is_empty() || ledger.len() > 256 {
            return Err(ErrorCode::InvalidQuery.into());
        }
        let candidate = match self.snapshot(|tx, _| prepare(tx, ledger, stop))? {
            Prepared::Ready(ready) => return Ok(ready),
            Prepared::Candidate(candidate) => candidate,
        };
        let input = candidate.input.clone();
        let created = input.clone();
        self.write(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            begin(&tx, &created, at_ms)?;
            tx.commit()?;
            Ok(())
        })?;
        let result = (|| -> StoreResult<RollupResult> {
            // Each writer task is at most 500 rows and 16 MiB. Turn membership
            // is staged separately so a single long turn cannot monopolize it.
            let count = candidate.rows.len();
            let mut batch = Vec::new();
            let mut bytes = 0usize;
            for row in candidate.rows {
                let size = serde_json::to_vec(&row)?.len();
                if size > MAX_BATCH_BYTES {
                    return Err(ErrorCode::InvalidQuery.into());
                }
                if !batch.is_empty() && (batch.len() == 500 || bytes + size > MAX_BATCH_BYTES) {
                    self.stage_rollup_rows(&input, std::mem::take(&mut batch), stop)?;
                    bytes = 0;
                }
                bytes += size;
                batch.push(row);
            }
            if !batch.is_empty() {
                self.stage_rollup_rows(&input, batch, stop)?;
            }
            stopped(stop)?;
            let published = input.clone();
            let expected = candidate.digest;
            self.write(move |conn| {
                let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
                publish(&tx, &published, &expected, at_ms)?;
                tx.commit()?;
                Ok(())
            })?;
            Ok(RollupResult {
                set_id: input.set_id.clone(),
                evidence_revision: input.revision,
                cohort_count: count,
                already_ready: false,
            })
        })();
        if result.is_err() {
            let failed = input.clone();
            let state = if matches!(
                result.as_ref().err().map(|e| e.code),
                Some(ErrorCode::CandidateObsolete | ErrorCode::JobInterrupted)
            ) {
                "obsolete"
            } else {
                "failed"
            };
            let _ = self.write(move |conn| {
                conn.execute(
                    "UPDATE usage_rollup_sets SET state=?2 WHERE set_id=?1 AND state='building'",
                    params![failed.set_id, state],
                )?;
                Ok(())
            });
        }
        result
    }
    fn stage_rollup_rows(
        &self,
        input: &Input,
        rows: Vec<PreparedRow>,
        stop: &AtomicBool,
    ) -> StoreResult<()> {
        stopped(stop)?;
        let stage = input.clone();
        // Move turn strings into their own bounded tasks after cohort rows.
        let mut turns = Vec::new();
        let mut turn_bytes = 0usize;
        let rows = self.write(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            fresh(&tx, &stage, true)?;
            for row in &rows {
                stage_row(&tx, &stage, row)?;
            }
            tx.commit()?;
            Ok(rows)
        })?;
        for row in rows {
            for turn in row.turns {
                let size = serde_json::to_vec(&(row.hour, &row.cohort_key, &turn))?.len();
                if size > MAX_BATCH_BYTES {
                    return Err(ErrorCode::InvalidQuery.into());
                }
                if !turns.is_empty() && (turns.len() == 500 || turn_bytes + size > MAX_BATCH_BYTES)
                {
                    self.stage_rollup_turns(input, std::mem::take(&mut turns), stop)?;
                    turn_bytes = 0;
                }
                turn_bytes += size;
                turns.push((row.hour, row.cohort_key.clone(), turn));
            }
        }
        if !turns.is_empty() {
            self.stage_rollup_turns(input, turns, stop)?;
        }
        Ok(())
    }
    fn stage_rollup_turns(
        &self,
        input: &Input,
        turns: Vec<(i64, String, String)>,
        stop: &AtomicBool,
    ) -> StoreResult<()> {
        stopped(stop)?;
        let input = input.clone();
        self.write(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            fresh(&tx, &input, true)?;
            for (hour, key, turn) in turns {
                tx.execute(
                    "INSERT INTO utc_hour_rollup_turns VALUES(?1,?2,?3,?4)",
                    params![input.set_id, hour, key, turn],
                )?;
            }
            tx.commit()?;
            Ok(())
        })
    }
    pub fn interrupt_rollup_builds(&self) -> StoreResult<usize> {
        self.write(|conn| {
            Ok(conn.execute(
                "UPDATE usage_rollup_sets SET state='obsolete' WHERE state='building'",
                [],
            )?)
        })
    }
}

#[cfg(test)]
mod tests;
