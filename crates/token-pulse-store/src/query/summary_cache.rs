//! Shared range aggregates are bound to the full real SQLite version vector.
use super::{coverage, dashboard, pricing, totals};
use crate::{Database, ErrorCode, Revision, StoreResult};
use rusqlite::Transaction;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};
use token_pulse_core::{
    pricing::PricingAccumulator,
    protocol::{Coverage, PriceBasis, PricingSummary, TokenTotals, UsageFilter},
    query::model_key,
};
#[derive(serde::Serialize)]
pub(crate) struct ScopeSummary {
    pub totals: TokenTotals,
    pub coverage: Coverage,
    pub pricing: PricingSummary,
    pub sessions: BTreeMap<String, PricingSummary>,
    pub models: BTreeMap<String, PricingSummary>,
    pub projects: BTreeMap<String, PricingSummary>,
    pub parsers: Vec<String>,
    pub accounting: Vec<String>,
}
#[derive(Default)]
pub(crate) struct SummaryCache {
    values: Mutex<VecDeque<(String, Arc<ScopeSummary>, usize)>>,
    hits: AtomicU64,
    computations: AtomicU64,
}
impl SummaryCache {
    pub(crate) fn get(
        &self,
        tx: &Transaction<'_>,
        revision: Revision,
        filter: &UsageFilter,
        basis: &PriceBasis,
    ) -> StoreResult<Arc<ScopeSummary>> {
        let version = crate::database::read_usage_revision(tx)?;
        let mut normalized = filter.clone();
        for selection in [
            &mut normalized.sources,
            &mut normalized.models,
            &mut normalized.projects,
            &mut normalized.sessions,
        ] {
            if let token_pulse_core::protocol::DimensionSelection::Ids { ids, .. } = selection {
                ids.sort();
                ids.dedup();
            }
        }
        let key = serde_json::to_string(&(1, version, normalized, basis))?;
        {
            let mut values = self.values.lock().map_err(|_| ErrorCode::DbWriteFailed)?;
            if let Some(index) = values.iter().position(|entry| entry.0 == key) {
                let entry = values.remove(index).ok_or(ErrorCode::DbCorrupt)?;
                let value = entry.1.clone();
                values.push_back(entry);
                self.hits.fetch_add(1, Ordering::Relaxed);
                return Ok(value);
            }
        }
        self.computations.fetch_add(1, Ordering::Relaxed);
        let summary = Arc::new(compute(tx, revision, filter, basis)?);
        let bytes = serde_json::to_vec(summary.as_ref())?.len();
        if bytes <= 16 * 1024 * 1024 {
            let mut values = self.values.lock().map_err(|_| ErrorCode::DbWriteFailed)?;
            values.retain(|entry| entry.0 != key);
            while values.len() >= 20
                || values.iter().map(|entry| entry.2).sum::<usize>() + bytes > 16 * 1024 * 1024
            {
                values.pop_front();
            }
            values.push_back((key, summary.clone(), bytes));
        }
        Ok(summary)
    }
    pub(crate) fn stats(&self) -> (u64, u64) {
        (
            self.hits.load(Ordering::Relaxed),
            self.computations.load(Ordering::Relaxed),
        )
    }
}
pub(crate) fn compute(
    tx: &Transaction<'_>,
    revision: Revision,
    filter: &UsageFilter,
    basis: &PriceBasis,
) -> StoreResult<ScopeSummary> {
    let started = Instant::now();
    let totals = totals(tx, filter)?;
    let coverage = coverage::coverage(tx, filter, &totals)?;
    let catalog = crate::pricing::catalog_at(tx, revision.price)?;
    let mut overall = PricingAccumulator::new(basis.clone());
    let mut sessions: BTreeMap<String, PricingAccumulator> = BTreeMap::new();
    let mut models: BTreeMap<String, PricingAccumulator> = BTreeMap::new();
    let mut projects: BTreeMap<String, PricingAccumulator> = BTreeMap::new();
    pricing::visit(tx, filter, basis, &catalog, |event| {
        sessions
            .entry(event.session_key.clone())
            .or_insert_with(|| PricingAccumulator::new(basis.clone()))
            .push(event.total_tokens, event.outcome.clone())?;
        models
            .entry(model_key(event.provider.as_deref(), event.model.as_deref()).unwrap_or_default())
            .or_insert_with(|| PricingAccumulator::new(basis.clone()))
            .push(event.total_tokens, event.outcome.clone())?;
        projects
            .entry(event.project_id.clone().unwrap_or_default())
            .or_insert_with(|| PricingAccumulator::new(basis.clone()))
            .push(event.total_tokens, event.outcome.clone())?;
        Ok(overall.push(event.total_tokens, event.outcome)?)
    })?;
    let pricing = overall.summary(false)?;
    if pricing
        .priced_total_tokens
        .value()
        .checked_add(pricing.unpriced_total_tokens.value())
        .ok_or(ErrorCode::NumericOverflow)?
        != totals.total_tokens.value()
    {
        return Err(ErrorCode::DbCorrupt.into());
    }
    let finish = |items: BTreeMap<String, PricingAccumulator>| -> StoreResult<BTreeMap<String, PricingSummary>> { items.into_iter().map(|(key, accumulator)| Ok((key, accumulator.summary(false)?))).collect() };
    let (parsers, accounting) = dashboard::versions(tx, filter, filter)?;
    let result = ScopeSummary {
        totals,
        coverage,
        pricing,
        sessions: finish(sessions)?,
        models: finish(models)?,
        projects: finish(projects)?,
        parsers,
        accounting,
    };
    crate::query_timing::record("summary_compute", started);
    Ok(result)
}
impl Database {
    pub(crate) fn scope_summary(
        &self,
        tx: &Transaction<'_>,
        revision: Revision,
        filter: &UsageFilter,
        basis: &PriceBasis,
    ) -> StoreResult<Arc<ScopeSummary>> {
        self.summary_cache().get(tx, revision, filter, basis)
    }
    pub fn summary_cache_stats(&self) -> (u64, u64) {
        self.summary_cache().stats()
    }
}
#[cfg(test)]
mod tests;
