//! Bounded model/project groups, priced inside the same SQLite snapshot.
use super::{MODEL_KEY, coverage, dashboard, fact_from, grouped, predicate, pricing, totals};
use crate::{Database, ErrorCode, Revision, StoreResult};
use rusqlite::{Transaction, params_from_iter};
use std::collections::BTreeMap;
use token_pulse_core::{
    numeric::{DecimalInt, EpochMs},
    pricing::PricingAccumulator,
    protocol::{
        DimensionSelection, PricingSummary, SnapshotMeta, TokenTotals, validate_request_id,
    },
    query::{GroupDimension, GroupedUsageBundle, GroupedUsageRequest, PricedUsageGroup, model_key},
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
    request.validate()?;
    validate_request_id(snapshot_id)?;
    let filter = &request.filter;
    let summary = totals(tx, filter)?;
    let coverage = coverage::coverage(tx, filter, &summary)?;
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
    let mut sums = groups
        .iter()
        .map(|group| {
            (
                group.key.clone(),
                PricingAccumulator::new(request.price_basis.clone()),
            )
        })
        .collect::<BTreeMap<_, _>>();
    if sums.len() != groups.len() {
        return Err(ErrorCode::DbCorrupt.into());
    }
    let mut overall = PricingAccumulator::new(request.price_basis.clone());
    let catalog = crate::pricing::catalog_at(tx, revision.price)?;
    pricing::visit(tx, filter, &request.price_basis, &catalog, |event| {
        let key = match request.dimension {
            GroupDimension::Models => model_key(event.provider.as_deref(), event.model.as_deref()),
            GroupDimension::Projects => event.project_id.clone(),
        };
        if let Some(sum) = sums.get_mut(&key) {
            sum.push(event.total_tokens, event.outcome.clone())?;
        }
        Ok(overall.push(event.total_tokens, event.outcome)?)
    })?;
    let pricing = overall.summary(false)?;
    check_pricing(&summary, &pricing)?;
    let mut output = Vec::with_capacity(groups.len());
    for group in groups.drain(..) {
        let pricing = sums
            .remove(&group.key)
            .ok_or(ErrorCode::DbCorrupt)?
            .summary(false)?;
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
    let (parser_versions, accounting_versions) = dashboard::versions(tx, filter, filter)?;
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
        self.snapshot(|tx, revision| bundle(tx, revision, request, at, snapshot_id))
    }
}

#[cfg(test)]
mod tests;
