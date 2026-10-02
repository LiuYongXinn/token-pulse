//! Same-snapshot event pricing. Source filters select facts, never price identity.
use super::{FROM, Predicate, predicate};
use crate::{Database, ErrorCode, StoreResult};
use rusqlite::{Transaction, params_from_iter};
use token_pulse_core::{
    domain::UsageVector,
    numeric::EpochMs,
    pricing::{PriceCatalog, PriceOutcome, PricingAccumulator, PricingEvent},
    protocol::{PriceBasis, PricingSummary, UsageFilter},
};

/// Read and priced one at a time; no raw-event vector sent to the renderer.
pub struct PricedEvent {
    pub event_id: String,
    pub ledger_id: String,
    pub session_key: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub project_id: Option<String>,
    pub occurred_at_ms: EpochMs,
    pub turn_id: Option<String>,
    pub total_tokens: i64,
    pub outcome: PriceOutcome,
    pub(crate) cache_fingerprint: String,
}
pub fn visit(
    tx: &Transaction<'_>,
    filter: &UsageFilter,
    basis: &PriceBasis,
    catalog: &PriceCatalog,
    consume: impl FnMut(PricedEvent) -> StoreResult<()>,
) -> StoreResult<()> {
    let p = predicate(filter)?;
    visit_where(tx, p, basis, catalog, true, consume)
}
pub(crate) fn visit_ledger_uncached(
    tx: &Transaction<'_>,
    ledger: &str,
    basis: &PriceBasis,
    catalog: &PriceCatalog,
    consume: impl FnMut(PricedEvent) -> StoreResult<()>,
) -> StoreResult<()> {
    visit_where(
        tx,
        Predicate {
            sql: "e.ledger_id=?".into(),
            values: vec![rusqlite::types::Value::Text(ledger.into())],
        },
        basis,
        catalog,
        false,
        consume,
    )
}
fn visit_where(
    tx: &Transaction<'_>,
    p: Predicate,
    basis: &PriceBasis,
    catalog: &PriceCatalog,
    use_cache: bool,
    mut consume: impl FnMut(PricedEvent) -> StoreResult<()>,
) -> StoreResult<()> {
    let mut cache = if use_cache {
        Some(crate::valuation::CacheReader::new(
            tx,
            &catalog.revision,
            basis,
        )?)
    } else {
        None
    };
    // The global source registry is bounded at 32. LIMIT 33 detects violation;
    // it never silently drops a source from rule matching. Mirrors are DISTINCT.
    let sources = "(SELECT json_group_array(source_id) FROM (SELECT DISTINCT sf.source_id AS source_id FROM event_provenance ep JOIN observations po ON po.observation_id=ep.observation_id JOIN file_generations fg ON fg.file_generation_id=po.file_generation_id JOIN source_files sf ON sf.file_id=fg.file_id WHERE ep.event_id=e.event_id ORDER BY sf.source_id COLLATE BINARY LIMIT 33))";
    let order = if use_cache {
        ""
    } else {
        " ORDER BY e.event_id COLLATE BINARY"
    };
    let sql = format!(
        "SELECT e.event_id,e.ledger_id,e.session_key,json_extract(o.normalized_json,'$.effective_metadata.provider'),e.model,e.project_id,e.occurred_at_ms,e.input_tokens_total,e.cached_input_tokens,e.output_tokens_total,e.reasoning_output_tokens,e.total_tokens,{sources},(SELECT accounting_version FROM ledger_generations WHERE ledger_id=e.ledger_id),e.turn_id FROM {FROM} WHERE {}{order}",
        p.sql
    );
    let mut statement = tx.prepare(&sql)?;
    let mut rows = statement.query(params_from_iter(p.values))?;
    while let Some(row) = rows.next()? {
        let provider: Option<String> = row.get(3).map_err(|_| ErrorCode::DbCorrupt)?;
        let model: Option<String> = row.get(4).map_err(|_| ErrorCode::DbCorrupt)?;
        let occurred_at_ms = EpochMs::new(row.get(6)?).map_err(|_| ErrorCode::DbCorrupt)?;
        let total: i64 = row.get(11)?;
        let usage = UsageVector {
            input_total: row.get(7)?,
            cached_input: row.get(8)?,
            output_total: row.get(9)?,
            reasoning_output: row.get(10)?,
            reported_total: Some(total),
        };
        let version: String = row.get(13)?;
        if usage.published_total(&version).map_err(|error| {
            if error == ErrorCode::UnsupportedFormat {
                error
            } else {
                ErrorCode::DbCorrupt
            }
        })? != Some(total)
        {
            return Err(ErrorCode::DbCorrupt.into());
        }
        let sources: Vec<String> =
            serde_json::from_str(&row.get::<_, String>(12)?).map_err(|_| ErrorCode::DbCorrupt)?;
        if sources.len() > 32
            || sources
                .iter()
                .any(|s| s.is_empty() || s.len() > 256 || s.chars().any(char::is_control))
        {
            return Err(ErrorCode::DbCorrupt.into());
        }
        let event_id: String = row.get(0)?;
        let event = PricingEvent {
            provider: provider.as_deref(),
            model: model.as_deref(),
            source_ids: &sources,
            occurred_at_ms,
            usage,
        };
        let cache_fingerprint = crate::valuation::fingerprint(&event, &version)?;
        let cached = if let Some(cache) = &mut cache {
            cache.lookup(&event_id, &cache_fingerprint)?
        } else {
            None
        };
        let outcome = cached.unwrap_or_else(|| catalog.estimate(&event, basis));
        consume(PricedEvent {
            event_id,
            ledger_id: row.get(1)?,
            session_key: row.get(2)?,
            provider,
            model,
            project_id: row.get(5)?,
            occurred_at_ms,
            turn_id: row.get(14)?,
            total_tokens: total,
            outcome,
            cache_fingerprint,
        })?;
    }
    Ok(())
}
pub fn summary(
    tx: &Transaction<'_>,
    filter: &UsageFilter,
    basis: &PriceBasis,
    price_revision: i64,
) -> StoreResult<PricingSummary> {
    let catalog = crate::pricing::catalog_at(tx, price_revision)?;
    let mut sums = PricingAccumulator::new(basis.clone());
    visit(tx, filter, basis, &catalog, |event| {
        Ok(sums.push(event.total_tokens, event.outcome)?)
    })?;
    Ok(sums.summary(false)?)
}
impl Database {
    pub fn pricing_summary(
        &self,
        filter: &UsageFilter,
        basis: &PriceBasis,
    ) -> StoreResult<PricingSummary> {
        self.snapshot(|tx, revision| summary(tx, filter, basis, revision.price))
    }
}

#[cfg(test)]
mod tests;
