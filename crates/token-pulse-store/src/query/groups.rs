//! Paged model/project groups, priced inside the same SQLite snapshot.
use super::{coverage, fact_from, grouped, predicate};
use crate::leases::cursor::QueryBinding;
use crate::{Database, ErrorCode, Revision, StoreResult};
use rusqlite::{Transaction, params_from_iter};
use serde::{Deserialize, Serialize};
#[cfg(test)]
use token_pulse_core::query::model_key;
use token_pulse_core::{
    numeric::{DecimalInt, EpochMs},
    protocol::{PricingSummary, SnapshotMeta, TokenTotals, validate_request_id},
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
    assemble(tx, revision, request, at, snapshot_id, &common, 0)
}
#[allow(clippy::too_many_arguments)]
fn assemble(
    tx: &Transaction<'_>,
    revision: Revision,
    request: &GroupedUsageRequest,
    at: EpochMs,
    snapshot_id: &str,
    common: &super::summary_cache::ScopeSummary,
    offset: u64,
) -> StoreResult<GroupedUsageBundle> {
    request.validate()?;
    validate_request_id(snapshot_id)?;
    let filter = &request.filter;
    let summary = common.totals.clone();
    let coverage = common.coverage.clone();
    // The complete, same-snapshot pricing traversal already collected every model
    // identity, including unknown and unpriced/zero-token groups. Avoid decoding
    // every model identity again merely to count it. Project keys keep their SQL count.
    let total_group_count: i64 = if matches!(request.dimension, GroupDimension::Models) {
        common
            .models
            .len()
            .try_into()
            .map_err(|_| ErrorCode::NumericOverflow)?
    } else {
        let p = predicate(filter)?;
        // A grouped subquery counts NULL too; COUNT(DISTINCT key) would drop it.
        tx.query_row(
            &format!(
                "SELECT COUNT(*) FROM (SELECT e.project_id FROM {} WHERE {} GROUP BY e.project_id)",
                fact_from(filter, false),
                p.sql
            ),
            params_from_iter(p.values),
            |row| row.get(0),
        )?
    };
    let mut groups = if offset == 0 {
        grouped(
            tx,
            filter,
            request.dimension,
            request.sort,
            usize::from(request.limit),
        )?
    } else {
        super::raw_grouped_page(
            tx,
            filter,
            request.dimension,
            request.sort,
            usize::from(request.limit),
            offset,
        )?
    };
    let pricing = common.pricing.clone();
    check_pricing(&summary, &pricing)?;
    let grouped_coverage =
        coverage::grouped_coverage(tx, filter, request.dimension, &groups, &common.coverage)?;
    let mut output = Vec::with_capacity(groups.len());
    for (group, coverage) in groups.drain(..).zip(grouped_coverage) {
        let prices = match request.dimension {
            GroupDimension::Models => &common.models,
            GroupDimension::Projects => &common.projects,
        };
        let pricing = prices
            .get(group.key.as_deref().unwrap_or(""))
            .ok_or(ErrorCode::DbCorrupt)?
            .clone();
        check_pricing(&group.totals, &pricing)?;
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
        truncated: i128::from(total_group_count) > i128::from(offset) + output.len() as i128,
        groups: output,
        next_cursor: None,
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
            assemble(tx, revision, request, at, snapshot_id, &common, 0)
        })
    }
}

#[derive(Serialize, Deserialize)]
struct Position {
    offset: u64,
    generated_at_ms: EpochMs,
}
impl Database {
    pub fn grouped_usage_page(
        &self,
        owner: &str,
        request: &GroupedUsageRequest,
        at: EpochMs,
    ) -> StoreResult<GroupedUsageBundle> {
        request.validate()?;
        let mut query = request.clone();
        query.cursor = None;
        let binding = QueryBinding::new(owner, &("groups", &query))?;
        let (handle, position) = match &request.cursor {
            Some(cursor) => self.leases().resolve_cursor::<Position>(cursor, &binding)?,
            None => (
                self.leases().open(&binding)?,
                Position {
                    offset: 0,
                    generated_at_ms: at,
                },
            ),
        };
        let offset = position.offset;
        let captured_at = position.generated_at_ms;
        let id = format!(
            "query-{}",
            handle
                .snapshot_id
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        let result = self.leases().read_summary(
            &handle,
            &binding,
            self.clone(),
            query.filter.clone(),
            query.price_basis.clone(),
            move |tx, revision, common| {
                assemble(tx, revision, &query, captured_at, &id, common, offset)
            },
        );
        let mut page = match result {
            Ok(page) => page,
            Err(error) => {
                let _ = self.leases().release(&handle, &binding);
                return Err(error);
            }
        };
        if page.truncated {
            let next = Position {
                offset: offset
                    .checked_add(page.groups.len() as u64)
                    .ok_or(ErrorCode::NumericOverflow)?,
                generated_at_ms: captured_at,
            };
            match self.leases().issue_cursor(&handle, &binding, &next) {
                Ok(cursor) => page.next_cursor = Some(cursor),
                Err(error) => {
                    let _ = self.leases().release(&handle, &binding);
                    return Err(error);
                }
            }
        } else {
            let _ = self.leases().release(&handle, &binding);
        }
        Ok(page)
    }
    pub fn close_grouped_usage(
        &self,
        owner: &str,
        request: &GroupedUsageRequest,
    ) -> StoreResult<()> {
        request.validate()?;
        let cursor = request.cursor.as_ref().ok_or(ErrorCode::InvalidQuery)?;
        let mut query = request.clone();
        query.cursor = None;
        let binding = QueryBinding::new(owner, &("groups", &query))?;
        match self
            .leases()
            .resolve_cursor::<Position>(cursor, &binding)
            .and_then(|(handle, _)| self.leases().release(&handle, &binding))
        {
            Ok(()) => Ok(()),
            Err(error) if error.code == ErrorCode::SnapshotExpired => Ok(()),
            Err(error) => Err(error),
        }
    }
}

#[cfg(test)]
mod tests;
