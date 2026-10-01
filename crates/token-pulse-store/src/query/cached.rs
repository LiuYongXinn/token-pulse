//! Full UTC hours use a matching immutable cache; boundary hours retain facts.
use super::{FROM, Predicate, VECTOR_SUM, predicate};
use crate::{StoreResult, rollup::CACHE_VERSION};
use rusqlite::{Row, Transaction, params_from_iter, types::Value};
use token_pulse_core::{
    protocol::{DimensionSelection, TokenTotals, UsageFilter},
    query::{GroupDimension, GroupSort, GroupedUsage},
};

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

#[derive(Clone, Copy)]
enum Partition {
    Total,
    Models,
    Projects,
}

struct Plan {
    ctes: String,
    values: Vec<Value>,
}

fn plan(
    tx: &Transaction<'_>,
    filter: &UsageFilter,
    partition: Partition,
) -> StoreResult<Option<Plan>> {
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
    let raw_from = if matches!(filter.models, DimensionSelection::All {})
        && !matches!(partition, Partition::Models)
    {
        "active_usage_events e"
    } else {
        FROM
    };
    // Only the fields needed for repeated aggregate/identity reads are retained.
    // SQLite owns this fixed transaction materialization, never the renderer.
    let (cache_key, raw_key, cache_label, raw_label, cache_join, raw_join) = match partition {
        Partition::Total => ("1", "1", "''", "''", "", ""),
        Partition::Models => (
            "usage_model_key(h.model_provider,h.model)",
            super::MODEL_KEY,
            "CASE WHEN h.model IS NULL THEN '未知模型' ELSE h.model || CASE WHEN h.model_provider IS NULL THEN '' ELSE ' · ' || h.model_provider END END",
            "CASE WHEN e.model IS NULL THEN '未知模型' ELSE e.model || CASE WHEN json_extract(o.normalized_json,'$.effective_metadata.provider') IS NULL THEN '' ELSE ' · ' || json_extract(o.normalized_json,'$.effective_metadata.provider') END END",
            "",
            "",
        ),
        Partition::Projects => (
            "h.project_id",
            "e.project_id",
            "COALESCE(project.user_alias,project.display_name,'未知项目')",
            "COALESCE(project.user_alias,project.display_name,'未知项目')",
            " LEFT JOIN projects project ON project.project_id=h.project_id",
            " LEFT JOIN projects project ON project.project_id=e.project_id",
        ),
    };
    let group = if matches!(partition, Partition::Total) {
        ""
    } else {
        " GROUP BY partition_key"
    };
    let ctes=format!("WITH ready AS MATERIALIZED ({READY}),
      cached AS MATERIALIZED (SELECT h.*,s.session_key,{cache_key} AS partition_key,{cache_label} AS label FROM utc_hour_usage_rollups h JOIN ready rs ON rs.set_id=h.set_id JOIN sessions s ON s.active_ledger_id=rs.ledger_id{cache_join} WHERE {}),
      raw AS MATERIALIZED (SELECT e.session_key,e.turn_id,e.input_tokens_total,e.cached_input_tokens,e.output_tokens_total,e.reasoning_output_tokens,e.total_tokens,{raw_key} AS partition_key,{raw_label} AS label FROM {raw_from}{raw_join} WHERE {}),
      raw_summary AS (SELECT {raw_partition} AS partition_key,MIN(label) AS label,{VECTOR_SUM} AS sums,COUNT(*) AS events,COUNT(CASE WHEN e.turn_id IS NOT NULL AND e.turn_id<>'' THEN 1 END) AS known FROM raw e{group}),
      fragments AS (SELECT partition_key,label,token_sums_json AS sums,usage_event_count AS events,known_turn_event_count AS known FROM cached UNION ALL SELECT partition_key,label,sums,events,known FROM raw_summary),
      summary AS (SELECT {raw_partition} AS partition_key,MIN(label) AS label,sum_usage_projection(sums,events) AS sums,sum_token_decimal(events) AS events,sum_token_decimal(known) AS known FROM fragments{group}),
      session_ids AS (SELECT partition_key,session_key FROM cached UNION SELECT partition_key,session_key FROM raw),
      turn_ids AS (SELECT c.partition_key,c.session_key,t.turn_id FROM cached c JOIN utc_hour_rollup_turns t ON t.set_id=c.set_id AND t.hour_start_ms=c.hour_start_ms AND t.cohort_key=c.cohort_key UNION SELECT partition_key,session_key,turn_id FROM raw WHERE turn_id IS NOT NULL AND turn_id<>''),
      session_counts AS (SELECT partition_key,COUNT(*) AS count FROM session_ids GROUP BY partition_key),
      turn_counts AS (SELECT partition_key,COUNT(*) AS count FROM turn_ids GROUP BY partition_key)", cached.sql,raw.sql,raw_partition=if matches!(partition, Partition::Total) { "1" } else { "partition_key" });
    Ok(Some(Plan { ctes, values }))
}

const RESULT: &str = "SELECT summary.partition_key,summary.label,json_extract(summary.sums,'$.total') AS amount,summary.sums,summary.events,COALESCE(sc.count,0),COALESCE(tc.count,0),summary.known FROM summary LEFT JOIN session_counts sc ON sc.partition_key IS summary.partition_key LEFT JOIN turn_counts tc ON tc.partition_key IS summary.partition_key";

pub(super) fn try_totals(
    tx: &Transaction<'_>,
    filter: &UsageFilter,
) -> StoreResult<Option<TokenTotals>> {
    let Some(plan) = plan(tx, filter, Partition::Total)? else {
        return Ok(None);
    };
    let mut statement = tx.prepare(&format!("{} {RESULT}", plan.ctes))?;
    let mut rows = statement.query(params_from_iter(plan.values))?;
    Ok(Some(read_totals(
        rows.next()?.ok_or(crate::ErrorCode::DbCorrupt)?,
        3,
    )?))
}

pub(super) fn try_grouped(
    tx: &Transaction<'_>,
    filter: &UsageFilter,
    dimension: GroupDimension,
    sort: GroupSort,
    limit: usize,
) -> StoreResult<Option<Vec<GroupedUsage>>> {
    let partition = match dimension {
        GroupDimension::Models => Partition::Models,
        GroupDimension::Projects => Partition::Projects,
    };
    let Some(mut plan) = plan(tx, filter, partition)? else {
        return Ok(None);
    };
    let order = match sort {
        GroupSort::TotalDesc => "length(amount) DESC,amount DESC,summary.partition_key ASC",
        GroupSort::NameAsc => "summary.label COLLATE BINARY ASC,summary.partition_key ASC",
    };
    plan.values.push(Value::Integer(limit as i64));
    let mut statement = tx.prepare(&format!("{} {RESULT} ORDER BY {order} LIMIT ?", plan.ctes))?;
    let mut rows = statement.query(params_from_iter(plan.values))?;
    let mut result = Vec::new();
    while let Some(row) = rows.next()? {
        result.push(GroupedUsage {
            key: row.get(0)?,
            display_name: row.get(1)?,
            totals: read_totals(row, 3)?,
        });
    }
    Ok(Some(result))
}

fn read_totals(row: &Row<'_>, offset: usize) -> StoreResult<TokenTotals> {
    // Event/known counts aggregate beyond i64, so adapt through a dedicated
    // reader rather than casting the exact decimal strings to SQLite integers.
    let sums: crate::aggregate::TokenSums = serde_json::from_str(&row.get::<_, String>(offset)?)?;
    let events = token_pulse_core::numeric::DecimalInt::parse(&row.get::<_, String>(offset + 1)?)?;
    let known = token_pulse_core::numeric::DecimalInt::parse(&row.get::<_, String>(offset + 4)?)?;
    let sessions: i64 = row.get(offset + 2)?;
    let turns: i64 = row.get(offset + 3)?;
    let [
        input_total,
        cached_input,
        noncached_input,
        output_total,
        reasoning_output,
    ] = sums.measures;
    Ok(TokenTotals {
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
    })
}
