//! Searchable candidates use keyset positions in real short-lived snapshots.
use super::{MODEL_KEY, dashboard, fact_from, predicate};
use crate::{
    Database, ErrorCode, Revision, StoreResult,
    leases::{LeaseHandle, cursor::QueryBinding},
};
use rusqlite::{Transaction, params_from_iter, types::Value};
use serde::{Deserialize, Serialize};
use token_pulse_core::{
    numeric::{DecimalInt, EpochMs},
    protocol::SnapshotMeta,
    query::{
        FacetDimension, FilterOption, FilterOptionsPage, FilterOptionsQuery, FilterOptionsRequest,
    },
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Position {
    key: Option<String>,
    generated_at_ms: EpochMs,
}
struct Rows {
    options: Vec<FilterOption>,
    more: bool,
    meta: SnapshotMeta,
}
fn rows(
    tx: &Transaction<'_>,
    revision: Revision,
    handle: LeaseHandle,
    query: &FilterOptionsQuery,
    last: Option<&Position>,
    at: EpochMs,
) -> StoreResult<Rows> {
    let filter = query.facet_filter()?;
    let p = predicate(&filter)?;
    let from = fact_from(&filter, matches!(query.dimension, FacetDimension::Models));
    let grouped = match query.dimension {
        FacetDimension::Models => format!(
            "SELECT {MODEL_KEY} AS dimension_key,MIN(CASE WHEN e.model IS NULL THEN '未知模型' ELSE e.model || CASE WHEN json_extract(o.normalized_json,'$.effective_metadata.provider') IS NULL THEN '' ELSE ' · ' || json_extract(o.normalized_json,'$.effective_metadata.provider') END END) AS label,COUNT(*) AS amount FROM {from} WHERE {} GROUP BY {MODEL_KEY}",
            p.sql
        ),
        FacetDimension::Projects => format!(
            "SELECT e.project_id AS dimension_key,MIN(COALESCE(project.user_alias,project.display_name,'未知项目')) AS label,COUNT(*) AS amount FROM {from} LEFT JOIN projects project ON project.project_id=e.project_id WHERE {} GROUP BY e.project_id",
            p.sql
        ),
        FacetDimension::Sessions => format!(
            "SELECT e.session_key AS dimension_key,MIN(COALESCE(session.provider_session_id,e.session_key)) AS label,COUNT(*) AS amount FROM {from} JOIN sessions session ON session.session_key=e.session_key WHERE {} GROUP BY e.session_key",
            p.sql
        ),
        FacetDimension::Sources => format!(
            "WITH counts AS (SELECT sf.source_id,COUNT(DISTINCT e.event_id) AS amount FROM {from} JOIN event_provenance ep ON ep.event_id=e.event_id JOIN observations po ON po.observation_id=ep.observation_id JOIN file_generations fg ON fg.file_generation_id=po.file_generation_id JOIN source_files sf ON sf.file_id=fg.file_id WHERE {} GROUP BY sf.source_id) SELECT source.source_id AS dimension_key,source.root_path || CASE WHEN source.enabled=0 THEN '（暂停）' ELSE '' END AS label,COALESCE(counts.amount,0) AS amount FROM sources source LEFT JOIN counts ON counts.source_id=source.source_id",
            p.sql
        ),
    };
    let mut values = p.values;
    values.push(Value::Text(query.search.clone()));
    let continuation = match last {
        None => String::new(),
        Some(Position { key: None, .. }) => " AND dimension_key IS NOT NULL".into(),
        Some(Position { key: Some(key), .. }) => {
            values.push(Value::Text(key.clone()));
            " AND dimension_key COLLATE BINARY > ?".into()
        }
    };
    values.push(Value::Integer(i64::from(query.page_size) + 1));
    // Stable NULL-first/opaque-key ordering. Labels and LIKE wildcards never
    // become cursor keys or SQL fragments; no OFFSET or mutable sort column.
    let sql = format!(
        "SELECT dimension_key,label,amount FROM ({grouped}) WHERE usage_search_contains(label,?){continuation} ORDER BY dimension_key COLLATE BINARY ASC LIMIT ?"
    );
    let mut statement = tx.prepare(&sql)?;
    let mut result = statement.query(params_from_iter(values))?;
    let mut options = Vec::new();
    while let Some(row) = result.next()? {
        let key: Option<String> = row.get(0)?;
        if key
            .as_ref()
            .is_some_and(|s| s.is_empty() || s.len() > 256 || s.chars().any(char::is_control))
        {
            return Err(ErrorCode::DbCorrupt.into());
        }
        options.push(FilterOption {
            key,
            display_name: row.get(1)?,
            count: DecimalInt::from_nonnegative(i128::from(row.get::<_, i64>(2)?))?,
        });
    }
    let more = options.len() > usize::from(query.page_size);
    options.truncate(usize::from(query.page_size));
    let (parser_versions, accounting_versions) = dashboard::versions(tx, &filter, &filter)?;
    let id = handle
        .snapshot_id
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    Ok(Rows {
        options,
        more,
        meta: SnapshotMeta {
            snapshot_id: format!("query-{id}"),
            data_revision: DecimalInt::from_nonnegative(revision.data.into())?,
            price_revision: DecimalInt::from_nonnegative(revision.price.into())?,
            generated_at_ms: last.map_or(at, |p| p.generated_at_ms),
            parser_versions,
            accounting_versions,
            display_timezone: filter.range.timezone,
        },
    })
}
impl Database {
    pub fn filter_options(
        &self,
        owner: &str,
        request: &FilterOptionsRequest,
        at: EpochMs,
    ) -> StoreResult<FilterOptionsPage> {
        request.validate()?;
        let binding = QueryBinding::new(owner, &("filter_options", &request.query))?;
        let (handle, last) = match &request.cursor {
            Some(cursor) => {
                let (handle, position) =
                    self.leases().resolve_cursor::<Position>(cursor, &binding)?;
                (handle, Some(position))
            }
            None => (self.leases().open(&binding)?, None),
        };
        let query = request.query.clone();
        let position = last.clone();
        let page = self.leases().read(&handle, &binding, move |tx, revision| {
            rows(tx, revision, handle, &query, position.as_ref(), at)
        });
        let page = match page {
            Ok(page) => page,
            Err(error) => {
                let _ = self.leases().release(&handle, &binding);
                return Err(error);
            }
        };
        let next_cursor = if page.more {
            let position = Position {
                key: page.options.last().ok_or(ErrorCode::DbCorrupt)?.key.clone(),
                generated_at_ms: page.meta.generated_at_ms,
            };
            match self.leases().issue_cursor(&handle, &binding, &position) {
                Ok(cursor) => Some(cursor),
                Err(error) => {
                    let _ = self.leases().release(&handle, &binding);
                    return Err(error);
                }
            }
        } else {
            // A terminal page is immutable; release immediately rather than
            // letting finished pickers occupy both slots for ten seconds.
            let _ = self.leases().release(&handle, &binding);
            None
        };
        Ok(FilterOptionsPage {
            meta: page.meta,
            dimension: request.query.dimension,
            options: page.options,
            next_cursor,
        })
    }
}

#[cfg(test)]
mod tests;
