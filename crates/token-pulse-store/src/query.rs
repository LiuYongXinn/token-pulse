//! Reusable reads for bundles and leases. All input values are bound parameters.
use crate::{Database, ErrorCode, StoreResult};
use rusqlite::{Row, Transaction, params_from_iter, types::Value};
use token_pulse_core::{
    numeric::DecimalInt,
    protocol::{DimensionSelection, TokenMeasure, TokenTotals, UsageFilter},
    query::{GroupDimension, GroupSort, GroupedUsage},
};

const MODEL_KEY: &str =
    "usage_model_key(json_extract(o.normalized_json,'$.effective_metadata.provider'),e.model)";
const FROM: &str =
    "active_usage_events e JOIN observations o ON o.observation_id=e.origin_observation_id";

pub(crate) struct Predicate {
    pub sql: String,
    pub values: Vec<Value>,
}
impl Predicate {
    fn selection(&mut self, expression: &str, selection: &DimensionSelection) {
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

fn decimal(value: Option<String>) -> StoreResult<Option<DecimalInt>> {
    value
        .map(|s| DecimalInt::parse(&s).map_err(Into::into))
        .transpose()
}
fn zero() -> DecimalInt {
    DecimalInt::from_nonnegative(0).expect("zero")
}

/// Aggregate values and covered-token denominators independently. A known zero
/// component is different from no observed component; empty sets remain unknown.
fn aggregate_sql() -> String {
    let mut fields = vec!["sum_token_decimal(e.total_tokens)".into(), "COUNT(*)".into(), "COUNT(DISTINCT e.session_key)".into(),
        "COUNT(DISTINCT CASE WHEN e.turn_id IS NOT NULL AND e.turn_id<>'' THEN json_array(e.session_key,e.turn_id) END)".into(),
        "COUNT(CASE WHEN e.turn_id IS NOT NULL AND e.turn_id<>'' THEN 1 END)".into()];
    for expression in [
        "e.input_tokens_total",
        "e.cached_input_tokens",
        "CASE WHEN e.input_tokens_total IS NOT NULL AND e.cached_input_tokens IS NOT NULL THEN e.input_tokens_total-e.cached_input_tokens END",
        "e.output_tokens_total",
        "e.reasoning_output_tokens",
    ] {
        fields.push(format!("sum_token_decimal({expression})"));
        fields.push(format!(
            "sum_token_decimal(CASE WHEN ({expression}) IS NOT NULL THEN e.total_tokens END)"
        ));
        fields.push(format!("COUNT({expression})"));
    }
    fields.join(",")
}

fn read_totals(row: &Row<'_>, offset: usize) -> StoreResult<TokenTotals> {
    let total = decimal(row.get(offset)?)?.unwrap_or_else(zero);
    let events: i64 = row.get(offset + 1)?;
    let sessions: i64 = row.get(offset + 2)?;
    let turns: i64 = row.get(offset + 3)?;
    let turn_events: i64 = row.get(offset + 4)?;
    let measure = |i: usize| -> StoreResult<TokenMeasure> {
        let count: i64 = row.get(offset + 5 + i * 3 + 2)?;
        Ok(TokenMeasure {
            value: decimal(row.get(offset + 5 + i * 3)?)?,
            covered_total_tokens: decimal(row.get(offset + 5 + i * 3 + 1)?)?.unwrap_or_else(zero),
            complete: events > 0 && count == events,
        })
    };
    Ok(TokenTotals {
        total_tokens: total,
        input_total: measure(0)?,
        cached_input: measure(1)?,
        noncached_input: measure(2)?,
        output_total: measure(3)?,
        reasoning_output: measure(4)?,
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
    let p = predicate(filter)?;
    let mut statement = tx.prepare(&format!(
        "SELECT {} FROM {FROM} WHERE {}",
        aggregate_sql(),
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
    let p = predicate(filter)?;
    let (key, label, join) = match dimension {
        GroupDimension::Models => (
            MODEL_KEY,
            "CASE WHEN e.model IS NULL THEN '未知模型' ELSE e.model || CASE WHEN json_extract(o.normalized_json,'$.effective_metadata.provider') IS NULL THEN '' ELSE ' · ' || json_extract(o.normalized_json,'$.effective_metadata.provider') END END",
            "",
        ),
        GroupDimension::Projects => (
            "e.project_id",
            "COALESCE(project.user_alias,project.display_name,'未知项目')",
            " LEFT JOIN projects project ON project.project_id=e.project_id",
        ),
    };
    // Decimal strings require numeric ordering by length and then digits, never
    // REAL casts (which lose integers above 2^53) or lexical-only comparisons.
    let order = match sort {
        GroupSort::TotalDesc => "length(amount) DESC, amount DESC, dimension_key ASC",
        GroupSort::NameAsc => "label COLLATE BINARY ASC, dimension_key ASC",
    };
    let sql = format!(
        "SELECT {key} AS dimension_key,MIN({label}) AS label,sum_token_decimal(e.total_tokens) AS amount,{} FROM {FROM}{join} WHERE {} GROUP BY {key} ORDER BY {order} LIMIT ?",
        aggregate_sql(),
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
