//! Full UTC hours use a matching immutable cache; boundary hours retain facts.
use super::{FROM, Predicate, VECTOR_SUM, predicate};
use crate::{StoreResult, rollup::CACHE_VERSION};
use rusqlite::{Transaction, params_from_iter, types::Value};
use token_pulse_core::protocol::{DimensionSelection, TokenTotals, UsageFilter};

const READY: &str = "SELECT rs.set_id,rs.ledger_id FROM usage_rollup_sets rs JOIN ledger_usage_versions lv ON lv.ledger_id=rs.ledger_id AND lv.revision=rs.evidence_revision JOIN ledger_generations lg ON lg.ledger_id=rs.ledger_id AND lg.parser_version=rs.parser_version AND lg.accounting_version=rs.accounting_version JOIN sessions owner ON owner.active_ledger_id=rs.ledger_id WHERE rs.state='ready' AND lg.state='active' AND rs.cache_version=?";

#[cfg(test)]
mod tests;

fn cached_predicate(filter: &UsageFilter, start: i64, end: i64) -> StoreResult<Predicate> {
    filter.validate()?;
    let mut p = Predicate {
        sql: "h.hour_start_ms>=? AND h.hour_start_ms<?".into(),
        values: vec![Value::Integer(start), Value::Integer(end)],
    };
    p.selection("usage_model_key(h.model_provider,h.model)", &filter.models);
    p.selection("h.project_id", &filter.projects);
    if let DimensionSelection::Ids { ids, .. } = &filter.sessions {
        if ids.is_empty() {
            p.sql.push_str(" AND 0");
        } else {
            let clauses=ids.iter().map(|id| {p.values.push(Value::Text(id.clone()));p.values.push(Value::Text(id.clone()));"s.session_key=COALESCE((SELECT canonical_session_key FROM session_aliases WHERE alias_session_key=?),?)"}).collect::<Vec<_>>();
            p.sql.push_str(&format!(" AND ({})", clauses.join(" OR ")));
        }
    }
    if let DimensionSelection::Ids { ids, .. } = &filter.sources {
        if ids.is_empty() {
            p.sql.push_str(" AND 0");
        } else {
            let names = ids
                .iter()
                .map(|id| {
                    p.values.push(Value::Text(id.clone()));
                    "?"
                })
                .collect::<Vec<_>>()
                .join(",");
            p.sql.push_str(&format!(" AND EXISTS(SELECT 1 FROM json_each(h.source_ids_json) source WHERE source.value IN ({names}))"));
        }
    }
    Ok(p)
}

