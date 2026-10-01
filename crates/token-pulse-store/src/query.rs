//! Reusable reads for bundles and leases. All input values are bound parameters.
use crate::{Database, ErrorCode, StoreResult};
use rusqlite::{
    Connection, Row, Transaction, functions::FunctionFlags, params_from_iter, types::Value,
};
use token_pulse_core::{
    calendar::{self, CalendarBucket, Grain},
    numeric::DecimalInt,
    protocol::{DimensionSelection, TokenMeasure, TokenTotals, UsageFilter},
    query::{GroupDimension, GroupSort, GroupedUsage},
};

mod cached;

pub struct BucketTotals {
    pub bucket: CalendarBucket,
    pub totals: TokenTotals,
}

const MODEL_KEY: &str =
    "usage_model_key(json_extract(o.normalized_json,'$.effective_metadata.provider'),e.model)";
const FROM: &str =
    "active_usage_events e JOIN observations o ON o.observation_id=e.origin_observation_id";
const VECTOR_SUM: &str = "sum_usage_vector(e.input_tokens_total,e.cached_input_tokens,e.output_tokens_total,e.reasoning_output_tokens,e.total_tokens)";

fn fact_from(filter: &UsageFilter, require_model: bool) -> &'static str {
    if require_model || !matches!(filter.models, DimensionSelection::All {}) {
        FROM
    } else {
        "active_usage_events e"
    }
}

pub(crate) struct Predicate {
    pub sql: String,
    pub values: Vec<Value>,
}
impl Predicate {
    pub(crate) fn selection(&mut self, expression: &str, selection: &DimensionSelection) {
        if let DimensionSelection::Ids {
            ids,
            include_unknown,
        } = selection
        {
            let names = ids
                .iter()
                .map(|id| {
                    self.values.push(Value::Text(id.clone()));
                    "?"
                })
                .collect::<Vec<_>>()
                .join(",");
            let known = if ids.is_empty() {
                "0".into()
            } else {
                format!("{expression} IN ({names})")
            };
            self.sql.push_str(&if *include_unknown {
                format!(" AND ({known} OR {expression} IS NULL)")
            } else {
                format!(" AND ({known})")
            });
        }
    }
}

pub(crate) fn predicate(filter: &UsageFilter) -> StoreResult<Predicate> {
    filter.validate()?;
    let mut p = Predicate {
        sql: "e.occurred_at_ms>=? AND e.occurred_at_ms<?".into(),
        values: vec![
            Value::Integer(filter.range.start_ms.value()),
            Value::Integer(filter.range.end_ms.value()),
        ],
    };
    p.selection(MODEL_KEY, &filter.models);
    p.selection("e.project_id", &filter.projects);
    // Aliases are resolved inside this transaction, including in old snapshots.
    if let DimensionSelection::Ids { ids, .. } = &filter.sessions {
        if ids.is_empty() {
            p.sql.push_str(" AND 0");
        } else {
            let clauses = ids.iter().map(|id| {
                p.values.push(Value::Text(id.clone()));
                p.values.push(Value::Text(id.clone()));
                "e.session_key=COALESCE((SELECT canonical_session_key FROM session_aliases WHERE alias_session_key=?),?)"
            }).collect::<Vec<_>>();
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
            p.sql.push_str(&format!(" AND EXISTS(SELECT 1 FROM event_provenance ep JOIN observations po ON po.observation_id=ep.observation_id JOIN file_generations fg ON fg.file_generation_id=po.file_generation_id JOIN source_files sf ON sf.file_id=fg.file_id WHERE ep.event_id=e.event_id AND sf.source_id IN ({names}))"));
        }
    }
    Ok(p)
}

fn zero() -> DecimalInt {
    DecimalInt::from_nonnegative(0).expect("zero")
}

fn empty_totals() -> TokenTotals {
    let measure = || TokenMeasure {
        value: None,
        covered_total_tokens: zero(),
        complete: false,
    };
    TokenTotals {
        total_tokens: zero(),
        input_total: measure(),
        cached_input: measure(),
        noncached_input: measure(),
        output_total: measure(),
        reasoning_output: measure(),
        session_count: zero(),
        usage_event_count: zero(),
        reliable_turn_count: None,
        reliable_turns_complete: false,
    }
}

