//! Dashboard components are captured by one real SQLite read transaction.
use super::{BucketTotals, aggregate_sql, coverage, fact_from, predicate, read_totals, series};
use crate::{Database, ErrorCode, Revision, StoreResult};
use rusqlite::{Transaction, params_from_iter};
use std::collections::BTreeSet;
use token_pulse_core::{
    calendar::Grain,
    numeric::{DecimalInt, EpochMs},
    pricing::PricingAccumulator,
    protocol::{PriceBasis, SnapshotMeta, UsageFilter, validate_request_id},
    query::{DashboardBundle, DashboardRequest, RecentSession, UsageSeriesBucket},
};

fn recent_sessions(
    tx: &Transaction<'_>,
    filter: &UsageFilter,
    basis: &PriceBasis,
) -> StoreResult<Vec<RecentSession>> {
    let p = predicate(filter)?;
    let sql = format!(
        "WITH selected AS MATERIALIZED (SELECT e.* FROM {} WHERE {}), grouped(session_key,latest_at_ms,vector_sums,events,sessions,turns,turn_events) AS (SELECT e.session_key,MAX(e.occurred_at_ms),{} FROM selected e GROUP BY e.session_key ORDER BY MAX(e.occurred_at_ms) DESC,e.session_key COLLATE BINARY ASC LIMIT 10), latest AS (SELECT e.session_key,e.model,e.project_id,ROW_NUMBER() OVER(PARTITION BY e.session_key ORDER BY e.occurred_at_ms DESC,e.event_id COLLATE BINARY ASC) AS rn FROM selected e JOIN grouped g ON g.session_key=e.session_key) SELECT g.session_key,s.display_name,g.latest_at_ms,l.model,l.project_id,COALESCE(project.user_alias,project.display_name),g.vector_sums,g.events,g.sessions,g.turns,g.turn_events FROM grouped g JOIN session_labels s ON s.session_key=g.session_key JOIN latest l ON l.session_key=g.session_key AND l.rn=1 LEFT JOIN projects project ON project.project_id=l.project_id ORDER BY g.latest_at_ms DESC,g.session_key COLLATE BINARY ASC",
        fact_from(filter, false),
        p.sql,
        aggregate_sql()
    );
    let mut statement = tx.prepare(&sql)?;
    let mut rows = statement.query(params_from_iter(p.values))?;
    let mut result = Vec::new();
    while let Some(row) = rows.next()? {
        result.push(RecentSession {
            session_key: row.get(0)?,
            display_name: row.get(1)?,
            latest_at_ms: EpochMs::new(row.get(2)?)?,
            latest_model: row.get(3)?,
            latest_project_id: row.get(4)?,
            latest_project_name: row.get(5)?,
            summary: read_totals(row, 6)?,
            pricing: PricingAccumulator::new(basis.clone()).summary(false)?,
        });
    }
    Ok(result)
}
fn series_with_coverage(
    tx: &Transaction<'_>,
    filter: &UsageFilter,
    grain: Grain,
    common: &token_pulse_core::protocol::Coverage,
    cache: Option<&super::series_cache::SeriesCache>,
) -> StoreResult<Vec<UsageSeriesBucket>> {
    let buckets = match cache {
        Some(cache) => cache.get(tx, filter, grain)?,
        None => std::sync::Arc::new(series(tx, filter, grain)?),
    };
    let gaps = coverage::series_coverage_from(tx, filter, &buckets, common)?;
    Ok(buckets
        .iter()
        .zip(gaps)
        .map(
            |(BucketTotals { bucket, totals }, coverage)| UsageSeriesBucket {
                start_ms: bucket.start_ms,
                end_ms: bucket.end_ms,
                display_label: bucket.display_label.clone(),
                utc_offset: bucket.utc_offset.clone(),
                totals: totals.clone(),
                coverage,
            },
        )
        .collect())
}
pub(crate) fn versions(
    tx: &Transaction<'_>,
    filter: &UsageFilter,
    heatmap: &UsageFilter,
) -> StoreResult<(Vec<String>, Vec<String>)> {
    let mut p = predicate(filter)?;
    let heat = predicate(heatmap)?;
    let sql = format!(
        "SELECT DISTINCT lg.parser_version,lg.accounting_version FROM {} JOIN ledger_generations lg ON lg.ledger_id=e.ledger_id WHERE ({}) OR ({}) ORDER BY lg.parser_version,lg.accounting_version LIMIT 129",
        fact_from(filter, false),
        p.sql,
        heat.sql
    );
    p.values.extend(heat.values);
    let mut statement = tx.prepare(&sql)?;
    let mut rows = statement.query(params_from_iter(p.values))?;
    let mut parsers = BTreeSet::new();
    let mut accounting = BTreeSet::new();
    let mut count = 0;
    while let Some(row) = rows.next()? {
        count += 1;
        if count > 128 {
            return Err(ErrorCode::InvalidQuery.into());
        }
        parsers.insert(row.get::<_, String>(0)?);
        accounting.insert(row.get::<_, String>(1)?);
    }
    Ok((
        parsers.into_iter().collect(),
        accounting.into_iter().collect(),
    ))
}
pub fn bundle(
    tx: &Transaction<'_>,
    revision: Revision,
    request: &DashboardRequest,
    generated_at_ms: EpochMs,
    snapshot_id: &str,
) -> StoreResult<DashboardBundle> {
    let common =
        super::summary_cache::compute(tx, revision, &request.filter, &request.price_basis)?;
    assemble(
        tx,
        revision,
        request,
        generated_at_ms,
        snapshot_id,
        &common,
        None,
    )
}
fn assemble(
    tx: &Transaction<'_>,
    revision: Revision,
    request: &DashboardRequest,
    generated_at_ms: EpochMs,
    snapshot_id: &str,
    common: &super::summary_cache::ScopeSummary,
    cache: Option<&super::series_cache::SeriesCache>,
) -> StoreResult<DashboardBundle> {
    request.validate()?;
    validate_request_id(snapshot_id)?;
    let summary = common.totals.clone();
    let coverage = common.coverage.clone();
    let series = series_with_coverage(tx, &request.filter, request.grain, &common.coverage, cache)?;
    let mut heatmap_filter = request.filter.clone();
    heatmap_filter.range = request.heatmap_range.clone();
    let heatmap_started = std::time::Instant::now();
    let heatmap = series_with_coverage(tx, &heatmap_filter, Grain::Day, &common.coverage, cache)?;
    crate::query_timing::record("heatmap_compute", heatmap_started);
    let mut recent_sessions = recent_sessions(tx, &request.filter, &request.price_basis)?;
    let pricing = common.pricing.clone();
    let check_pricing = |summary: &token_pulse_core::protocol::TokenTotals,
                         pricing: &token_pulse_core::protocol::PricingSummary|
     -> StoreResult<()> {
        let total = pricing
            .priced_total_tokens
            .value()
            .checked_add(pricing.unpriced_total_tokens.value())
            .ok_or(ErrorCode::NumericOverflow)?;
        if total != summary.total_tokens.value() {
            return Err(ErrorCode::DbCorrupt.into());
        }
        Ok(())
    };
    check_pricing(&summary, &pricing)?;
    for session in &mut recent_sessions {
        session.pricing = common
            .sessions
            .get(&session.session_key)
            .ok_or(ErrorCode::DbCorrupt)?
            .clone();
        check_pricing(&session.summary, &session.pricing)?;
    }
    let (parser_versions, accounting_versions) = versions(tx, &request.filter, &heatmap_filter)?;
    Ok(DashboardBundle {
        meta: SnapshotMeta {
            snapshot_id: snapshot_id.into(),
            data_revision: DecimalInt::from_nonnegative(revision.data.into())?,
            price_revision: DecimalInt::from_nonnegative(revision.price.into())?,
            generated_at_ms,
            parser_versions,
            accounting_versions,
            display_timezone: request.filter.range.timezone.clone(),
        },
        summary,
        pricing,
        coverage,
        series,
        heatmap,
        recent_sessions,
    })
}
impl Database {
    pub fn dashboard_bundle(
        &self,
        request: &DashboardRequest,
        at: EpochMs,
        snapshot_id: &str,
    ) -> StoreResult<DashboardBundle> {
        self.usage_snapshot(|tx, revision| {
            let common = self.scope_summary(tx, revision, &request.filter, &request.price_basis)?;
            assemble(
                tx,
                revision,
                request,
                at,
                snapshot_id,
                &common,
                Some(self.series_cache()),
            )
        })
    }
}
#[cfg(test)]
mod tests;
