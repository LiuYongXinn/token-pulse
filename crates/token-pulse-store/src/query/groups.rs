//! Bounded model/project groups, priced inside the same SQLite snapshot.
use super::{MODEL_KEY, coverage, fact_from, grouped, predicate};
use crate::{Database, ErrorCode, Revision, StoreResult};
use rusqlite::{Transaction, params_from_iter};
#[cfg(test)]
use token_pulse_core::query::model_key;
use token_pulse_core::{
    numeric::{DecimalInt, EpochMs},
    protocol::{
        DimensionSelection, PricingSummary, SnapshotMeta, TokenTotals, validate_request_id,
    },
    query::{GroupDimension, GroupedUsageBundle, GroupedUsageRequest, PricedUsageGroup},
};

fn check_pricing(totals: &TokenTotals, pricing: &PricingSummary) -> StoreResult<()> {
    let sum = pricing
        .priced_total_tokens
        .value()
        .checked_add(pricing.unpriced_total_tokens.value())
        .ok_or(ErrorCode::NumericOverflow)?;
    if sum != totals.total_tokens.value() {
        return Err(ErrorCode::DbCorrupt.into());
    }
    Ok(())
}
pub fn bundle(
    tx: &Transaction<'_>,
    revision: Revision,
    request: &GroupedUsageRequest,
    at: EpochMs,
    snapshot_id: &str,
) -> StoreResult<GroupedUsageBundle> {
    let common =
        super::summary_cache::compute(tx, revision, &request.filter, &request.price_basis)?;
    assemble(tx, revision, request, at, snapshot_id, &common)
}
fn assemble(
    tx: &Transaction<'_>,
    revision: Revision,
    request: &GroupedUsageRequest,
    at: EpochMs,
    snapshot_id: &str,
    common: &super::summary_cache::ScopeSummary,
) -> StoreResult<GroupedUsageBundle> {
    request.validate()?;
    validate_request_id(snapshot_id)?;
    let filter = &request.filter;
    let summary = common.totals.clone();
    let coverage = common.coverage.clone();
    let p = predicate(filter)?;
    let key = match request.dimension {
        GroupDimension::Models => MODEL_KEY,
        GroupDimension::Projects => "e.project_id",
    };
    // A grouped subquery counts the NULL category too. COUNT(DISTINCT key) would drop it.
    let total_group_count: i64 = tx.query_row(
        &format!(
            "SELECT COUNT(*) FROM (SELECT {key} FROM {} WHERE {} GROUP BY {key})",
            fact_from(filter, matches!(request.dimension, GroupDimension::Models)),
            p.sql
        ),
        params_from_iter(p.values),
        |row| row.get(0),
    )?;
    let mut groups = grouped(
        tx,
        filter,
        request.dimension,
        request.sort,
        usize::from(request.limit),
    )?;
    let pricing = common.pricing.clone();
    check_pricing(&summary, &pricing)?;
    let mut output = Vec::with_capacity(groups.len());
    for group in groups.drain(..) {
        let prices = match request.dimension {
            GroupDimension::Models => &common.models,
            GroupDimension::Projects => &common.projects,
        };
        let pricing = prices
            .get(group.key.as_deref().unwrap_or(""))
            .ok_or(ErrorCode::DbCorrupt)?
            .clone();
        check_pricing(&group.totals, &pricing)?;
        let selection = DimensionSelection::Ids {
            ids: group.key.iter().cloned().collect(),
            include_unknown: group.key.is_none(),
        };
        let mut scope = filter.clone();
        match request.dimension {
            GroupDimension::Models => scope.models = selection,
            GroupDimension::Projects => scope.projects = selection,
        }
        let coverage = coverage::coverage(tx, &scope, &group.totals)?;
        output.push(PricedUsageGroup {
            key: group.key,
            display_name: group.display_name,
            totals: group.totals,
            pricing,
            coverage,
        });
    }
    let (parser_versions, accounting_versions) =
        (common.parsers.clone(), common.accounting.clone());
    Ok(GroupedUsageBundle {
        meta: SnapshotMeta {
            snapshot_id: snapshot_id.into(),
            data_revision: DecimalInt::from_nonnegative(revision.data.into())?,
            price_revision: DecimalInt::from_nonnegative(revision.price.into())?,
            generated_at_ms: at,
            parser_versions,
            accounting_versions,
            display_timezone: filter.range.timezone.clone(),
        },
        summary,
        pricing,
        coverage,
        total_group_count: DecimalInt::from_nonnegative(total_group_count.into())?,
        truncated: total_group_count > output.len() as i64,
        groups: output,
    })
}
impl Database {
    pub fn grouped_usage_bundle(
        &self,
        request: &GroupedUsageRequest,
        at: EpochMs,
        snapshot_id: &str,
    ) -> StoreResult<GroupedUsageBundle> {
        self.usage_snapshot(|tx, revision| {
            let common = self.scope_summary(tx, revision, &request.filter, &request.price_basis)?;
            assemble(tx, revision, request, at, snapshot_id, &common)
        })
    }
}

#[cfg(test)]
mod tests;