fn series_sql(filter: &UsageFilter) -> StoreResult<(String, Vec<Value>)> {
    let p = predicate(filter)?;
    let sql = format!(
        "SELECT current_usage_bucket(e.occurred_at_ms) AS bucket_index,{} FROM {} WHERE {} GROUP BY bucket_index ORDER BY bucket_index",
        aggregate_sql(),
        fact_from(filter, false),
        p.sql
    );
    Ok((sql, p.values))
}

struct BucketFunction<'a>(&'a Connection);
impl Drop for BucketFunction<'_> {
    fn drop(&mut self) {
        let _ = self.0.remove_function("current_usage_bucket", 1);
    }
}

fn bucket_function<'a>(
    conn: &'a Connection,
    bins: &[CalendarBucket],
) -> StoreResult<BucketFunction<'a>> {
    let ends = bins.iter().map(|b| b.end_ms.value()).collect::<Vec<_>>();
    let start = bins
        .first()
        .ok_or(ErrorCode::InvalidQuery)?
        .start_ms
        .value();
    conn.create_scalar_function(
        "current_usage_bucket",
        1,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        move |ctx| {
            let time: i64 = ctx.get(0)?;
            let index = ends.partition_point(|end| *end <= time);
            if time < start || index >= ends.len() {
                return Err(rusqlite::Error::UserFunctionError(Box::new(
                    ErrorCode::InvalidQuery,
                )));
            }
            Ok(index as i64)
        },
    )?;
    Ok(BucketFunction(conn))
}

/// One controlled range query, never all raw events in the renderer or 2000
/// separate transactions. The caller can query summary and heatmap on this tx.
pub fn series(
    tx: &Transaction<'_>,
    filter: &UsageFilter,
    grain: Grain,
) -> StoreResult<Vec<BucketTotals>> {
    let bins = calendar::buckets(&filter.range, grain)?;
    // This connection belongs exclusively to the current read transaction.
    // Function capture is immutable and removed after all statements drop.
    let _function = bucket_function(tx, &bins)?;
    let (sql, values) = series_sql(filter)?;
    let mut result = bins
        .into_iter()
        .map(|bucket| BucketTotals {
            bucket,
            totals: empty_totals(),
        })
        .collect::<Vec<_>>();
    let mut statement = tx.prepare(&sql)?;
    let mut rows = statement.query(params_from_iter(values))?;
    while let Some(row) = rows.next()? {
        let index = usize::try_from(row.get::<_, i64>(0)?).map_err(|_| ErrorCode::DbCorrupt)?;
        result.get_mut(index).ok_or(ErrorCode::DbCorrupt)?.totals = read_totals(row, 1)?;
    }
    Ok(result)
}

/// Aggregate values and covered-token denominators independently. A known zero
/// component is different from no observed component; empty sets remain unknown.
fn aggregate_sql() -> String {
    format!(
        "{VECTOR_SUM},COUNT(*),COUNT(DISTINCT e.session_key),COUNT(DISTINCT CASE WHEN e.turn_id IS NOT NULL AND e.turn_id<>'' THEN json_array(e.session_key,e.turn_id) END),COUNT(CASE WHEN e.turn_id IS NOT NULL AND e.turn_id<>'' THEN 1 END)"
    )
}

pub(crate) fn read_totals(row: &Row<'_>, offset: usize) -> StoreResult<TokenTotals> {
    let sums: crate::aggregate::TokenSums =
        serde_json::from_str(&row.get::<_, String>(offset)?).map_err(|_| ErrorCode::DbCorrupt)?;
    let events: i64 = row.get(offset + 1)?;
    let sessions: i64 = row.get(offset + 2)?;
    let turns: i64 = row.get(offset + 3)?;
    let turn_events: i64 = row.get(offset + 4)?;
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
        session_count: DecimalInt::from_nonnegative(sessions.into())?,
        usage_event_count: DecimalInt::from_nonnegative(events.into())?,
        reliable_turn_count: if turns == 0 {
            None
        } else {
            Some(DecimalInt::from_nonnegative(turns.into())?)
        },
        reliable_turns_complete: events > 0 && turn_events == events,
    })
}

