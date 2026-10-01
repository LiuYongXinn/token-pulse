//! Relationships and classified observations are lifetime ledger evidence.
//! They never become extra consumption or change the selected event scope.
use super::*;
use rusqlite::{OptionalExtension, Row};
use token_pulse_core::{
    protocol::validate_request_id,
    query::{
        ClassificationKind, SessionActivity, SessionBundle, SessionBundleRequest,
        SessionClassification, SessionIdentity,
    },
};

fn canonical(tx: &Transaction<'_>, key: &str) -> StoreResult<String> {
    Ok(tx.query_row("SELECT COALESCE((SELECT canonical_session_key FROM session_aliases WHERE alias_session_key=?1),?1)", [key], |r| r.get(0))?)
}
fn identity(row: &Row<'_>) -> rusqlite::Result<SessionIdentity> {
    Ok(SessionIdentity {
        session_key: row.get(0)?,
        display_name: row.get(1)?,
        parent_key: row.get(2)?,
        parent_display_name: row.get(3)?,
        parent_provider_id: row.get(4)?,
    })
}
const IDENTITY: &str = "SELECT s.session_key,COALESCE(s.provider_session_id,s.session_key),parent.session_key,COALESCE(parent.provider_session_id,parent.session_key),s.parent_provider_id FROM sessions s LEFT JOIN sessions parent ON parent.session_key=COALESCE((SELECT canonical_session_key FROM session_aliases WHERE alias_session_key=s.parent_key),s.parent_key)";

pub(super) fn bundle(
    tx: &Transaction<'_>,
    revision: Revision,
    request: &SessionBundleRequest,
    at: EpochMs,
    snapshot_id: &str,
) -> StoreResult<SessionBundle> {
    let key = canonical(tx, &request.session_key)?;
    let identity = tx
        .query_row(
            &format!("{IDENTITY} WHERE s.session_key=?1"),
            [&key],
            identity,
        )
        .optional()?
        .ok_or(ErrorCode::InvalidQuery)?;
    let mut filter = request.filter.clone();
    // A detail target intersects the supplied session selection. It does not
    // silently broaden it, including when aliases occur in either selection.
    let allowed = match &filter.sessions {
        DimensionSelection::All {} => true,
        DimensionSelection::Ids { ids, .. } => {
            let mut allowed = false;
            for selected in ids {
                allowed |= canonical(tx, selected)? == key;
            }
            allowed
        }
    };
    filter.sessions = DimensionSelection::Ids {
        ids: if allowed { vec![key.clone()] } else { vec![] },
        include_unknown: false,
    };
    let mut page = rows(
        tx,
        revision,
        snapshot_id.into(),
        &SessionsQuery {
            filter,
            price_basis: request.price_basis.clone(),
            sort: SessionSort::LatestDesc,
            page_size: 1,
        },
        None,
        at,
    )?
    .data;
    if page.sessions.len() > 1 {
        return Err(ErrorCode::DbCorrupt.into());
    }
    let latest_selected_activity = page.sessions.pop().map(|row| SessionActivity {
        occurred_at_ms: row.latest_at_ms,
        model: row.latest_model,
        project_id: row.latest_project_id,
        project_display_name: row.latest_project_name,
    });
    let children_where = "COALESCE((SELECT canonical_session_key FROM session_aliases WHERE alias_session_key=s.parent_key),s.parent_key)=?1 AND NOT EXISTS(SELECT 1 FROM session_aliases WHERE alias_session_key=s.session_key)";
    let child_count: i64 = tx.query_row(
        &format!("SELECT COUNT(*) FROM sessions s WHERE {children_where}"),
        [&key],
        |r| r.get(0),
    )?;
    let mut statement = tx.prepare(&format!(
        "{IDENTITY} WHERE {children_where} ORDER BY s.session_key COLLATE BINARY LIMIT 100"
    ))?;
    let children = statement
        .query_map([&key], self::identity)?
        .collect::<Result<Vec<_>, _>>()?;
    let ledger: Option<(String,String,String)> = tx.query_row("SELECT l.ledger_id,l.parser_version,l.accounting_version FROM sessions s JOIN ledger_generations l ON l.ledger_id=s.active_ledger_id WHERE s.session_key=?1", [&key], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
    let mut classifications = Vec::new();
    if let Some((ledger, parser, accounting)) = ledger {
        page.meta.parser_versions.push(parser);
        page.meta.parser_versions.sort();
        page.meta.parser_versions.dedup();
        page.meta.accounting_versions.push(accounting);
        page.meta.accounting_versions.sort();
        page.meta.accounting_versions.dedup();
        let mut statement = tx.prepare("SELECT kind,reason_code,COUNT(DISTINCT observation_id) FROM pending_usage WHERE ledger_id=?1 GROUP BY kind,reason_code ORDER BY kind COLLATE BINARY,reason_code COLLATE BINARY LIMIT 65")?;
        let mut result = statement.query([ledger])?;
        while let Some(row) = result.next()? {
            let kind: String = row.get(0)?;
            let kind = match kind.as_str() {
                "pending" => ClassificationKind::Pending,
                "inherited" => ClassificationKind::Inherited,
                "duplicate" => ClassificationKind::Duplicate,
                "unattributed" => ClassificationKind::Unattributed,
                _ => return Err(ErrorCode::DbCorrupt.into()),
            };
            let reason_code: String = row.get(1)?;
            if reason_code.is_empty()
                || reason_code.len() > 128
                || !reason_code
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_')
            {
                return Err(ErrorCode::DbCorrupt.into());
            }
            classifications.push(SessionClassification {
                kind,
                reason_code,
                observation_count: DecimalInt::from_nonnegative(i128::from(row.get::<_, i64>(2)?))?,
            });
        }
        if classifications.len() > 64 {
            return Err(ErrorCode::DbCorrupt.into());
        }
    }
    Ok(SessionBundle {
        meta: page.meta,
        identity,
        summary: page.summary,
        pricing: page.pricing,
        coverage: page.coverage,
        latest_selected_activity,
        latest_context: context::latest_context(tx, &key)?,
        child_count: DecimalInt::from_nonnegative(child_count.into())?,
        children_truncated: child_count > children.len() as i64,
        children,
        classifications,
    })
}
impl Database {
    pub fn session_bundle(
        &self,
        request: &SessionBundleRequest,
        at: EpochMs,
        snapshot_id: &str,
    ) -> StoreResult<SessionBundle> {
        request.validate()?;
        validate_request_id(snapshot_id)?;
        self.snapshot(|tx, revision| bundle(tx, revision, request, at, snapshot_id))
    }
}
#[cfg(test)]
mod tests;
