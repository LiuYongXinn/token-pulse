//! Persistent, disposable estimates. Publication never edits consumption facts or revisions.
use crate::{Database, ErrorCode, StoreResult};
use rusqlite::{OptionalExtension, Row, Statement, Transaction, TransactionBehavior, params};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use token_pulse_core::{
    numeric::{DecimalInt, DecimalMoney, EpochMs},
    pricing::{PriceOutcome, PricingEvent, UnpricedCode},
    protocol::PriceBasis,
};

/// Increment when the price algorithm or the fingerprint's interpretation changes.
pub const CACHE_VERSION: i64 = 7;
const BATCH_ROWS: usize = 500;

pub(crate) fn fingerprint(
    event: &PricingEvent<'_>,
    accounting: &str,
    request: Option<&crate::query::request_input::PricingRequestInput>,
) -> StoreResult<String> {
    let mut sources = event.source_ids.to_vec();
    sources.sort();
    sources.dedup();
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&(
            CACHE_VERSION,
            accounting,
            event.provider,
            event.model,
            sources,
            event.occurred_at_ms,
            event.usage,
            request,
        ))?)
    ))
}
fn basis_fields(basis: &PriceBasis) -> (&'static str, Option<i64>) {
    match basis {
        PriceBasis::EventTime {} => ("event_time", None),
        PriceBasis::SpecifiedTime { specified_at_ms } => {
            ("specified_time", Some(specified_at_ms.value()))
        }
    }
}
fn decode(row: &Row<'_>, offset: usize) -> rusqlite::Result<Option<PriceOutcome>> {
    let rule: Option<String> = row.get(offset)?;
    let currency: Option<String> = row.get(offset + 1)?;
    let atoms: Option<String> = row.get(offset + 2)?;
    let status: String = row.get(offset + 3)?;
    if status == "priced" {
        let (Some(rule_id), Some(currency), Some(atoms)) = (rule, currency, atoms) else {
            return Ok(None);
        };
        if rule_id.is_empty()
            || rule_id.len() > 256
            || rule_id.chars().any(char::is_control)
            || currency.len() != 3
            || !currency.bytes().all(|c| c.is_ascii_uppercase())
        {
            return Ok(None);
        }
        let Ok(cost_atoms) = DecimalInt::parse(&atoms) else {
            return Ok(None);
        };
        let Ok(estimated_cost) = DecimalMoney::from_atoms(cost_atoms.value()) else {
            return Ok(None);
        };
        Ok(Some(PriceOutcome::Priced {
            rule_id,
            currency,
            cost_atoms,
            estimated_cost,
        }))
    } else {
        if rule.is_some() || currency.is_some() || atoms.is_some() {
            return Ok(None);
        }
        let Ok(reason) = serde_json::from_value::<UnpricedCode>(status.into()) else {
            return Ok(None);
        };
        Ok(Some(PriceOutcome::Unpriced { reason }))
    }
}
/// Called only after the same snapshot's source vector has been validated.
/// Invalid derived values or contradictory duplicate estimates fall back to the live engine.
pub(crate) struct CacheReader<'a> {
    statement: Option<Statement<'a>>,
    candidates: std::collections::BTreeMap<String, Vec<String>>,
    prefetched: Option<std::collections::HashMap<String, Vec<CachedEstimate>>>,
}
struct CachedEstimate {
    set: String,
    input: String,
    outcome: Option<PriceOutcome>,
}
impl<'a> CacheReader<'a> {
    pub fn new(
        tx: &'a Transaction<'_>,
        revision: &DecimalInt,
        basis: &PriceBasis,
    ) -> StoreResult<Self> {
        let revision = i64::try_from(revision.value()).map_err(|_| ErrorCode::NumericOverflow)?;
        let (mode, specified) = basis_fields(basis);
        // Immutable header eligibility is shared by every event in this pinned transaction.
        // Keep only the two newest eligible sets per ledger. Missing/fingerprint-mismatched
        // estimates fall back to the exact engine; older caches are optional acceleration.
        let mut headers = tx.prepare("WITH eligible AS (SELECT cs.ledger_id,vs.valuation_set_id,ROW_NUMBER() OVER(PARTITION BY cs.ledger_id ORDER BY vs.created_at_ms DESC,vs.valuation_set_id) AS position FROM valuation_sets vs JOIN valuation_cache_sets cs USING(valuation_set_id) WHERE vs.price_revision=?1 AND vs.mode=?2 AND vs.specified_at_ms IS ?3 AND vs.state='ready' AND cs.cache_version=?4 AND cs.published_at_ms IS NOT NULL AND cs.content_sha256 IS NOT NULL) SELECT ledger_id,valuation_set_id FROM eligible WHERE position<=2 ORDER BY ledger_id,position LIMIT 8193")?;
        let mut rows = headers.query(params![revision, mode, specified, CACHE_VERSION])?;
        let mut candidates: std::collections::BTreeMap<String, Vec<String>> = Default::default();
        let mut count = 0;
        while let Some(row) = rows.next()? {
            count += 1;
            if count > 8192 {
                candidates.clear();
                break;
            }
            candidates.entry(row.get(0)?).or_default().push(row.get(1)?);
        }
        let statement = if candidates.is_empty() {
            None
        } else {
            Some(tx.prepare("SELECT ev.rule_id,ev.currency,ev.cost_atoms,ev.status FROM event_valuations ev JOIN valuation_cache_inputs ci ON ci.valuation_set_id=ev.valuation_set_id AND ci.event_id=ev.event_id WHERE ev.event_id=?1 AND ci.input_sha256=?2 AND ev.valuation_set_id IN (?3,?4) ORDER BY CASE WHEN ev.valuation_set_id=?3 THEN 0 ELSE 1 END LIMIT 2")?)
        };
        Ok(Self {
            statement,
            candidates,
            prefetched: None,
        })
    }
    /// Amortize point reads during a full aggregate, with a strict transient bound.
    /// Page/detail readers keep their point lookup; no cache spans a transaction.
    pub(crate) fn prefetch(
        &mut self,
        tx: &Transaction<'_>,
        scope: &crate::query::Predicate,
    ) -> StoreResult<()> {
        self.prefetch_bounded(tx, scope, 64 * 1024 * 1024)
    }
    fn prefetch_bounded(
        &mut self,
        tx: &Transaction<'_>,
        scope: &crate::query::Predicate,
        limit: usize,
    ) -> StoreResult<()> {
        self.prefetched = None;
        let sets: Vec<&str> = self
            .candidates
            .values()
            .flatten()
            .map(String::as_str)
            .collect();
        if sets.is_empty() {
            return Ok(());
        }
        let started = std::time::Instant::now();
        let placeholders = std::iter::repeat_n("?", sets.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT ev.event_id,ev.valuation_set_id,ci.input_sha256,ev.rule_id,ev.currency,ev.cost_atoms,ev.status FROM {} JOIN event_valuations ev ON ev.event_id=e.event_id JOIN valuation_cache_inputs ci ON ci.valuation_set_id=ev.valuation_set_id AND ci.event_id=ev.event_id WHERE ({}) AND ev.valuation_set_id IN ({placeholders})",
            crate::query::FROM,
            scope.sql
        );
        let mut statement = tx.prepare(&sql)?;
        let values = scope.values.iter().cloned().chain(
            sets.into_iter()
                .map(|s| rusqlite::types::Value::Text(s.into())),
        );
        let mut rows = statement.query(rusqlite::params_from_iter(values))?;
        let mut values: std::collections::HashMap<String, Vec<CachedEstimate>> = Default::default();
        let mut bytes = 0usize;
        while let Some(row) = rows.next()? {
            let event: String = row.get(0)?;
            let set: String = row.get(1)?;
            let input: String = row.get(2)?;
            let outcome = decode(row, 3)?;
            // Includes map/vector allocations and decoded strings with conservative slack.
            let cost_bytes = match &outcome {
                Some(PriceOutcome::Priced {
                    rule_id,
                    currency,
                    cost_atoms,
                    estimated_cost,
                }) => {
                    rule_id.len()
                        + currency.len()
                        + cost_atoms.as_str().len()
                        + estimated_cost.as_str().len()
                }
                _ => 0,
            };
            bytes = bytes.saturating_add(512 + event.len() + set.len() + input.len() + cost_bytes);
            if bytes > limit {
                crate::query_timing::record("pricing_prefetch_overflow", started);
                return Ok(()); // Discard the entire partial map; point queries remain exact.
            }
            values.entry(event).or_default().push(CachedEstimate {
                set,
                input,
                outcome,
            });
        }
        self.prefetched = Some(values);
        crate::query_timing::record("pricing_prefetch", started);
        Ok(())
    }
    pub fn lookup(
        &mut self,
        event: &str,
        input: &str,
        ledger: &str,
    ) -> StoreResult<Option<PriceOutcome>> {
        let Some(sets) = self.candidates.get(ledger) else {
            return Ok(None);
        };
        if let Some(prefetched) = &self.prefetched {
            let Some(records) = prefetched.get(event) else {
                return Ok(None);
            };
            let mut result: Option<PriceOutcome> = None;
            for set in sets {
                if let Some(record) = records.iter().find(|r| &r.set == set && r.input == input) {
                    let Some(outcome) = &record.outcome else {
                        return Ok(None);
                    };
                    if result.as_ref().is_some_and(|old| old != outcome) {
                        return Ok(None);
                    }
                    result = Some(outcome.clone());
                }
            }
            return Ok(result);
        }
        let Some(statement) = self.statement.as_mut() else {
            return Ok(None);
        };
        let mut rows = statement.query(params![event, input, &sets[0], sets.get(1)])?;
        let Some(first) = rows.next()? else {
            return Ok(None);
        };
        let Some(outcome) = decode(first, 0)? else {
            return Ok(None);
        };
        if let Some(second) = rows.next()? {
            let Some(other) = decode(second, 0)? else {
                return Ok(None);
            };
            if outcome != other {
                return Ok(None);
            }
        }
        Ok(Some(outcome))
    }
}

