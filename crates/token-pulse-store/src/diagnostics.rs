//! Read current saved problems in one real snapshot; never open source files.
use crate::{Database, ErrorCode, StoreResult};
use rusqlite::{OptionalExtension, Transaction};
use sha2::{Digest, Sha256};
use token_pulse_core::{diagnostics::*, numeric::DecimalInt};

const ISSUES: &str = "
WITH issues(origin,identity,source,kind,code,path,offset,sort_time) AS (
 SELECT 'diagnostic',d.diagnostic_id,COALESCE(d.source_id,f.source_id),'log_record',d.code,f.canonical_path,d.byte_offset,d.last_seen_at_ms
 FROM diagnostics d LEFT JOIN file_generations g ON g.file_generation_id=d.file_generation_id LEFT JOIN source_files f ON f.file_id=g.file_id
 WHERE d.resolved_at_ms IS NULL AND (d.file_generation_id IS NULL OR (g.state='current' AND f.current_generation_id=g.file_generation_id AND (d.source_id IS NULL OR d.source_id=f.source_id)))
 UNION ALL
 SELECT 'usage',p.pending_id,f.source_id,CASE p.kind WHEN 'pending' THEN 'unconfirmed_usage' ELSE 'unattributed_usage' END,NULL,f.canonical_path,o.byte_offset,o.observed_at_ms
 FROM pending_usage p JOIN sessions s ON s.active_ledger_id=p.ledger_id JOIN observations o ON o.observation_id=p.observation_id JOIN file_generations g ON g.file_generation_id=o.file_generation_id JOIN source_files f ON f.file_id=g.file_id
 WHERE p.kind IN ('pending','unattributed') AND g.state='current' AND f.current_generation_id=g.file_generation_id
 UNION ALL
 SELECT 'file',f.file_id,f.source_id,'missing_file',NULL,f.canonical_path,NULL,NULL FROM source_files f JOIN file_generations g ON g.file_generation_id=f.current_generation_id WHERE f.status='missing' AND g.state='current'
 UNION ALL
 SELECT 'scan',ss.source_id,ss.source_id,'directory_scan',ss.issue_code,s.root_path,NULL,NULL FROM source_scan_state ss JOIN sources s ON s.source_id=ss.source_id WHERE ss.issue_code IS NOT NULL AND s.enabled=1 AND s.root_path=ss.source_root
), ranked AS (
 SELECT *,ROW_NUMBER() OVER (PARTITION BY source,kind,code,path ORDER BY sort_time DESC,identity COLLATE BINARY) AS location_rank FROM issues WHERE (?1 IS NULL OR source=?1)
)
SELECT origin,identity,source,kind,code,path,offset FROM ranked WHERE location_rank=1 ORDER BY sort_time DESC,origin COLLATE BINARY,identity COLLATE BINARY LIMIT 21";

pub(crate) fn read(
    tx: &Transaction<'_>,
    revision: i64,
    request: &DiagnosticsRequest,
) -> StoreResult<DiagnosticsSnapshot> {
    request.validate()?;
    if let Some(id) = &request.source_id {
        if tx
            .query_row(
                "SELECT source_id FROM sources WHERE source_id=?1",
                [id],
                |r| r.get::<_, String>(0),
            )
            .optional()?
            .is_none()
        {
            return Err(ErrorCode::InvalidQuery.into());
        }
    }
    let mut statement = tx.prepare(ISSUES)?;
    let mut rows = statement.query([&request.source_id])?;
    let mut issues = Vec::new();
    while let Some(row) = rows.next()? {
        let origin: String = row.get(0)?;
        let identity: String = row.get(1)?;
        let kind: String = row.get(3)?;
        let code: Option<String> = row.get(4)?;
        let offset: Option<i64> = row.get(6)?;
        issues.push(DiagnosticIssue {
            issue_id: format!(
                "{:x}",
                Sha256::digest(format!("{origin}:{identity}").as_bytes())
            ),
            source_id: row.get(2)?,
            kind: serde_json::from_value(serde_json::Value::String(kind))
                .map_err(|_| ErrorCode::DbCorrupt)?,
            code: code
                .map(|c| {
                    serde_json::from_value(serde_json::Value::String(c))
                        .map_err(|_| ErrorCode::DbCorrupt)
                })
                .transpose()?,
            path: row.get(5)?,
            byte_offset: offset
                .map(|o| DecimalInt::from_nonnegative(i128::from(o)))
                .transpose()?,
        });
    }
    let has_more = issues.len() > 20;
    issues.truncate(20);
    Ok(DiagnosticsSnapshot {
        data_revision: DecimalInt::from_nonnegative(i128::from(revision))?,
        issues,
        has_more,
    })
}
impl Database {
    pub fn diagnostics(&self, request: &DiagnosticsRequest) -> StoreResult<DiagnosticsSnapshot> {
        self.snapshot(|tx, revision| read(tx, revision.data, request))
    }
}
#[cfg(test)]
mod tests;
