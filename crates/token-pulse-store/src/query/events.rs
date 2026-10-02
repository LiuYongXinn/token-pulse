//! Published usage, whitelisted source vectors and price audit share a lease.
use super::{FROM, coverage, dashboard, predicate, pricing, totals};
use crate::{
    Database, ErrorCode, Revision, StoreResult,
    leases::{LeaseHandle, cursor::QueryBinding},
};
use rusqlite::{Transaction, params_from_iter, types::Value};
use serde::{Deserialize, Serialize};
use token_pulse_core::{
    domain::UsageVector,
    numeric::{DecimalInt, EpochMs},
    pricing::PricingEvent,
    protocol::{DimensionSelection, SnapshotMeta},
    query::{
        RawUsageVector, UsageEventRow, UsageEventSort, UsageEventsPage, UsageEventsQuery,
        UsageEventsRequest,
    },
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Position {
    event_id: String,
    occurred_at_ms: EpochMs,
    total_tokens: DecimalInt,
    generated_at_ms: EpochMs,
}
struct Page {
    data: UsageEventsPage,
    more: bool,
}
fn raw(value: Option<String>) -> StoreResult<Option<RawUsageVector>> {
    value
        .map(|json| {
            serde_json::from_str::<UsageVector>(&json)
                .map(RawUsageVector::from)
                .map_err(|_| ErrorCode::DbCorrupt.into())
        })
        .transpose()
}
fn key(value: &str) -> StoreResult<()> {
    DimensionSelection::Ids {
        ids: vec![value.into()],
        include_unknown: false,
    }
    .validate()
    .map_err(|_| ErrorCode::DbCorrupt.into())
}
fn rows(
    tx: &Transaction<'_>,
    revision: Revision,
    handle: LeaseHandle,
    query: &UsageEventsQuery,
    last: Option<&Position>,
    at: EpochMs,
) -> StoreResult<Page> {
    let p = predicate(&query.filter)?;
    let mut values = p.values;
    let order_key = match query.sort {
        UsageEventSort::TimeDesc => "e.occurred_at_ms",
        UsageEventSort::TotalDesc => "e.total_tokens",
    };
    let continuation = if let Some(last) = last {
        let value = match query.sort {
            UsageEventSort::TimeDesc => last.occurred_at_ms.value(),
            UsageEventSort::TotalDesc => {
                i64::try_from(last.total_tokens.value()).map_err(|_| ErrorCode::CursorInvalid)?
            }
        };
        values.extend([
            Value::Integer(value),
            Value::Integer(value),
            Value::Text(last.event_id.clone()),
        ]);
        format!(" AND ({order_key} < ? OR ({order_key} = ? AND e.event_id COLLATE BINARY > ?))")
    } else {
        String::new()
    };
    values.push(Value::Integer(i64::from(query.page_size) + 1));
    let sources = "(SELECT json_group_array(source_id) FROM (SELECT DISTINCT sf.source_id AS source_id FROM event_provenance ep JOIN observations po ON po.observation_id=ep.observation_id JOIN file_generations fg ON fg.file_generation_id=po.file_generation_id JOIN source_files sf ON sf.file_id=fg.file_id WHERE ep.event_id=e.event_id ORDER BY sf.source_id COLLATE BINARY LIMIT 33))";
    // Extract only normalized token vectors. Never return normalized JSON,
    // prompts, cwd, authentication or arbitrary fields to the renderer.
    let sql = format!(
        "SELECT e.event_id,e.session_key,COALESCE(session.provider_session_id,e.session_key),e.occurred_at_ms,e.model,json_extract(o.normalized_json,'$.effective_metadata.provider'),e.project_id,COALESCE(project.user_alias,project.display_name),e.turn_id,e.input_tokens_total,e.cached_input_tokens,e.output_tokens_total,e.reasoning_output_tokens,e.source_total_tokens,e.total_tokens,e.calculation_method,e.quality_json,lg.parser_version,lg.accounting_version,{sources},json_extract(o.normalized_json,'$.last'),json_extract(o.normalized_json,'$.cumulative') FROM {FROM} JOIN sessions session ON session.session_key=e.session_key JOIN ledger_generations lg ON lg.ledger_id=e.ledger_id LEFT JOIN projects project ON project.project_id=e.project_id WHERE {}{continuation} ORDER BY {order_key} DESC,e.event_id COLLATE BINARY ASC LIMIT ?",
        p.sql
    );
    let catalog = crate::pricing::catalog_at(tx, revision.price)?;
    let mut cache = crate::valuation::CacheReader::new(tx, &catalog.revision, &query.price_basis)?;
    let mut statement = tx.prepare(&sql)?;
    let mut result = statement.query(params_from_iter(values))?;
    let mut events = Vec::new();
    while let Some(row) = result.next()? {
        let event_id: String = row.get(0)?;
        let session_key: String = row.get(1)?;
        key(&event_id)?;
        key(&session_key)?;
        let occurred_at_ms = EpochMs::new(row.get(3)?).map_err(|_| ErrorCode::DbCorrupt)?;
        let model: Option<String> = row.get(4)?;
        let provider: Option<String> = row.get(5)?;
        let total: i64 = row.get(14)?;
        let version: String = row.get(18)?;
        let usage = UsageVector {
            input_total: row.get(9)?,
            cached_input: row.get(10)?,
            output_total: row.get(11)?,
            reasoning_output: row.get(12)?,
            reported_total: Some(total),
        };
        if usage.published_total(&version).map_err(|e| {
            if e == ErrorCode::UnsupportedFormat {
                e
            } else {
                ErrorCode::DbCorrupt
            }
        })? != Some(total)
        {
            return Err(ErrorCode::DbCorrupt.into());
        }
        let source_ids: Vec<String> =
            serde_json::from_str(&row.get::<_, String>(19)?).map_err(|_| ErrorCode::DbCorrupt)?;
        if source_ids.len() > 32 {
            return Err(ErrorCode::DbCorrupt.into());
        }
        for source in &source_ids {
            key(source)?;
        }
        let quality_flags: Vec<String> =
            serde_json::from_str(&row.get::<_, String>(16)?).map_err(|_| ErrorCode::DbCorrupt)?;
        if quality_flags.len() > 16
            || quality_flags
                .iter()
                .any(|q| q.is_empty() || q.len() > 64 || q.chars().any(char::is_control))
        {
            return Err(ErrorCode::DbCorrupt.into());
        }
        let pricing_event = PricingEvent {
            provider: provider.as_deref(),
            model: model.as_deref(),
            source_ids: &source_ids,
            occurred_at_ms,
            usage,
        };
        let fingerprint = crate::valuation::fingerprint(&pricing_event, &version)?;
        let price = cache
            .lookup(&event_id, &fingerprint)?
            .unwrap_or_else(|| catalog.estimate(&pricing_event, &query.price_basis));
        events.push(UsageEventRow {
            event_id,
            session_key,
            session_display_name: row.get(2)?,
            occurred_at_ms,
            model,
            provider,
            project_id: row.get(6)?,
            project_display_name: row.get(7)?,
            source_ids,
            turn_id: row.get(8)?,
            total_tokens: DecimalInt::from_nonnegative(total.into())?,
            usage: RawUsageVector::from(UsageVector {
                reported_total: row.get(13)?,
                ..usage
            }),
            raw_last: raw(row.get(20)?)?,
            raw_cumulative: raw(row.get(21)?)?,
            calculation_method: row.get(15)?,
            quality_flags,
            price,
            parser_version: row.get(17)?,
            accounting_version: version,
        });
    }
    let more = events.len() > usize::from(query.page_size);
    events.truncate(usize::from(query.page_size));
    let summary = totals(tx, &query.filter)?;
    let coverage = coverage::coverage(tx, &query.filter, &summary)?;
    let pricing = pricing::summary(tx, &query.filter, &query.price_basis, revision.price)?;
    if pricing
        .priced_total_tokens
        .value()
        .checked_add(pricing.unpriced_total_tokens.value())
        .ok_or(ErrorCode::NumericOverflow)?
        != summary.total_tokens.value()
    {
        return Err(ErrorCode::DbCorrupt.into());
    }
    let (parser_versions, accounting_versions) =
        dashboard::versions(tx, &query.filter, &query.filter)?;
    let id = handle
        .snapshot_id
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    Ok(Page {
        more,
        data: UsageEventsPage {
            meta: SnapshotMeta {
                snapshot_id: format!("query-{id}"),
                data_revision: DecimalInt::from_nonnegative(revision.data.into())?,
                price_revision: DecimalInt::from_nonnegative(revision.price.into())?,
                generated_at_ms: last.map_or(at, |p| p.generated_at_ms),
                parser_versions,
                accounting_versions,
                display_timezone: query.filter.range.timezone.clone(),
            },
            summary,
            pricing,
            coverage,
            events,
            next_cursor: None,
        },
    })
}
impl Database {
    pub fn query_usage_events(
        &self,
        owner: &str,
        request: &UsageEventsRequest,
        at: EpochMs,
    ) -> StoreResult<UsageEventsPage> {
        request.validate()?;
        let binding = QueryBinding::new(owner, &("usage_events", &request.query))?;
        let (handle, last) = match &request.cursor {
            Some(cursor) => {
                let (handle, position) =
                    self.leases().resolve_cursor::<Position>(cursor, &binding)?;
                (handle, Some(position))
            }
            None => (self.leases().open(&binding)?, None),
        };
        let query = request.query.clone();
        let page = self.leases().read(&handle, &binding, move |tx, revision| {
            rows(tx, revision, handle, &query, last.as_ref(), at)
        });
        let mut page = match page {
            Ok(page) => page,
            Err(error) => {
                let _ = self.leases().release(&handle, &binding);
                return Err(error);
            }
        };
        if page.more {
            let last = page.data.events.last().ok_or(ErrorCode::DbCorrupt)?;
            let position = Position {
                event_id: last.event_id.clone(),
                occurred_at_ms: last.occurred_at_ms,
                total_tokens: last.total_tokens.clone(),
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
    pub fn close_usage_events(&self, owner: &str, request: &UsageEventsRequest) -> StoreResult<()> {
        request.validate()?;
        let cursor = request.cursor.as_ref().ok_or(ErrorCode::InvalidQuery)?;
        let binding = QueryBinding::new(owner, &("usage_events", &request.query))?;
        match self.leases().resolve_cursor::<Position>(cursor, &binding) {
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