pub(super) fn try_totals(
    tx: &Transaction<'_>,
    filter: &UsageFilter,
) -> StoreResult<Option<TokenTotals>> {
    filter.validate()?;
    let hour = 3_600_000i64;
    let floor = filter.range.start_ms.value().div_euclid(hour) * hour;
    let start = if floor == filter.range.start_ms.value() {
        floor
    } else {
        floor + hour
    };
    let end = filter.range.end_ms.value().div_euclid(hour) * hour;
    if start >= end {
        return Ok(None);
    }
    let any: bool = tx.query_row(&format!("SELECT EXISTS({READY})"), [CACHE_VERSION], |r| {
        r.get(0)
    })?;
    if !any {
        return Ok(None);
    }
    let cached = cached_predicate(filter, start, end)?;
    let mut raw = predicate(filter)?;
    let mut values = vec![Value::Integer(CACHE_VERSION)];
    values.extend(cached.values);
    values.extend(raw.values);
    // In an aligned range, reject ready sessions before their fact index scan.
    // Boundary fragments use the original half-open range and exact predicates.
    let aligned = start == filter.range.start_ms.value() && end == filter.range.end_ms.value();
    if aligned {
        raw.sql.push_str(" AND e.session_key IN(SELECT session_key FROM sessions WHERE active_ledger_id NOT IN(SELECT ledger_id FROM ready))");
    } else {
        raw.sql.push_str(" AND (e.session_key IN(SELECT session_key FROM sessions WHERE active_ledger_id NOT IN(SELECT ledger_id FROM ready)) OR e.occurred_at_ms<? OR e.occurred_at_ms>=?)");
        values.push(Value::Integer(start));
        values.push(Value::Integer(end));
    }
    let raw_from = if matches!(filter.models, DimensionSelection::All {}) {
        "active_usage_events e"
    } else {
        FROM
    };
    // Only the fields needed for repeated aggregate/identity reads are retained.
    // SQLite owns this fixed transaction materialization, never the renderer.
    let sql=format!("WITH ready AS MATERIALIZED ({READY}),
      cached AS MATERIALIZED (SELECT h.*,s.session_key FROM utc_hour_usage_rollups h JOIN ready rs ON rs.set_id=h.set_id JOIN sessions s ON s.active_ledger_id=rs.ledger_id WHERE {}),
      raw AS MATERIALIZED (SELECT e.session_key,e.turn_id,e.input_tokens_total,e.cached_input_tokens,e.output_tokens_total,e.reasoning_output_tokens,e.total_tokens FROM {raw_from} WHERE {}),
      raw_summary AS (SELECT {VECTOR_SUM} AS sums,COUNT(*) AS events,COUNT(CASE WHEN e.turn_id IS NOT NULL AND e.turn_id<>'' THEN 1 END) AS known FROM raw e),
      fragments AS (SELECT token_sums_json AS sums,usage_event_count AS events,known_turn_event_count AS known FROM cached UNION ALL SELECT sums,events,known FROM raw_summary),
      summary AS (SELECT sum_usage_projection(sums,events) AS sums,sum_token_decimal(events) AS events,sum_token_decimal(known) AS known FROM fragments),
      session_ids AS (SELECT session_key FROM cached UNION SELECT session_key FROM raw),
      turn_ids AS (SELECT c.session_key,t.turn_id FROM cached c JOIN utc_hour_rollup_turns t ON t.set_id=c.set_id AND t.hour_start_ms=c.hour_start_ms AND t.cohort_key=c.cohort_key UNION SELECT session_key,turn_id FROM raw WHERE turn_id IS NOT NULL AND turn_id<>'')
      SELECT sums,events,(SELECT COUNT(*) FROM session_ids),(SELECT COUNT(*) FROM turn_ids),known FROM summary",cached.sql,raw.sql);
    let mut statement = tx.prepare(&sql)?;
    let mut rows = statement.query(params_from_iter(values))?;
    let row = rows.next()?.ok_or(crate::ErrorCode::DbCorrupt)?;
    // Event/known counts aggregate beyond i64, so adapt through a dedicated
    // reader rather than casting the exact decimal strings to SQLite integers.
    let sums: crate::aggregate::TokenSums = serde_json::from_str(&row.get::<_, String>(0)?)?;
    let events = token_pulse_core::numeric::DecimalInt::parse(&row.get::<_, String>(1)?)?;
    let known = token_pulse_core::numeric::DecimalInt::parse(&row.get::<_, String>(4)?)?;
    let sessions: i64 = row.get(2)?;
    let turns: i64 = row.get(3)?;
    let [
        input_total,
        cached_input,
        noncached_input,
        output_total,
        reasoning_output,
    ] = sums.measures;
    Ok(Some(TokenTotals {
        total_tokens: sums.total,
        input_total,
        cached_input,
        noncached_input,
        output_total,
        reasoning_output,
        session_count: token_pulse_core::numeric::DecimalInt::from_nonnegative(sessions.into())?,
        usage_event_count: events.clone(),
        reliable_turn_count: if turns == 0 {
            None
        } else {
            Some(token_pulse_core::numeric::DecimalInt::from_nonnegative(
                turns.into(),
            )?)
        },
        reliable_turns_complete: events.value() > 0 && known == events,
    }))
}