#[cfg(test)]
fn lookup(
    tx: &Transaction<'_>,
    event: &str,
    input: &str,
    revision: &DecimalInt,
    basis: &PriceBasis,
) -> StoreResult<Option<PriceOutcome>> {
    CacheReader::new(tx, revision, basis)?.lookup(
        event,
        input,
        &tx.query_row(
            "SELECT ledger_id FROM usage_events WHERE event_id=?1",
            [event],
            |r| r.get::<_, String>(0),
        )?,
    )
}

#[derive(Debug)]
pub struct ValuationResult {
    pub set_id: String,
    pub event_count: u64,
    pub price_revision: i64,
    pub already_ready: bool,
}
#[derive(Clone, Serialize)]
struct Input {
    ledger: String,
    evidence: i64,
    parser: String,
    accounting: String,
    count: i64,
    price: i64,
    basis: PriceBasis,
}
impl Input {
    fn id(&self) -> StoreResult<String> {
        Ok(format!(
            "valuation:{:x}",
            Sha256::digest(serde_json::to_vec(&(CACHE_VERSION, self))?)
        ))
    }
}
#[derive(Serialize)]
struct Estimate {
    event_id: String,
    fingerprint: String,
    outcome: PriceOutcome,
}
fn hash_item(hash: &mut Sha256, value: &impl Serialize) -> StoreResult<()> {
    let bytes = serde_json::to_vec(value)?;
    hash.update(
        u64::try_from(bytes.len())
            .map_err(|_| ErrorCode::NumericOverflow)?
            .to_le_bytes(),
    );
    hash.update(bytes);
    Ok(())
}
fn input(tx: &Transaction<'_>, ledger: &str, price: i64, basis: &PriceBasis) -> StoreResult<Input> {
    let (evidence,parser,accounting,count):(i64,String,String,i64)=tx.query_row("SELECT v.revision,l.parser_version,l.accounting_version,(SELECT COUNT(*) FROM active_usage_events e WHERE e.ledger_id=l.ledger_id) FROM ledger_usage_versions v JOIN ledger_generations l ON l.ledger_id=v.ledger_id JOIN sessions s ON s.active_ledger_id=l.ledger_id WHERE l.ledger_id=?1 AND l.state='active'",[ledger],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?.ok_or(ErrorCode::CandidateObsolete)?;
    Ok(Input {
        ledger: ledger.into(),
        evidence,
        parser,
        accounting,
        count,
        price,
        basis: basis.clone(),
    })
}
fn fresh(tx: &Transaction<'_>, expected: &Input, building: bool) -> StoreResult<()> {
    let id = expected.id()?;
    if input(tx, &expected.ledger, expected.price, &expected.basis)?.id()? != id {
        return Err(ErrorCode::CandidateObsolete.into());
    }
    if building {
        let state: Option<String> = tx
            .query_row(
                "SELECT state FROM valuation_sets WHERE valuation_set_id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()?;
        if state.as_deref() != Some("building") {
            return Err(ErrorCode::CandidateObsolete.into());
        }
    }
    Ok(())
}
fn stopped(stop: &AtomicBool) -> StoreResult<()> {
    if stop.load(Ordering::Acquire) {
        Err(ErrorCode::JobCancelled.into())
    } else {
        Ok(())
    }
}
fn insert(tx: &Transaction<'_>, id: &str, estimate: &Estimate) -> StoreResult<()> {
    let (rule, currency, atoms, status) = match &estimate.outcome {
        PriceOutcome::Priced {
            rule_id,
            currency,
            cost_atoms,
            ..
        } => (
            Some(rule_id.as_str()),
            Some(currency.as_str()),
            Some(cost_atoms.as_str()),
            "priced",
        ),
        PriceOutcome::Unpriced { reason } => (None, None, None, reason.as_str()),
        PriceOutcome::Redacted {} => return Err(ErrorCode::DbCorrupt.into()),
    };
    tx.execute("INSERT INTO event_valuations(valuation_set_id,event_id,rule_id,currency,cost_atoms,status) VALUES(?1,?2,?3,?4,?5,?6)",params![id,estimate.event_id,rule,currency,atoms,status])?;
    tx.execute("INSERT INTO valuation_cache_inputs(valuation_set_id,event_id,input_sha256) VALUES(?1,?2,?3)",params![id,estimate.event_id,estimate.fingerprint])?;
    Ok(())
}
impl Database {
    pub fn build_event_valuation(
        &self,
        ledger: &str,
        basis: &PriceBasis,
        at_ms: i64,
    ) -> StoreResult<ValuationResult> {
        self.build_event_valuation_interruptible(
            ledger,
            basis,
            at_ms,
            &Arc::new(AtomicBool::new(false)),
            |_, _| {},
        )
    }
    /// Stream a single fixed read transaction into bounded writer batches, then validate and publish.
    /// Progress callbacks run on the caller's thread and may persist job progress through the writer.
    pub fn build_event_valuation_interruptible(
        &self,
        ledger: &str,
        basis: &PriceBasis,
        at_ms: i64,
        stop: &Arc<AtomicBool>,
        progress: impl FnMut(u64, u64),
    ) -> StoreResult<ValuationResult> {
        self.build_event_valuation_pinned(ledger, basis, None, at_ms, stop, progress)
    }
    pub fn build_event_valuation_at_revision_interruptible(
        &self,
        ledger: &str,
        basis: &PriceBasis,
        price_revision: i64,
        at_ms: i64,
        stop: &Arc<AtomicBool>,
        progress: impl FnMut(u64, u64),
    ) -> StoreResult<ValuationResult> {
        self.build_event_valuation_pinned(
            ledger,
            basis,
            Some(price_revision),
            at_ms,
            stop,
            progress,
        )
    }
    fn build_event_valuation_pinned(
        &self,
        ledger: &str,
        basis: &PriceBasis,
        requested: Option<i64>,
        at_ms: i64,
        stop: &Arc<AtomicBool>,
        mut progress: impl FnMut(u64, u64),
    ) -> StoreResult<ValuationResult> {
        EpochMs::new(at_ms)?;
        stopped(stop)?;
        if ledger.is_empty() || ledger.len() > 256 || ledger.chars().any(char::is_control) {
            return Err(ErrorCode::InvalidQuery.into());
        }
        self.snapshot(|tx,revision| {
            let price=requested.unwrap_or(revision.price);
            if price<0 || price>revision.price {return Err(ErrorCode::InvalidQuery.into())}
            let input=input(tx,ledger,price,basis)?;
            let id=input.id()?;
            let begin=input.clone(); let begin_id=id.clone();
            let already_ready=self.write(move |conn| {
                let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
                fresh(&tx,&begin,false)?;
                let state:Option<String>=tx.query_row("SELECT state FROM valuation_sets WHERE valuation_set_id=?1",[&begin_id],|r|r.get(0)).optional()?;
                if state.as_deref()==Some("ready") {tx.commit()?; return Ok(true)}
                if state.as_deref()==Some("building") {return Err(ErrorCode::RevisionConflict.into())}
                let (mode,specified)=basis_fields(&begin.basis);
                tx.execute("INSERT INTO valuation_sets(valuation_set_id,price_revision,mode,specified_at_ms,state,created_at_ms) VALUES(?1,?2,?3,?4,'building',?5) ON CONFLICT(valuation_set_id) DO UPDATE SET state='building',created_at_ms=excluded.created_at_ms",params![begin_id,begin.price,mode,specified,at_ms])?;
                tx.execute("DELETE FROM event_valuations WHERE valuation_set_id=?1",[&begin_id])?;
                tx.execute("INSERT INTO valuation_cache_sets(valuation_set_id,ledger_id,evidence_revision,cache_version,parser_version,accounting_version,event_count) VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(valuation_set_id) DO UPDATE SET content_sha256=NULL,published_at_ms=NULL",params![begin_id,begin.ledger,begin.evidence,CACHE_VERSION,begin.parser,begin.accounting,begin.count])?;
                tx.commit()?;Ok(false)
            })?;
            let total=u64::try_from(input.count).map_err(|_|ErrorCode::DbCorrupt)?;
            if already_ready {return Ok(ValuationResult {set_id:id,event_count:total,price_revision:input.price,already_ready:true})}
            let result=(|| {
                let catalog=crate::pricing::catalog_at(tx,input.price)?;
                let mut hash=Sha256::new();hash_item(&mut hash,&(CACHE_VERSION,&input))?;
                let mut count=0_u64; let mut batch=Vec::with_capacity(BATCH_ROWS);
                progress(0,total);
                crate::query::pricing::visit_ledger_uncached(tx,ledger,basis,&catalog,|event| {
                    stopped(stop)?;
                    if event.ledger_id!=input.ledger {return Err(ErrorCode::DbCorrupt.into())}
                    let estimate=Estimate {event_id:event.event_id,fingerprint:event.cache_fingerprint,outcome:event.outcome};
                    hash_item(&mut hash,&estimate)?;batch.push(estimate);
                    count=count.checked_add(1).ok_or(ErrorCode::NumericOverflow)?;
                    if batch.len()==BATCH_ROWS {self.write_valuation_batch(&input,std::mem::take(&mut batch),stop)?;progress(count,total);}
                    Ok(())
                })?;
                if !batch.is_empty() {self.write_valuation_batch(&input,batch,stop)?;progress(count,total);}
                stopped(stop)?;
                if count!=total {return Err(ErrorCode::CandidateObsolete.into())}
                let digest=format!("{:x}",hash.finalize());
                let published=input.clone(); let published_id=id.clone();
                let publish_stop=stop.clone();
                self.write(move |conn| {
                    let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
                    stopped(&publish_stop)?;
                    fresh(&tx,&published,true)?;
                    let mut actual=Sha256::new();hash_item(&mut actual,&(CACHE_VERSION,&published))?;
                    let mut statement=tx.prepare("SELECT ev.event_id,ci.input_sha256,ev.rule_id,ev.currency,ev.cost_atoms,ev.status FROM event_valuations ev JOIN valuation_cache_inputs ci ON ci.valuation_set_id=ev.valuation_set_id AND ci.event_id=ev.event_id WHERE ev.valuation_set_id=?1 ORDER BY ev.event_id COLLATE BINARY")?;
                    let mut rows=statement.query([&published_id])?; let mut actual_count=0_u64;
                    while let Some(row)=rows.next()? {
                        stopped(&publish_stop)?;
                        let estimate=Estimate {event_id:row.get(0)?,fingerprint:row.get(1)?,outcome:decode(row,2)?.ok_or(ErrorCode::DbCorrupt)?};
                        hash_item(&mut actual,&estimate)?;actual_count=actual_count.checked_add(1).ok_or(ErrorCode::NumericOverflow)?;
                    }
                    if actual_count!=total || format!("{:x}",actual.finalize())!=digest {return Err(ErrorCode::DbCorrupt.into())}
                    drop(rows);drop(statement);
                    stopped(&publish_stop)?;
                    tx.execute("UPDATE valuation_cache_sets SET content_sha256=?1,published_at_ms=?2 WHERE valuation_set_id=?3",params![digest,at_ms,published_id])?;
                    tx.execute("UPDATE valuation_sets SET state='ready' WHERE valuation_set_id=?1 AND state='building'",[published_id])?;
                    tx.commit()?;Ok(())
                })?;
                Ok(ValuationResult {set_id:id.clone(),event_count:count,price_revision:input.price,already_ready:false})
            })();
            if result.is_err() {
                let failed_id=id.clone();
                let _=self.write(move |conn| {conn.execute("UPDATE valuation_sets SET state='failed' WHERE valuation_set_id=?1 AND state='building'",[failed_id])?;Ok(())});
            }
            result
        })
    }
    fn write_valuation_batch(
        &self,
        input: &Input,
        batch: Vec<Estimate>,
        stop: &Arc<AtomicBool>,
    ) -> StoreResult<()> {
        let input = input.clone();
        let stop = stop.clone();
        self.write(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            stopped(&stop)?;
            fresh(&tx, &input, true)?;
            let id = input.id()?;
            for estimate in batch {
                insert(&tx, &id, &estimate)?;
            }
            tx.commit()?;
            Ok(())
        })
    }
    pub fn interrupt_valuation_builds(&self) -> StoreResult<usize> {
        self.write(|conn| {
            Ok(conn.execute(
                "UPDATE valuation_sets SET state='failed' WHERE state='building'",
                [],
            )?)
        })
    }
}

#[cfg(test)]
mod tests;
