//! Coverage gaps are independent of confirmed dated consumption.
use super::Predicate;
use crate::{Database, ErrorCode, StoreResult};
use rusqlite::{Transaction, params_from_iter, types::Value};
use token_pulse_core::{
    numeric::{DecimalInt, EpochMs},
    protocol::{
        Coverage, CoverageState, DimensionSelection, FormatIssue, SourceIssue, TokenTotals,
        UsageFilter,
    },
    sources::SourceReadability,
};

fn source_selection(filter: &UsageFilter, expression: &str) -> Predicate {
    let mut p = Predicate {
        sql: "1".into(),
        values: vec![],
    };
    match &filter.sources {
        DimensionSelection::All {} => {}
        DimensionSelection::Ids { ids, .. } => p.selection(
            expression,
            &DimensionSelection::Ids {
                ids: ids.clone(),
                include_unknown: false,
            },
        ),
    }
    p
}

fn pending_predicate(filter: &UsageFilter) -> StoreResult<Predicate> {
    filter.validate()?;
    let mut p = Predicate {
        sql: "(o.observed_at_ms IS NULL OR (o.observed_at_ms>=? AND o.observed_at_ms<?))".into(),
        values: vec![
            Value::Integer(filter.range.start_ms.value()),
            Value::Integer(filter.range.end_ms.value()),
        ],
    };
    p.selection(
        "usage_model_key(json_extract(o.normalized_json,'$.effective_metadata.provider'),o.model)",
        &filter.models,
    );
    p.selection("o.project_id", &filter.projects);
    p.selection("sf.source_id", &filter.sources);
    if let DimensionSelection::Ids { ids, .. } = &filter.sessions {
        if ids.is_empty() {
            p.sql.push_str(" AND 0");
        } else {
            let clauses=ids.iter().map(|id|{p.values.push(Value::Text(id.clone()));p.values.push(Value::Text(id.clone()));"s.session_key=COALESCE((SELECT canonical_session_key FROM session_aliases WHERE alias_session_key=?),?)"}).collect::<Vec<_>>();
            p.sql.push_str(&format!(" AND ({})", clauses.join(" OR ")));
        }
    }
    Ok(p)
}

pub fn coverage(
    tx: &Transaction<'_>,
    filter: &UsageFilter,
    totals: &TokenTotals,
) -> StoreResult<Coverage> {
    let p = pending_predicate(filter)?;
    let sql = format!(
        "WITH selected AS MATERIALIZED (SELECT p.observation_id,p.kind,p.vector_json FROM pending_usage p JOIN sessions s ON s.active_ledger_id=p.ledger_id JOIN observations o ON o.observation_id=p.observation_id JOIN file_generations fg ON fg.file_generation_id=o.file_generation_id JOIN source_files sf ON sf.file_id=fg.file_id WHERE {} AND p.kind IN ('pending','unattributed')) SELECT COUNT(DISTINCT CASE WHEN kind='pending' THEN observation_id END),COUNT(DISTINCT CASE WHEN kind='unattributed' THEN observation_id END),(SELECT sum_token_decimal(usage_vector_total(vector_json)) FROM selected WHERE kind='unattributed'),COUNT(CASE WHEN kind='unattributed' AND usage_vector_total(vector_json) IS NULL THEN 1 END) FROM selected",
        p.sql
    );
    let (pending, unattributed, known_total, missing): (i64, i64, Option<String>, i64) = tx
        .query_row(&sql, params_from_iter(p.values), |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?;
    let source = source_selection(filter, "sf.source_id");
    let pending_files:i64=tx.query_row(&format!("SELECT COUNT(*) FROM source_files sf LEFT JOIN file_generations fg ON fg.file_generation_id=sf.current_generation_id WHERE {} AND (fg.file_generation_id IS NULL OR fg.state<>'current' OR fg.committed_offset<fg.observed_size OR sf.status NOT IN ('present','known'))",source.sql),params_from_iter(source.values),|r|r.get(0))?;

    // A file with unknown time/identity can belong to the selected date/session.
    // Whole-source health gaps must not disappear behind a model/date filter.
    let source = source_selection(filter, "source_id");
    let mut statement=tx.prepare(&format!("SELECT source_id,enabled,readability,last_success_at_ms FROM sources WHERE {} ORDER BY source_id COLLATE BINARY",source.sql))?;
    let mut rows = statement.query(params_from_iter(source.values))?;
    let mut source_issues = Vec::new();
    let mut known_source_gap = false;
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let enabled: bool = row.get(1)?;
        let readability: SourceReadability =
            serde_json::from_value(serde_json::Value::String(row.get(2)?))
                .map_err(|_| ErrorCode::DbCorrupt)?;
        let time: Option<i64> = row.get(3)?;
        let code = if !enabled {
            known_source_gap = true;
            "source_paused"
        } else {
            match readability {
                SourceReadability::Readable => "scan_evidence_missing",
                SourceReadability::AwaitingDirectory => "source_awaiting_directory",
                SourceReadability::PartiallyReadable => {
                    known_source_gap = true;
                    "source_partially_readable"
                }
                SourceReadability::Unreadable => {
                    known_source_gap = true;
                    "source_unreadable"
                }
                SourceReadability::Disabled => {
                    known_source_gap = true;
                    "source_paused"
                }
            }
        };
        source_issues.push(SourceIssue {
            source_id: id,
            code: code.into(),
            last_success_ms: time.map(EpochMs::new).transpose()?,
        });
    }
    drop(rows);
    drop(statement);
    let source = source_selection(filter, "d.source_id");
    let mut statement=tx.prepare(&format!("SELECT COALESCE(json_extract(d.metadata_json,'$.parser_version'),'unknown') AS format,sum_token_decimal(d.occurrences) FROM diagnostics d WHERE {} AND d.code='UNSUPPORTED_FORMAT' AND d.resolved_at_ms IS NULL GROUP BY format ORDER BY format COLLATE BINARY",source.sql))?;
    let mut rows = statement.query(params_from_iter(source.values))?;
    let mut format_issues = Vec::new();
    while let Some(row) = rows.next()? {
        format_issues.push(FormatIssue {
            format: row.get(0)?,
            count: DecimalInt::parse(&row.get::<_, String>(1)?)?,
        });
    }
    let state = if pending > 0
        || unattributed > 0
        || pending_files > 0
        || known_source_gap
        || !format_issues.is_empty()
    {
        CoverageState::Partial
    } else {
        CoverageState::Unknown
    };
    // Full scan manifests are not yet emitted by Collector. No current runtime
    // path can claim Complete from mere successful reads or empty event totals.
    Ok(Coverage {
        state,
        pending_observation_count: DecimalInt::from_nonnegative(pending.into())?,
        unattributed_observation_count: DecimalInt::from_nonnegative(unattributed.into())?,
        unattributed_total_tokens: if missing == 0 {
            known_total.map(|s| DecimalInt::parse(&s)).transpose()?
        } else {
            None
        },
        pending_file_count: DecimalInt::from_nonnegative(pending_files.into())?,
        source_issues,
        format_issues,
        breakdown_complete: [
            &totals.input_total,
            &totals.cached_input,
            &totals.noncached_input,
            &totals.output_total,
            &totals.reasoning_output,
        ]
        .iter()
        .all(|m| m.complete),
    })
}
impl Database {
    pub fn usage_coverage(&self, filter: &UsageFilter) -> StoreResult<Coverage> {
        self.snapshot(|tx, _| coverage(tx, filter, &super::totals(tx, filter)?))
    }
}
#[cfg(test)]
mod tests;
