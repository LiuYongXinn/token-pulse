//! Bounded session pages retain facts, relations and prices in one real lease.
use super::{aggregate_sql, context, coverage, fact_from, predicate, read_totals};
use super::{dashboard, pricing, totals};
use crate::{Database, ErrorCode, Revision, StoreResult, leases::cursor::QueryBinding};
use rusqlite::{Transaction, params_from_iter, types::Value};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use token_pulse_core::{
    numeric::{DecimalInt, EpochMs},
    pricing::PricingAccumulator,
    protocol::{DimensionSelection, PricingSummary, SnapshotMeta, TokenTotals},
    query::{SessionRow, SessionSort, SessionsPage, SessionsQuery, SessionsRequest},
};
mod bundle;
mod turns;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Position {
    session_key: String,
    latest_at_ms: EpochMs,
    total_tokens: DecimalInt,
    generated_at_ms: EpochMs,
}
struct Page {
    data: SessionsPage,
    more: bool,
}
fn check_price(totals: &TokenTotals, price: &PricingSummary) -> StoreResult<()> {
    if price
        .priced_total_tokens
        .value()
        .checked_add(price.unpriced_total_tokens.value())
        .ok_or(ErrorCode::NumericOverflow)?
        != totals.total_tokens.value()
    {
        return Err(ErrorCode::DbCorrupt.into());
    }
    Ok(())
}
fn rows(
    tx: &Transaction<'_>,
    revision: Revision,
    snapshot_id: String,
    query: &SessionsQuery,
    last: Option<&Position>,
    at: EpochMs,
    common: &super::summary_cache::ScopeSummary,
) -> StoreResult<Page> {
    let filter = &query.filter;
    let p = predicate(filter)?;
    let mut values = p.values;
    // Padding canonical decimal strings to the same width preserves exact i128
    // order without SQLite REAL/integer coercion. i128 has at most 39 digits.
    let total_key = "printf('%039s',json_extract(vector_sums,'$.total')) COLLATE BINARY";
    let order = match query.sort {
        SessionSort::LatestDesc => "latest_at_ms DESC,session_key COLLATE BINARY ASC".to_owned(),
        SessionSort::TotalDesc => format!("{total_key} DESC,session_key COLLATE BINARY ASC"),
    };
    let continuation = if let Some(last) = last {
        values.push(match query.sort {
            SessionSort::LatestDesc => Value::Integer(last.latest_at_ms.value()),
            SessionSort::TotalDesc => Value::Text(format!("{:>39}", last.total_tokens.as_str())),
        });
        values.push(values.last().ok_or(ErrorCode::DbCorrupt)?.clone());
        values.push(Value::Text(last.session_key.clone()));
        let key = match query.sort {
            SessionSort::LatestDesc => "latest_at_ms",
            SessionSort::TotalDesc => total_key,
        };
        format!("WHERE ({key} < ? OR ({key} = ? AND session_key COLLATE BINARY > ?))")
    } else {
        String::new()
    };
    values.push(Value::Integer(i64::from(query.page_size) + 1));
    let sql = format!(
        "WITH selected AS MATERIALIZED (SELECT e.* FROM {} WHERE {}), grouped(session_key,latest_at_ms,vector_sums,events,sessions,turns,turn_events) AS (SELECT e.session_key,MAX(e.occurred_at_ms),{} FROM selected e GROUP BY e.session_key), page AS MATERIALIZED (SELECT * FROM grouped {continuation} ORDER BY {order} LIMIT ?), latest AS (SELECT e.session_key,e.model,e.project_id,ROW_NUMBER() OVER(PARTITION BY e.session_key ORDER BY e.occurred_at_ms DESC,e.event_id COLLATE BINARY ASC) AS rn FROM selected e JOIN page p ON p.session_key=e.session_key) SELECT p.session_key,COALESCE(s.provider_session_id,p.session_key),p.latest_at_ms,l.model,l.project_id,COALESCE(project.user_alias,project.display_name),parent.session_key,COALESCE(parent.provider_session_id,parent.session_key),s.parent_provider_id,(SELECT COUNT(*) FROM sessions child WHERE child.active_ledger_id IS NOT NULL AND COALESCE((SELECT canonical_session_key FROM session_aliases WHERE alias_session_key=child.parent_key),child.parent_key)=s.session_key AND NOT EXISTS(SELECT 1 FROM session_aliases WHERE alias_session_key=child.session_key)),p.vector_sums,p.events,p.sessions,p.turns,p.turn_events FROM page p JOIN sessions s ON s.session_key=p.session_key JOIN latest l ON l.session_key=p.session_key AND l.rn=1 LEFT JOIN projects project ON project.project_id=l.project_id LEFT JOIN sessions parent ON parent.session_key=COALESCE((SELECT canonical_session_key FROM session_aliases WHERE alias_session_key=s.parent_key),s.parent_key) AND parent.active_ledger_id IS NOT NULL ORDER BY {}",
        fact_from(filter, false), p.sql, aggregate_sql(),
        match query.sort { SessionSort::LatestDesc => "p.latest_at_ms DESC,p.session_key COLLATE BINARY ASC".to_owned(), SessionSort::TotalDesc => "printf('%039s',json_extract(p.vector_sums,'$.total')) COLLATE BINARY DESC,p.session_key COLLATE BINARY ASC".to_owned() }
    );
    let mut statement = tx.prepare(&sql)?;
    let mut result = statement.query(params_from_iter(values))?;
    let mut sessions = Vec::new();
    while let Some(row) = result.next()? {
        let key: String = row.get(0)?;
        DimensionSelection::Ids {
            ids: vec![key.clone()],
            include_unknown: false,
        }
        .validate()
        .map_err(|_| ErrorCode::DbCorrupt)?;
        let summary = read_totals(row, 10)?;
        sessions.push(SessionRow {
            session_key: key.clone(),
            display_name: row.get(1)?,
            latest_at_ms: EpochMs::new(row.get(2)?)?,
            latest_model: row.get(3)?,
            latest_project_id: row.get(4)?,
            latest_project_name: row.get(5)?,
            parent_key: row.get(6)?,
            parent_display_name: row.get(7)?,
            parent_provider_id: row.get(8)?,
            child_count: DecimalInt::from_nonnegative(i128::from(row.get::<_, i64>(9)?))?,
            coverage: coverage::coverage(
                tx,
                &token_pulse_core::protocol::UsageFilter {
                    sessions: DimensionSelection::Ids {
                        ids: vec![key.clone()],
                        include_unknown: false,
                    },
                    ..filter.clone()
                },
                &summary,
            )?,
            summary,
            pricing: PricingAccumulator::new(query.price_basis.clone()).summary(false)?,
            latest_context: context::latest_context(tx, &key)?,
        });
    }
    let more = sessions.len() > usize::from(query.page_size);
    sessions.truncate(usize::from(query.page_size));
    for session in &mut sessions {
        session.pricing = common
            .sessions
            .get(&session.session_key)
            .ok_or(ErrorCode::DbCorrupt)?
            .clone();
        check_price(&session.summary, &session.pricing)?;
    }
    let summary = common.totals.clone();
    let coverage = common.coverage.clone();
    let pricing = common.pricing.clone();
    check_price(&summary, &pricing)?;
    let (parser_versions, accounting_versions) =
        (common.parsers.clone(), common.accounting.clone());
    Ok(Page {
        more,
        data: SessionsPage {
            meta: SnapshotMeta {
                snapshot_id,
                data_revision: DecimalInt::from_nonnegative(revision.data.into())?,
                price_revision: DecimalInt::from_nonnegative(revision.price.into())?,
                generated_at_ms: last.map_or(at, |p| p.generated_at_ms),
                parser_versions,
                accounting_versions,
                display_timezone: filter.range.timezone.clone(),
            },
            summary,
            pricing,
            coverage,
            sessions,
            next_cursor: None,
        },
    })
}
impl Database {
    pub fn close_sessions(&self, owner: &str, request: &SessionsRequest) -> StoreResult<()> {
        request.validate()?;
        let cursor = request.cursor.as_ref().ok_or(ErrorCode::InvalidQuery)?;
        let binding = QueryBinding::new(owner, &("sessions", &request.query))?;
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
    pub fn query_sessions(
        &self,
        owner: &str,
        request: &SessionsRequest,
        at: EpochMs,
    ) -> StoreResult<SessionsPage> {
        request.validate()?;
        let binding = QueryBinding::new(owner, &("sessions", &request.query))?;
        let (handle, last) = match &request.cursor {
            Some(cursor) => {
                let (handle, position) =
                    self.leases().resolve_cursor::<Position>(cursor, &binding)?;
                (handle, Some(position))
            }
            None => (self.leases().open(&binding)?, None),
        };
        let query = request.query.clone();
        let snapshot_id = format!(
            "query-{}",
            handle
                .snapshot_id
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        let database = self.clone();
        let page = self.leases().read_summary(
            &handle,
            &binding,
            database,
            query.filter.clone(),
            query.price_basis.clone(),
            move |tx, revision, common| {
                rows(tx, revision, snapshot_id, &query, last.as_ref(), at, common)
            },
        );
        let mut page = match page {
            Ok(page) => page,
            Err(error) => {
                let _ = self.leases().release(&handle, &binding);
                return Err(error);
            }
        };
        if page.more {
            let last = page.data.sessions.last().ok_or(ErrorCode::DbCorrupt)?;
            let position = Position {
                session_key: last.session_key.clone(),
                latest_at_ms: last.latest_at_ms,
                total_tokens: last.summary.total_tokens.clone(),
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
}

#[cfg(test)]
mod tests;
