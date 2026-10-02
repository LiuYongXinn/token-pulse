//! Replacement scope admits only this job's unpublished identities and freezes both relationships.
use super::*;

pub(super) fn select(files: &mut BTreeSet<String>, replacements: &[ReplacementInput]) {
    for input in replacements {
        files.remove(&input.base.generation_id);
        files.insert(input.generation_id.clone());
    }
}
pub(super) fn closure(
    tx: &Transaction<'_>,
    scope: &JobScope,
    replacements: &[ReplacementInput],
) -> StoreResult<BTreeSet<String>> {
    let mut selected = dependency_closure(tx, scope)?;
    let headers = replacements
        .iter()
        .flat_map(|r| &r.sessions)
        .collect::<Vec<_>>();
    selected.extend(headers.iter().map(|h| h.session_key.clone()));
    // Published identities keep their old edges; proposed headers add the new edges.
    // NULL rows from other/failed candidates do not participate, including their stale headers.
    let encoded = serde_json::to_string(&headers)?;
    let mut frontier = selected.iter().cloned().collect::<Vec<_>>();
    let mut q = tx.prepare("WITH identity AS (SELECT session_key,provider,provider_session_id,parent_key,parent_provider_id FROM sessions WHERE active_ledger_id IS NOT NULL UNION ALL SELECT json_extract(value,'$.session_key'),'codex',json_extract(value,'$.provider_session_id'),NULL,json_extract(value,'$.parent_provider_id') FROM json_each(?2)), edges AS (SELECT DISTINCT other.session_key FROM identity seed JOIN identity other ON other.provider=seed.provider AND (other.session_key=seed.parent_key OR other.parent_key=seed.session_key OR (seed.provider_session_id IS NOT NULL AND (other.provider_session_id=seed.provider_session_id OR other.parent_provider_id=seed.provider_session_id)) OR (seed.parent_provider_id IS NOT NULL AND other.provider_session_id=seed.parent_provider_id)) WHERE seed.session_key=?1 UNION SELECT canonical_session_key FROM session_aliases WHERE alias_session_key=?1 UNION SELECT alias_session_key FROM session_aliases WHERE canonical_session_key=?1) SELECT session_key FROM edges")?;
    while let Some(key) = frontier.pop() {
        if selected.len() > 32768 {
            return Err(ErrorCode::InvalidQuery.into());
        }
        for row in q.query_map(params![key, encoded], |r| r.get::<_, String>(0))? {
            let other = row?;
            if selected.insert(other.clone()) {
                frontier.push(other);
            }
        }
    }
    Ok(selected)
}
pub(super) fn publish_files(tx: &Transaction<'_>, m: &RebuildManifest, at: i64) -> StoreResult<()> {
    for replacement in &m.replacements {
        let input = m
            .files
            .iter()
            .find(|f| f.generation_id == replacement.generation_id)
            .ok_or(ErrorCode::CandidateObsolete)?;
        let identity: Option<String> = json(&input.identity_json)?;
        tx.execute("UPDATE file_generations SET state='retired' WHERE file_generation_id=?1 AND state='current'",[&replacement.base.generation_id])?;
        tx.execute("UPDATE file_generations SET state='current' WHERE file_generation_id=?1 AND state='candidate'",[&input.generation_id])?;
        tx.execute("UPDATE source_files SET current_generation_id=?1,file_identity=?2,status='present',last_seen_at_ms=?3 WHERE file_id=?4",params![input.generation_id,identity,at,input.file_id])?;
        tx.execute("UPDATE file_read_candidates SET state='published',error_code=NULL,updated_at_ms=?1 WHERE generation_id=?2 AND state='claimed'",params![at,input.generation_id])?;
        // A structural EOF is not a fresh directory/read confirmation.
        tx.execute("UPDATE source_scan_files SET file_generation_id=NULL,checkpoint_revision=NULL,checked_at_ms=NULL WHERE source_id=?1 AND file_id=?2",params![replacement.base.source_id,input.file_id])?;
        tx.execute("UPDATE source_scan_state SET state=CASE WHEN discovery_complete=0 THEN 'scanning' ELSE 'incomplete' END,completed_at_ms=NULL WHERE source_id=?1",[&replacement.base.source_id])?;
        // Retired diagnostics describe superseded bytes. New necessary diagnostics publish here.
        tx.execute("UPDATE diagnostics SET resolved_at_ms=?1 WHERE file_generation_id=?2 AND resolved_at_ms IS NULL",params![at,replacement.base.generation_id])?;
        tx.execute("INSERT INTO diagnostics(diagnostic_id,source_id,file_generation_id,byte_offset,session_key,code,severity,metadata_json,dedup_key,first_seen_at_ms,last_seen_at_ms) SELECT diagnostic_id,json_extract(diagnostic_json,'$.source_id'),generation_id,byte_offset,json_extract(diagnostic_json,'$.session_key'),json_extract(diagnostic_json,'$.code'),json_extract(diagnostic_json,'$.severity'),json_extract(diagnostic_json,'$.metadata'),json_extract(diagnostic_json,'$.dedup_key'),json_extract(diagnostic_json,'$.observed_at_ms'),json_extract(diagnostic_json,'$.observed_at_ms') FROM file_candidate_diagnostics WHERE generation_id=?1 ON CONFLICT(dedup_key) DO UPDATE SET occurrences=occurrences+1,last_seen_at_ms=excluded.last_seen_at_ms,resolved_at_ms=NULL",[&input.generation_id])?;
    }
    Ok(())
}
pub(super) fn publish_identities(
    tx: &Transaction<'_>,
    m: &RebuildManifest,
    plan: &CanonicalReplayPlan,
) -> StoreResult<()> {
    for group in &plan.plan.groups {
        // Header agreement and canonical origin selection determine the published identity.
        // A replacement copy that conflicts with a trusted mirror must not override its header.
        if !group.member_sequence_keys.iter().any(|k| {
            m.replacements
                .iter()
                .any(|r| r.generation_id == plan.sequences[k].file_generation_id)
        }) {
            continue;
        }
        let identity = &plan.sequences[&group.primary_sequence_key]
            .physical
            .identity;
        tx.execute("UPDATE sessions SET parent_provider_id=?1,parent_key=NULL,created_at_ms=?2 WHERE session_key=?3",params![identity.parent_provider_id,identity.created_at_ms,group.session_key])?;
    }
    Ok(())
}
pub(super) fn resolve_parents(tx: &Transaction<'_>, m: &RebuildManifest) -> StoreResult<()> {
    if m.replacements.is_empty() {
        return Ok(());
    }
    for ledger in &m.ledgers {
        tx.execute("UPDATE sessions SET parent_key=(SELECT CASE WHEN COUNT(DISTINCT COALESCE(a.canonical_session_key,p.session_key))=1 THEN MIN(COALESCE(a.canonical_session_key,p.session_key)) ELSE NULL END FROM sessions p LEFT JOIN session_aliases a ON a.alias_session_key=p.session_key WHERE p.active_ledger_id IS NOT NULL AND p.provider=sessions.provider AND p.provider_session_id=sessions.parent_provider_id AND p.session_key<>sessions.session_key) WHERE session_key=?1",[&ledger.session_key])?;
    }
    Ok(())
}
#[cfg(test)]
mod tests;
