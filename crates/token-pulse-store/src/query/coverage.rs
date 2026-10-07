//! Coverage gaps are independent of confirmed dated consumption.
use super::Predicate;
use crate::source_scan::BAD_ENTRY;
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

// A missing scan confirmation alone is verification work, not an unread file.
// An enumerated append still counts before collection updates observed_size.
const UNREAD_ENTRY: &str = "f.file_id IS NULL OR readg.file_generation_id IS NULL OR readg.state<>'current' OR f.source_id IS NOT e.source_id OR f.status NOT IN ('present','known') OR readg.committed_offset<readg.observed_size OR (e.file_generation_id IS NULL AND readg.committed_offset<e.upper_bound)";

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

// A fixed date-first plan is bad for a single session: it would scan the
// whole month for every row in a session page. Narrow sessions lead from ledgers.
fn pending_from(filter: &UsageFilter) -> &'static str {
    if matches!(&filter.sessions, DimensionSelection::Ids { .. }) {
        "sessions s INDEXED BY session_active_ledger_read CROSS JOIN pending_usage p INDEXED BY pending_ledger_lookup ON p.ledger_id=s.active_ledger_id CROSS JOIN observations o ON o.observation_id=p.observation_id"
    } else {
        "observations o INDEXED BY observation_usage_time CROSS JOIN pending_usage p INDEXED BY pending_observation_lookup ON p.observation_id=o.observation_id CROSS JOIN sessions s INDEXED BY session_active_ledger_read ON s.active_ledger_id=p.ledger_id"
    }
}
type PendingCounts = (i64, i64, Option<String>, i64);
fn pending_counts(tx: &Transaction<'_>, filter: &UsageFilter) -> StoreResult<PendingCounts> {
    let started = std::time::Instant::now();
    let p = pending_predicate(filter)?;
    let sql = format!(
        "WITH selected AS MATERIALIZED (SELECT p.observation_id,p.kind,p.vector_json FROM {} JOIN file_generations fg ON fg.file_generation_id=o.file_generation_id JOIN source_files sf ON sf.file_id=fg.file_id WHERE {} AND p.kind IN ('pending','unattributed')) SELECT COUNT(DISTINCT CASE WHEN kind='pending' THEN observation_id END),COUNT(DISTINCT CASE WHEN kind='unattributed' THEN observation_id END),(SELECT sum_token_decimal(usage_vector_total(vector_json)) FROM selected WHERE kind='unattributed'),COUNT(CASE WHEN kind='unattributed' AND usage_vector_total(vector_json) IS NULL THEN 1 END) FROM selected",
        pending_from(filter),
        p.sql
    );
    let counts: PendingCounts = tx.query_row(&sql, params_from_iter(p.values), |r| {
        Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
    })?;
    crate::query_timing::record("coverage_pending", started);
    Ok(counts)
}
/// Reuse only source-wide health in the SAME transaction and source selection.
/// Pending/unattributed counts remain specific to the narrowed dimensions/range.
pub(super) fn narrowed_coverage(
    tx: &Transaction<'_>,
    filter: &UsageFilter,
    totals: &TokenTotals,
    common: &Coverage,
) -> StoreResult<Coverage> {
    narrowed_from_counts(totals, common, pending_counts(tx, filter)?)
}
fn narrowed_from_counts(
    totals: &TokenTotals,
    common: &Coverage,
    (pending, unattributed, known_total, missing): PendingCounts,
) -> StoreResult<Coverage> {
    let mut result = common.clone();
    result.pending_observation_count = DecimalInt::from_nonnegative(pending.into())?;
    result.unattributed_observation_count = DecimalInt::from_nonnegative(unattributed.into())?;
    result.unattributed_total_tokens = if missing == 0 {
        known_total.map(|n| DecimalInt::parse(&n)).transpose()?
    } else {
        None
    };
    result.state = if common_gap(common) || pending > 0 || unattributed > 0 {
        CoverageState::Partial
    } else if common.source_issues.is_empty() && !matches!(common.state, CoverageState::Unknown) {
        CoverageState::Complete
    } else {
        CoverageState::Unknown
    };
    result.breakdown_complete = [
        &totals.input_total,
        &totals.cached_input,
        &totals.noncached_input,
        &totals.output_total,
        &totals.reasoning_output,
        &totals.cache_write_input,
    ]
    .iter()
    .all(|m| m.complete);
    Ok(result)
}
/// Scan the selected range once for bounded model/project rows, in the SAME snapshot.
pub(super) fn grouped_coverage(
    tx: &Transaction<'_>,
    filter: &UsageFilter,
    dimension: token_pulse_core::query::GroupDimension,
    groups: &[token_pulse_core::query::GroupedUsage],
    common: &Coverage,
) -> StoreResult<Vec<Coverage>> {
    if groups.is_empty() {
        return Ok(vec![]);
    }
    let started = std::time::Instant::now();
    let p = pending_predicate(filter)?;
    let key = match dimension {
        token_pulse_core::query::GroupDimension::Models => {
            "usage_model_key(json_extract(o.normalized_json,'$.effective_metadata.provider'),o.model)"
        }
        token_pulse_core::query::GroupDimension::Projects => "o.project_id",
    };
    let mut selected = Predicate {
        sql: "1".into(),
        values: vec![],
    };
    selected.selection(
        "group_key",
        &DimensionSelection::Ids {
            ids: groups.iter().filter_map(|g| g.key.clone()).collect(),
            include_unknown: groups.iter().any(|g| g.key.is_none()),
        },
    );
    let sql = format!(
        "WITH selected AS MATERIALIZED (SELECT p.observation_id,p.kind,p.vector_json,{key} AS group_key FROM {} JOIN file_generations fg ON fg.file_generation_id=o.file_generation_id JOIN source_files sf ON sf.file_id=fg.file_id WHERE {} AND p.kind IN ('pending','unattributed')) SELECT group_key,COUNT(DISTINCT CASE WHEN kind='pending' THEN observation_id END),COUNT(DISTINCT CASE WHEN kind='unattributed' THEN observation_id END),sum_token_decimal(CASE WHEN kind='unattributed' THEN usage_vector_total(vector_json) END),COUNT(CASE WHEN kind='unattributed' AND usage_vector_total(vector_json) IS NULL THEN 1 END) FROM selected WHERE {} GROUP BY group_key",
        pending_from(filter),
        p.sql,
        selected.sql
    );
    let mut statement = tx.prepare(&sql)?;
    let mut rows = statement.query(params_from_iter(
        p.values.into_iter().chain(selected.values),
    ))?;
    let mut counts = std::collections::BTreeMap::<Option<String>, PendingCounts>::new();
    while let Some(row) = rows.next()? {
        counts.insert(
            row.get(0)?,
            (row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?),
        );
    }
    // No matching pending rows is an authoritative empty count; its amount stays NULL.
    let result = groups
        .iter()
        .map(|group| {
            narrowed_from_counts(
                &group.totals,
                common,
                counts.remove(&group.key).unwrap_or((0, 0, None, 0)),
            )
        })
        .collect();
    crate::query_timing::record("coverage_grouped_pending", started);
    result
}
fn common_gap(base: &Coverage) -> bool {
    base.pending_file_count.value() > 0
        || !base.format_issues.is_empty()
        || base.source_issues.iter().any(|s| {
            matches!(
                s.code.as_str(),
                "source_paused"
                    | "source_unreadable"
                    | "source_partially_readable"
                    | "source_scan_incomplete"
                    | "source_scan_pending"
            )
        })
}
pub fn coverage(
    tx: &Transaction<'_>,
    filter: &UsageFilter,
    totals: &TokenTotals,
) -> StoreResult<Coverage> {
    let (pending, unattributed, known_total, missing) = pending_counts(tx, filter)?;
    let source = source_selection(filter, "sf.source_id");
    let files_started = std::time::Instant::now();
    let scan_source = source_selection(filter, "e.source_id");
    let mut values = source.values;
    values.extend(scan_source.values);
    // Discovered-but-unregistered files count too. The union counts a known file only once.
    let pending_files:i64=tx.query_row(&format!("SELECT COUNT(*) FROM (SELECT sf.source_id,'file:'||sf.file_id AS identity FROM source_files sf LEFT JOIN file_generations fg ON fg.file_generation_id=sf.current_generation_id WHERE {} AND (fg.file_generation_id IS NULL OR fg.state<>'current' OR fg.committed_offset<fg.observed_size OR sf.status NOT IN ('present','known')) UNION SELECT e.source_id,CASE WHEN e.file_id IS NULL THEN 'path:'||e.canonical_path ELSE 'file:'||e.file_id END FROM source_scan_files e JOIN source_scan_state ss USING(source_id,scan_revision) JOIN sources s ON s.source_id=e.source_id AND s.root_path=ss.source_root LEFT JOIN source_files f ON f.file_id=e.file_id LEFT JOIN file_generations readg ON readg.file_generation_id=f.current_generation_id WHERE {} AND ({UNREAD_ENTRY}))",source.sql,scan_source.sql),params_from_iter(values),|r|r.get(0))?;
    let scan_source = source_selection(filter, "e.source_id");
    let verifying_files:i64=tx.query_row(&format!("SELECT COUNT(*) FROM (SELECT DISTINCT e.source_id,CASE WHEN e.file_id IS NULL THEN 'path:'||e.canonical_path ELSE 'file:'||e.file_id END FROM source_scan_files e JOIN source_scan_state ss USING(source_id,scan_revision) JOIN sources s ON s.source_id=e.source_id AND s.root_path=ss.source_root LEFT JOIN source_files f ON f.file_id=e.file_id LEFT JOIN file_generations g ON g.file_generation_id=e.file_generation_id LEFT JOIN file_generations readg ON readg.file_generation_id=f.current_generation_id WHERE {} AND ({BAD_ENTRY}) AND NOT ({UNREAD_ENTRY}))",scan_source.sql),params_from_iter(scan_source.values),|r|r.get(0))?;

    // A file with unknown time/identity can belong to the selected date/session.
    // Whole-source health gaps must not disappear behind a model/date filter.
    let source = source_selection(filter, "s.source_id");
    crate::query_timing::record("coverage_files", files_started);
    let sources_started = std::time::Instant::now();
    let mut statement=tx.prepare(&format!("SELECT s.source_id,s.enabled,s.readability,s.last_success_at_ms,ss.state,ss.source_root=s.root_path,ss.discovery_complete,ss.invalidated,ss.issue_code,EXISTS(SELECT 1 FROM source_scan_files e LEFT JOIN source_files f ON f.file_id=e.file_id LEFT JOIN file_generations g ON g.file_generation_id=e.file_generation_id WHERE e.source_id=ss.source_id AND e.scan_revision=ss.scan_revision AND ({BAD_ENTRY})),EXISTS(SELECT 1 FROM source_scan_files e LEFT JOIN source_files f ON f.file_id=e.file_id LEFT JOIN file_generations readg ON readg.file_generation_id=f.current_generation_id WHERE e.source_id=ss.source_id AND e.scan_revision=ss.scan_revision AND ({UNREAD_ENTRY})) FROM sources s LEFT JOIN source_scan_state ss USING(source_id) WHERE {} ORDER BY s.source_id COLLATE BINARY",source.sql))?;
    let mut rows = statement.query(params_from_iter(source.values))?;
    let mut source_issues = Vec::new();
    let mut known_source_gap = false;
    let mut selected_sources = 0;
    while let Some(row) = rows.next()? {
        selected_sources += 1;
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
                SourceReadability::Readable => {
                    let state: Option<String> = row.get(4)?;
                    let same_root: Option<bool> = row.get(5)?;
                    let complete: Option<bool> = row.get(6)?;
                    let invalidated: Option<bool> = row.get(7)?;
                    let issue: Option<String> = row.get(8)?;
                    let pending_entries: bool = row.get(9)?;
                    let unread_entries: bool = row.get(10)?;
                    if same_root != Some(true) || state.is_none() {
                        "scan_evidence_missing"
                    } else if issue.is_some() {
                        known_source_gap = true;
                        "source_scan_incomplete"
                    } else if state.as_deref() == Some("interrupted") {
                        "source_scan_interrupted"
                    } else if invalidated == Some(true) {
                        "source_scan_changed"
                    } else if complete != Some(true) {
                        "source_scanning"
                    } else if pending_entries {
                        if unread_entries {
                            known_source_gap = true;
                            "source_scan_pending"
                        } else {
                            "source_scan_verifying"
                        }
                    } else if state.as_deref() == Some("ready") {
                        continue;
                    } else {
                        "source_scan_incomplete"
                    }
                }
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
    crate::query_timing::record("coverage_sources", sources_started);
    let formats_started = std::time::Instant::now();
    let source = source_selection(filter, "COALESCE(d.source_id,df.source_id)");
    let mut statement=tx.prepare(&format!("SELECT COALESCE(json_extract(d.metadata_json,'$.parser_version'),'unknown') AS format,sum_token_decimal(d.occurrences) FROM diagnostics d LEFT JOIN file_generations dg ON dg.file_generation_id=d.file_generation_id LEFT JOIN source_files df ON df.file_id=dg.file_id WHERE {} AND d.code='UNSUPPORTED_FORMAT' AND d.resolved_at_ms IS NULL AND (d.file_generation_id IS NULL OR (dg.state='current' AND df.current_generation_id=dg.file_generation_id AND (d.source_id IS NULL OR d.source_id=df.source_id))) GROUP BY format ORDER BY format COLLATE BINARY",source.sql))?;
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
    } else if selected_sources > 0 && source_issues.is_empty() {
        CoverageState::Complete
    } else {
        CoverageState::Unknown
    };
    crate::query_timing::record("coverage_formats", formats_started);
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
        verifying_file_count: Some(DecimalInt::from_nonnegative(verifying_files.into())?),
        source_issues,
        format_issues,
        breakdown_complete: [
            &totals.input_total,
            &totals.cached_input,
            &totals.noncached_input,
            &totals.output_total,
            &totals.reasoning_output,
            &totals.cache_write_input,
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

#[derive(Default, Clone, Copy)]
struct BucketGaps {
    pending: i128,
    unattributed: i128,
    known_tokens: i128,
    missing: bool,
}
impl BucketGaps {
    fn add(&mut self, kind: &str, total: Option<i64>) -> StoreResult<()> {
        let count = if kind == "pending" {
            &mut self.pending
        } else {
            &mut self.unattributed
        };
        *count = count.checked_add(1).ok_or(ErrorCode::NumericOverflow)?;
        if kind == "unattributed" {
            match total {
                Some(value) => {
                    self.known_tokens = self
                        .known_tokens
                        .checked_add(i128::from(value))
                        .ok_or(ErrorCode::NumericOverflow)?
                }
                None => self.missing = true,
            }
        }
        Ok(())
    }
    fn merged(self, unknown_time: Self) -> StoreResult<Self> {
        let sum = |a: i128, b: i128| a.checked_add(b).ok_or(ErrorCode::NumericOverflow);
        Ok(Self {
            pending: sum(self.pending, unknown_time.pending)?,
            unattributed: sum(self.unattributed, unknown_time.unattributed)?,
            known_tokens: sum(self.known_tokens, unknown_time.known_tokens)?,
            missing: self.missing || unknown_time.missing,
        })
    }
}

/// One pending scan for every calendar bucket. Unknown-time gaps apply to each
/// bucket; known-time gaps apply only to their real UTC interval.
pub fn series_coverage(
    tx: &Transaction<'_>,
    filter: &UsageFilter,
    buckets: &[super::BucketTotals],
) -> StoreResult<Vec<Coverage>> {
    let base = coverage(tx, filter, &super::empty_totals())?;
    series_coverage_from(tx, filter, buckets, &base)
}
pub(super) fn series_coverage_from(
    tx: &Transaction<'_>,
    filter: &UsageFilter,
    buckets: &[super::BucketTotals],
    common: &Coverage,
) -> StoreResult<Vec<Coverage>> {
    if buckets.is_empty()
        || buckets.len() > token_pulse_core::calendar::MAX_BUCKETS
        || buckets[0].bucket.start_ms != filter.range.start_ms
        || buckets.last().unwrap().bucket.end_ms != filter.range.end_ms
        || buckets.iter().any(|b| b.bucket.start_ms >= b.bucket.end_ms)
        || buckets
            .windows(2)
            .any(|pair| pair[0].bucket.end_ms != pair[1].bucket.start_ms)
    {
        return Err(ErrorCode::InvalidQuery.into());
    }
    let base = common;
    let common_gap = common_gap(base);
    let p = pending_predicate(filter)?;
    let mut statement=tx.prepare(&format!("SELECT DISTINCT p.observation_id,p.kind,o.observed_at_ms,CASE WHEN p.kind='unattributed' THEN usage_vector_total(p.vector_json) END FROM {} JOIN file_generations fg ON fg.file_generation_id=o.file_generation_id JOIN source_files sf ON sf.file_id=fg.file_id WHERE {} AND p.kind IN ('pending','unattributed')",pending_from(filter),p.sql))?;
    let mut rows = statement.query(params_from_iter(p.values))?;
    let mut gaps = vec![BucketGaps::default(); buckets.len()];
    let mut unknown_time = BucketGaps::default();
    while let Some(row) = rows.next()? {
        let time: Option<i64> = row.get(2)?;
        let kind: String = row.get(1)?;
        let target = match time {
            None => &mut unknown_time,
            Some(time) => {
                let index = buckets.partition_point(|b| b.bucket.end_ms.value() <= time);
                gaps.get_mut(index).ok_or(ErrorCode::DbCorrupt)?
            }
        };
        target.add(&kind, row.get(3)?)?;
    }
    buckets
        .iter()
        .zip(gaps)
        .map(|(bucket, gap)| {
            let gap = gap.merged(unknown_time)?;
            let mut result = base.clone();
            result.pending_observation_count = DecimalInt::from_nonnegative(gap.pending)?;
            result.unattributed_observation_count = DecimalInt::from_nonnegative(gap.unattributed)?;
            result.unattributed_total_tokens = if gap.unattributed > 0 && !gap.missing {
                Some(DecimalInt::from_nonnegative(gap.known_tokens)?)
            } else {
                None
            };
            result.state = if common_gap || gap.pending > 0 || gap.unattributed > 0 {
                CoverageState::Partial
            } else if base.source_issues.is_empty() && !matches!(base.state, CoverageState::Unknown)
            {
                CoverageState::Complete
            } else {
                CoverageState::Unknown
            };
            result.breakdown_complete = [
                &bucket.totals.input_total,
                &bucket.totals.cached_input,
                &bucket.totals.noncached_input,
                &bucket.totals.output_total,
                &bucket.totals.reasoning_output,
                &bucket.totals.cache_write_input,
            ]
            .iter()
            .all(|m| m.complete);
            Ok(result)
        })
        .collect()
}
#[cfg(test)]
mod scan_tests;
#[cfg(test)]
mod series_tests;
#[cfg(test)]
mod tests;
