//! Stable pages group explicit turn identity, never derive it from chronology.
use super::*;
use crate::leases::LeaseHandle;
use token_pulse_core::query::{TurnRow, TurnsPage, TurnsQuery, TurnsRequest};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TurnPosition {
    turn_id: String,
    last_at_ms: EpochMs,
    generated_at_ms: EpochMs,
}
struct TurnPage {
    data: TurnsPage,
    more: bool,
}
fn page(
    tx: &Transaction<'_>,
    revision: Revision,
    handle: LeaseHandle,
    query: &TurnsQuery,
    last: Option<&TurnPosition>,
    at: EpochMs,
) -> StoreResult<TurnPage> {
    let (session_key, filter) =
        super::bundle::scoped_filter(tx, &query.session_key, &query.filter)?;
    let p = predicate(&filter)?;
    let mut values = p.values;
    let continuation = if let Some(last) = last {
        values.extend([
            Value::Integer(last.last_at_ms.value()),
            Value::Integer(last.last_at_ms.value()),
            Value::Text(last.turn_id.clone()),
        ]);
        "WHERE (last_at_ms<? OR (last_at_ms=? AND turn_id COLLATE BINARY>?))"
    } else {
        ""
    };
    values.push(Value::Integer(i64::from(query.page_size) + 1));
    let sql = format!(
        "WITH selected AS MATERIALIZED (SELECT e.* FROM {} WHERE {} AND e.turn_id IS NOT NULL AND e.turn_id<>''), grouped AS (SELECT e.turn_id,MIN(e.occurred_at_ms) AS first_at_ms,MAX(e.occurred_at_ms) AS last_at_ms,{} FROM selected e GROUP BY e.turn_id) SELECT * FROM grouped {continuation} ORDER BY last_at_ms DESC,turn_id COLLATE BINARY ASC LIMIT ?",
        fact_from(&filter, false),
        p.sql,
        aggregate_sql()
    );
    let mut statement = tx.prepare(&sql)?;
    let mut result = statement.query(params_from_iter(values))?;
    let mut turns = Vec::new();
    while let Some(row) = result.next()? {
        let turn_id: String = row.get(0)?;
        DimensionSelection::Ids {
            ids: vec![turn_id.clone()],
            include_unknown: false,
        }
        .validate()
        .map_err(|_| ErrorCode::DbCorrupt)?;
        turns.push(TurnRow {
            turn_id,
            first_at_ms: EpochMs::new(row.get(1)?)?,
            last_at_ms: EpochMs::new(row.get(2)?)?,
            summary: read_totals(row, 3)?,
            pricing: PricingAccumulator::new(query.price_basis.clone()).summary(false)?,
        });
    }
    let more = turns.len() > usize::from(query.page_size);
    turns.truncate(usize::from(query.page_size));
    let mut prices = turns
        .iter()
        .map(|turn| {
            (
                turn.turn_id.clone(),
                PricingAccumulator::new(query.price_basis.clone()),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let catalog = crate::pricing::catalog_at(tx, revision.price)?;
    let mut overall = PricingAccumulator::new(query.price_basis.clone());
    let mut unidentified = 0_i128;
    pricing::visit(tx, &filter, &query.price_basis, &catalog, |event| {
        if let Some(turn) = event.turn_id.as_ref().filter(|s| !s.is_empty()) {
            DimensionSelection::Ids {
                ids: vec![turn.clone()],
                include_unknown: false,
            }
            .validate()
            .map_err(|_| ErrorCode::DbCorrupt)?;
            if let Some(price) = prices.get_mut(turn) {
                price.push(event.total_tokens, event.outcome.clone())?;
            }
        } else {
            unidentified = unidentified
                .checked_add(1)
                .ok_or(ErrorCode::NumericOverflow)?;
        }
        Ok(overall.push(event.total_tokens, event.outcome)?)
    })?;
    for turn in &mut turns {
        turn.pricing = prices
            .remove(&turn.turn_id)
            .ok_or(ErrorCode::DbCorrupt)?
            .summary(false)?;
        check_price(&turn.summary, &turn.pricing)?;
    }
    let summary = totals(tx, &filter)?;
    let coverage = coverage::coverage(tx, &filter, &summary)?;
    let pricing = overall.summary(false)?;
    check_price(&summary, &pricing)?;
    let (parser_versions, accounting_versions) = dashboard::versions(tx, &filter, &filter)?;
    let id = handle
        .snapshot_id
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    Ok(TurnPage {
        more,
        data: TurnsPage {
            meta: SnapshotMeta {
                snapshot_id: format!("query-{id}"),
                data_revision: DecimalInt::from_nonnegative(revision.data.into())?,
                price_revision: DecimalInt::from_nonnegative(revision.price.into())?,
                generated_at_ms: last.map_or(at, |p| p.generated_at_ms),
                parser_versions,
                accounting_versions,
                display_timezone: filter.range.timezone,
            },
            session_key,
            summary,
            pricing,
            coverage,
            unidentified_usage_event_count: DecimalInt::from_nonnegative(unidentified)?,
            turns,
            next_cursor: None,
        },
    })
}
impl Database {
    pub fn query_turns(
        &self,
        owner: &str,
        request: &TurnsRequest,
        at: EpochMs,
    ) -> StoreResult<TurnsPage> {
        request.validate()?;
        let binding = QueryBinding::new(owner, &("turns", &request.query))?;
        let (handle, last) = match &request.cursor {
            Some(cursor) => {
                let (handle, last) = self
                    .leases()
                    .resolve_cursor::<TurnPosition>(cursor, &binding)?;
                (handle, Some(last))
            }
            None => (self.leases().open(&binding)?, None),
        };
        let query = request.query.clone();
        let result = self.leases().read(&handle, &binding, move |tx, revision| {
            page(tx, revision, handle, &query, last.as_ref(), at)
        });
        let mut page = match result {
            Ok(page) => page,
            Err(error) => {
                let _ = self.leases().release(&handle, &binding);
                return Err(error);
            }
        };
        if page.more {
            let last = page.data.turns.last().ok_or(ErrorCode::DbCorrupt)?;
            let position = TurnPosition {
                turn_id: last.turn_id.clone(),
                last_at_ms: last.last_at_ms,
                generated_at_ms: page.data.meta.generated_at_ms,
            };
            match self.leases().issue_cursor(&handle, &binding, &position) {
                Ok(cursor) => page.data.next_cursor = Some(cursor),
                Err(error) => {
                    let _ = self.leases().release(&handle, &binding);
                    return Err(error);
                }
            }
        } else {
            let _ = self.leases().release(&handle, &binding);
        }
        Ok(page.data)
    }
    pub fn close_turns(&self, owner: &str, request: &TurnsRequest) -> StoreResult<()> {
        request.validate()?;
        let cursor = request.cursor.as_ref().ok_or(ErrorCode::InvalidQuery)?;
        let binding = QueryBinding::new(owner, &("turns", &request.query))?;
        match self
            .leases()
            .resolve_cursor::<TurnPosition>(cursor, &binding)
        {
            Ok((handle, _)) => match self.leases().release(&handle, &binding) {
                Ok(()) => Ok(()),
                Err(e) if e.code == ErrorCode::SnapshotExpired => Ok(()),
                Err(e) => Err(e),
            },
            Err(e) if e.code == ErrorCode::SnapshotExpired => Ok(()),
            Err(e) => Err(e),
        }
    }
}
#[cfg(test)]
mod tests;
