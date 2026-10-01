//! Dashboard components are captured by one real SQLite read transaction.
use super::{
    BucketTotals, aggregate_sql, coverage, fact_from, predicate, pricing, read_totals, series,
    totals,
};
use crate::{Database, ErrorCode, Revision, StoreResult};
use rusqlite::{Transaction, params_from_iter};
use std::collections::{BTreeMap, BTreeSet};
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
        "WITH selected AS MATERIALIZED (SELECT e.* FROM {} WHERE {}), grouped(session_key,latest_at_ms,vector_sums,events,sessions,turns,turn_events) AS (SELECT e.session_key,MAX(e.occurred_at_ms),{} FROM selected e GROUP BY e.session_key ORDER BY MAX(e.occurred_at_ms) DESC,e.session_key COLLATE BINARY ASC LIMIT 10), latest AS (SELECT e.session_key,e.model,e.project_id,ROW_NUMBER() OVER(PARTITION BY e.session_key ORDER BY e.occurred_at_ms DESC,e.event_id COLLATE BINARY ASC) AS rn FROM selected e JOIN grouped g ON g.session_key=e.session_key) SELECT g.session_key,COALESCE(s.provider_session_id,g.session_key),g.latest_at_ms,l.model,l.project_id,COALESCE(project.user_alias,project.display_name),g.vector_sums,g.events,g.sessions,g.turns,g.turn_events FROM grouped g JOIN sessions s ON s.session_key=g.session_key JOIN latest l ON l.session_key=g.session_key AND l.rn=1 LEFT JOIN projects project ON project.project_id=l.project_id ORDER BY g.latest_at_ms DESC,g.session_key COLLATE BINARY ASC",
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
) -> StoreResult<Vec<UsageSeriesBucket>> {
    let buckets = series(tx, filter, grain)?;
    let gaps = coverage::series_coverage(tx, filter, &buckets)?;
    Ok(buckets
        .into_iter()
        .zip(gaps)
        .map(
            |(BucketTotals { bucket, totals }, coverage)| UsageSeriesBucket {
                start_ms: bucket.start_ms,
                end_ms: bucket.end_ms,
                display_label: bucket.display_label,
                utc_offset: bucket.utc_offset,
                totals,
                coverage,
            },
        )
        .collect())
}
pub(super) fn versions(
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
    request.validate()?;
    validate_request_id(snapshot_id)?;
    let summary = totals(tx, &request.filter)?;
    let coverage = coverage::coverage(tx, &request.filter, &summary)?;
    let series = series_with_coverage(tx, &request.filter, request.grain)?;
    let mut heatmap_filter = request.filter.clone();
    heatmap_filter.range = request.heatmap_range.clone();
    let heatmap = series_with_coverage(tx, &heatmap_filter, Grain::Day)?;
    let mut recent_sessions = recent_sessions(tx, &request.filter, &request.price_basis)?;
    let mut per_session = recent_sessions
        .iter()
        .map(|s| {
            (
                s.session_key.clone(),
                PricingAccumulator::new(request.price_basis.clone()),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let catalog = crate::pricing::catalog_at(tx, revision.price)?;
    let mut prices = PricingAccumulator::new(request.price_basis.clone());
    pricing::visit(
        tx,
        &request.filter,
        &request.price_basis,
        &catalog,
        |event| {
            if let Some(session) = per_session.get_mut(&event.session_key) {
                session.push(event.total_tokens, event.outcome.clone())?;
            }
            Ok(prices.push(event.total_tokens, event.outcome)?)
        },
    )?;
    let pricing = prices.summary(false)?;
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
        session.pricing = per_session
            .remove(&session.session_key)
            .ok_or(ErrorCode::DbCorrupt)?
            .summary(false)?;
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
        self.snapshot(|tx, revision| bundle(tx, revision, request, at, snapshot_id))
    }
}
#[cfg(test)]
mod tests;