pub fn totals(tx: &Transaction<'_>, filter: &UsageFilter) -> StoreResult<TokenTotals> {
    if let Some(totals) = cached::try_totals(tx, filter)? {
        return Ok(totals);
    }
    raw_totals(tx, filter)
}

pub(crate) fn raw_totals(tx: &Transaction<'_>, filter: &UsageFilter) -> StoreResult<TokenTotals> {
    let p = predicate(filter)?;
    let mut statement = tx.prepare(&format!(
        "SELECT {} FROM {} WHERE {}",
        aggregate_sql(),
        fact_from(filter, false),
        p.sql
    ))?;
    let mut rows = statement.query(params_from_iter(p.values))?;
    read_totals(rows.next()?.ok_or(ErrorCode::DbCorrupt)?, 0)
}

pub fn grouped(
    tx: &Transaction<'_>,
    filter: &UsageFilter,
    dimension: GroupDimension,
    sort: GroupSort,
    limit: usize,
) -> StoreResult<Vec<GroupedUsage>> {
    if !(1..=200).contains(&limit) {
        return Err(ErrorCode::InvalidQuery.into());
    }
    if let Some(groups) = cached::try_grouped(tx, filter, dimension, sort, limit)? {
        return Ok(groups);
    }
    raw_grouped(tx, filter, dimension, sort, limit)
}

pub(crate) fn raw_grouped(
    tx: &Transaction<'_>,
    filter: &UsageFilter,
    dimension: GroupDimension,
    sort: GroupSort,
    limit: usize,
) -> StoreResult<Vec<GroupedUsage>> {
    if !(1..=200).contains(&limit) {
        return Err(ErrorCode::InvalidQuery.into());
    }
    let p = predicate(filter)?;
    let (key, label, join, group_by) = match dimension {
        GroupDimension::Models => (
            MODEL_KEY,
            "CASE WHEN e.model IS NULL THEN '未知模型' ELSE e.model || CASE WHEN json_extract(o.normalized_json,'$.effective_metadata.provider') IS NULL THEN '' ELSE ' · ' || json_extract(o.normalized_json,'$.effective_metadata.provider') END END",
            "",
            "e.model,CASE WHEN e.model IS NULL THEN NULL ELSE json_extract(o.normalized_json,'$.effective_metadata.provider') END",
        ),
        GroupDimension::Projects => (
            "e.project_id",
            "COALESCE(project.user_alias,project.display_name,'未知项目')",
            " LEFT JOIN projects project ON project.project_id=e.project_id",
            "e.project_id",
        ),
    };
    // Decimal strings require numeric ordering by length and then digits, never
    // REAL casts (which lose integers above 2^53) or lexical-only comparisons.
    let order = match sort {
        GroupSort::TotalDesc => "length(amount) DESC, amount DESC, dimension_key ASC",
        GroupSort::NameAsc => "label COLLATE BINARY ASC, dimension_key ASC",
    };
    let sql = format!(
        "SELECT {key} AS dimension_key,MIN({label}) AS label,json_extract({VECTOR_SUM},'$.total') AS amount,{} FROM {}{join} WHERE {} GROUP BY {group_by} ORDER BY {order} LIMIT ?",
        aggregate_sql(),
        fact_from(filter, matches!(dimension, GroupDimension::Models)),
        p.sql
    );
    let mut values = p.values;
    values.push(Value::Integer(limit as i64));
    let mut statement = tx.prepare(&sql)?;
    let mut rows = statement.query(params_from_iter(values))?;
    let mut result = Vec::new();
    while let Some(row) = rows.next()? {
        result.push(GroupedUsage {
            key: row.get(0)?,
            display_name: row.get(1)?,
            totals: read_totals(row, 3)?,
        });
    }
    Ok(result)
}

impl Database {
    pub fn usage_totals(&self, filter: &UsageFilter) -> StoreResult<TokenTotals> {
        self.snapshot(|tx, _| totals(tx, filter))
    }
}

#[cfg(test)]
mod tests;
