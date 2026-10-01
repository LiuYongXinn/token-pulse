//! Proofs are computed from frozen stored observations, never from caller supplied alias pairs.
use super::*;
use token_pulse_core::{
    canonical::{self, CanonicalPlan, PhysicalSequence},
    domain::NormalizedObservation,
    sequence::{SequenceIdentity, UsageSignature},
};

pub struct PhysicalReplaySequence {
    pub physical: PhysicalSequence,
    pub file_generation_id: String,
    pub observation_ids: Vec<String>,
}
pub struct CanonicalReplayPlan {
    pub plan: CanonicalPlan,
    pub sequences: BTreeMap<String, PhysicalReplaySequence>,
}
fn root(tx: &Transaction<'_>, key: &str) -> StoreResult<String> {
    let canonical: Option<String> = tx
        .query_row(
            "SELECT canonical_session_key FROM session_aliases WHERE alias_session_key=?1",
            [key],
            |r| r.get(0),
        )
        .optional()?;
    let result = canonical.unwrap_or_else(|| key.into());
    if tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM session_aliases WHERE alias_session_key=?1)",
        [&result],
        |r| r.get::<_, bool>(0),
    )? {
        return Err(ErrorCode::DbCorrupt.into());
    }
    Ok(result)
}
pub(super) fn alias_target(
    tx: &Transaction<'_>,
    job: &str,
    session: &str,
) -> StoreResult<Option<String>> {
    Ok(tx.query_row("SELECT canonical_session_key FROM candidate_session_aliases WHERE job_id=?1 AND alias_session_key=?2 UNION ALL SELECT canonical_session_key FROM session_aliases WHERE alias_session_key=?2 LIMIT 1", params![job,session], |r|r.get(0)).optional()?)
}
pub(super) fn publish_aliases(tx: &Transaction<'_>, m: &RebuildManifest) -> StoreResult<()> {
    let mut query=tx.prepare("SELECT alias_session_key,canonical_session_key FROM candidate_session_aliases WHERE job_id=?1 ORDER BY alias_session_key")?;
    let aliases = query
        .query_map([&m.job_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(query);
    let mut changed_roots = BTreeSet::new();
    for (alias, canonical) in aliases {
        if !m.ledgers.iter().any(|l| l.session_key == alias)
            || !m.ledgers.iter().any(|l| l.session_key == canonical)
            || alias_target(tx, &m.job_id, &canonical)?.is_some()
        {
            return Err(ErrorCode::InvalidUsage.into());
        }
        // Move only derived logical keys. Physical positions and original usage fingerprints remain.
        let mut after = 0i64;
        loop {
            let mut q=tx.prepare("SELECT rowid,observation_id,normalized_json FROM observations WHERE session_key=?1 AND rowid>?2 ORDER BY rowid LIMIT 256")?;
            let rows = q
                .query_map(params![alias, after], |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            drop(q);
            if rows.is_empty() {
                break;
            }
            for (rowid, id, encoded) in rows {
                let mut record: NormalizedObservation = json(&encoded)?;
                match &mut record {
                    NormalizedObservation::Usage(u) => u.session_key = canonical.clone(),
                    NormalizedObservation::TurnMetadata { session_key, .. }
                    | NormalizedObservation::Context { session_key, .. } => {
                        *session_key = canonical.clone()
                    }
                    NormalizedObservation::SessionMetadata { .. } => {}
                }
                tx.execute("UPDATE observations SET session_key=?1,normalized_json=?2 WHERE observation_id=?3",params![canonical,serde_json::to_string(&record)?,id])?;
                after = rowid;
            }
        }
        tx.execute("INSERT INTO file_session_bindings(file_generation_id,session_key,first_offset,identity_evidence) SELECT file_generation_id,?1,first_offset,'verified_mirror' FROM file_session_bindings WHERE session_key=?2 ON CONFLICT(file_generation_id,session_key) DO NOTHING",params![canonical,alias])?;
        tx.execute("UPDATE session_aliases SET canonical_session_key=?1,evidence_job_id=?2 WHERE canonical_session_key=?3",params![canonical,m.job_id,alias])?;
        tx.execute("INSERT INTO session_aliases VALUES(?1,?2,?3) ON CONFLICT(alias_session_key) DO UPDATE SET canonical_session_key=excluded.canonical_session_key,evidence_job_id=excluded.evidence_job_id",params![alias,canonical,m.job_id])?;
        tx.execute(
            "UPDATE sessions SET identity_status='verified_mirror' WHERE session_key=?1",
            [&alias],
        )?;
        tx.execute(
            "UPDATE sessions SET parent_key=?1 WHERE parent_key=?2",
            params![canonical, alias],
        )?;
        changed_roots.insert(canonical);
    }
    for canonical in changed_roots {
        // The collector must consult the aligned ordinal cursor before using the shared baseline.
        // Keeping this conservative flag also protects callers that do not load that evidence.
        let overflow:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM file_generations WHERE state='current' AND checkpoint_revision=9223372036854775807 AND file_generation_id IN (SELECT file_generation_id FROM file_session_bindings WHERE session_key=?1))",[&canonical],|r|r.get(0))?;
        if overflow {
            return Err(ErrorCode::NumericOverflow.into());
        }
        tx.execute("UPDATE file_generations SET checkpoint_revision=checkpoint_revision+1,reader_context_json=json_set(reader_context_json,'$.session_key',?1,'$.requires_sequence_rebuild',json('true'),'$.independent_head_available',json('false')) WHERE state='current' AND file_generation_id IN (SELECT file_generation_id FROM file_session_bindings WHERE session_key=?1)",[&canonical])?;
    }
    Ok(())
}
fn load(tx: &Transaction<'_>, m: &RebuildManifest) -> StoreResult<CanonicalReplayPlan> {
    let mut sequences: BTreeMap<String, PhysicalReplaySequence> = BTreeMap::new();
    let mut footprint = 0usize;
    for ledger in &m.ledgers {
        let owner = root(tx, &ledger.session_key)?;
        if !m.ledgers.iter().any(|l| l.session_key == owner) {
            return Err(ErrorCode::CandidateObsolete.into());
        }
        let mut query = tx.prepare("SELECT observation_id,normalized_json,file_generation_id,byte_offset,byte_end FROM observations WHERE session_key=?1 ORDER BY file_generation_id,byte_offset")?;
        let mut rows = query.query([&ledger.session_key])?;
        while let Some(row) = rows.next()? {
            let observation_id: String = row.get(0)?;
            let generation: String = row.get(2)?;
            let offset: i64 = row.get(3)?;
            let end: i64 = row.get(4)?;
            let file = m
                .files
                .iter()
                .find(|f| f.generation_id == generation && end <= f.committed_offset)
                .ok_or(ErrorCode::CandidateObsolete)?;
            let key = format!(
                "physical-{:x}",
                Sha256::digest(serde_json::to_vec(&(&owner, &generation))?)
            );
            if !sequences.contains_key(&key) {
                let previously_canonical = tx.query_row("SELECT EXISTS(SELECT 1 FROM canonical_usage_sequence c JOIN observations o ON o.observation_id=c.observation_id JOIN sessions s ON s.active_ledger_id=c.ledger_id WHERE s.session_key=?1 AND c.ordinal=0 AND o.file_generation_id=?2)", params![owner,generation], |r|r.get(0))?;
                let has_trusted_history = tx.query_row("SELECT EXISTS(SELECT 1 FROM active_usage_events e JOIN observations o ON o.observation_id=e.origin_observation_id WHERE e.session_key=?1 AND o.file_generation_id=?2)", params![owner,generation], |r|r.get(0))?;
                sequences.insert(
                    key.clone(),
                    PhysicalReplaySequence {
                        physical: PhysicalSequence {
                            sequence_key: key.clone(),
                            owner_session_key: owner.clone(),
                            identity: SequenceIdentity {
                                provider_namespace: ledger.provider.clone(),
                                provider_session_id: ledger
                                    .provider_session_id
                                    .clone()
                                    .unwrap_or_default(),
                                created_at_ms: ledger.created_at_ms,
                                parent_provider_id: ledger.parent_provider_id.clone(),
                            },
                            records: vec![],
                            starts_at_session_head: false,
                            scanned_to_upper_bound: file.committed_offset == file.observed_size,
                            previously_canonical,
                            has_trusted_history,
                        },
                        file_generation_id: generation.clone(),
                        observation_ids: vec![],
                    },
                );
                footprint = footprint
                    .checked_add(1024 + key.len() + owner.len() + generation.len())
                    .ok_or(ErrorCode::NumericOverflow)?;
            }
            let encoded: String = row.get(1)?;
            if encoded.len() > 16 * 1024 * 1024 {
                return Err(ErrorCode::InvalidQuery.into());
            }
            let record: NormalizedObservation = json(&encoded)?;
            let sequence = sequences.get_mut(&key).ok_or(ErrorCode::DbCorrupt)?;
            match record {
                NormalizedObservation::SessionMetadata {
                    provider_session_id,
                    created_at_ms,
                    metadata,
                    ..
                } => {
                    if offset == 0 {
                        if provider_session_id != sequence.physical.identity.provider_session_id {
                            return Err(ErrorCode::CandidateObsolete.into());
                        }
                        sequence.physical.starts_at_session_head = true;
                        sequence.physical.identity.created_at_ms = created_at_ms;
                        sequence.physical.identity.parent_provider_id = metadata.parent_provider_id;
                    }
                }
                NormalizedObservation::Usage(u) => {
                    let signature = UsageSignature::from(&u);
                    footprint = footprint
                        .checked_add(
                            serde_json::to_vec(&signature)?.len() + observation_id.len() + 256,
                        )
                        .ok_or(ErrorCode::NumericOverflow)?;
                    sequence.physical.records.push(signature);
                    sequence.observation_ids.push(observation_id);
                }
                _ => {}
            }
            if footprint > 128 * 1024 * 1024 {
                return Err(ErrorCode::InvalidQuery.into());
            }
        }
    }
    let mut physical = sequences.into_values().collect::<Vec<_>>();
    // The planner consumes only immutable usage signatures. Metadata stays in the observations.
    let inputs = physical
        .iter_mut()
        .map(|s| PhysicalSequence {
            sequence_key: s.physical.sequence_key.clone(),
            owner_session_key: s.physical.owner_session_key.clone(),
            identity: s.physical.identity.clone(),
            records: std::mem::take(&mut s.physical.records),
            starts_at_session_head: s.physical.starts_at_session_head,
            scanned_to_upper_bound: s.physical.scanned_to_upper_bound,
            previously_canonical: s.physical.previously_canonical,
            has_trusted_history: s.physical.has_trusted_history,
        })
        .collect::<Vec<_>>();
    let plan = canonical::plan(&inputs)?;
    for (sequence, input) in physical.iter_mut().zip(inputs) {
        sequence.physical.records = input.records;
    }
    Ok(CanonicalReplayPlan {
        plan,
        sequences: physical
            .into_iter()
            .map(|s| (s.physical.sequence_key.clone(), s))
            .collect(),
    })
}
impl Database {
    pub fn candidate_event_ids(
        &self,
        job: &str,
        ledger: &str,
        from: i64,
        count: usize,
    ) -> StoreResult<Vec<Option<String>>> {
        if from < 0 || count == 0 || count > 256 {
            return Err(ErrorCode::InvalidQuery.into());
        }
        self.snapshot(|tx,_| {
            let m=manifest(tx,job)?;fresh(tx,&m)?;
            if !m.ledgers.iter().any(|l|l.candidate_ledger_id==ledger){return Err(ErrorCode::InvalidQuery.into());}
            let end=from.checked_add(count as i64).ok_or(ErrorCode::NumericOverflow)?;
            let mut q=tx.prepare("SELECT event_id FROM canonical_usage_sequence WHERE ledger_id=?1 AND ordinal>=?2 AND ordinal<?3 ORDER BY ordinal")?;
            let result=q.query_map(params![ledger,from,end],|r|r.get::<_,Option<String>>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
            if result.len()!=count{return Err(ErrorCode::InvalidUsage.into());}Ok(result)
        })
    }
    pub fn stage_candidate_ordinals(
        &self,
        job: String,
        ledger: String,
        from: i64,
        observation_ids: Vec<String>,
    ) -> StoreResult<()> {
        if from < 0 || observation_ids.is_empty() || observation_ids.len() > 256 {
            return Err(ErrorCode::InvalidQuery.into());
        }
        self.write(move|conn| {
            let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            require_state(&tx,&job,JobState::Running)?;let m=manifest(&tx,&job)?;fresh(&tx,&m)?;
            if !m.ledgers.iter().any(|l|l.candidate_ledger_id==ledger){return Err(ErrorCode::InvalidQuery.into());}
            let count:i64=tx.query_row("SELECT COUNT(*) FROM canonical_usage_sequence WHERE ledger_id=?1",[&ledger],|r|r.get(0))?;
            let sealed:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM file_usage_cursors WHERE ledger_id=?1)",[&ledger],|r|r.get(0))?;
            if sealed{return Err(ErrorCode::RevisionConflict.into());}
            if count!=from {return Err(ErrorCode::RevisionConflict.into());}
            for (index,observation) in observation_ids.into_iter().enumerate() {
                batch::same_session(&tx,&ledger,&observation)?;
                let classified:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM usage_events WHERE ledger_id=?1 AND origin_observation_id=?2 UNION ALL SELECT 1 FROM pending_usage WHERE ledger_id=?1 AND observation_id=?2)",params![ledger,observation],|r|r.get(0))?;
                if !classified{return Err(ErrorCode::InvalidUsage.into());}
                let ordinal=from.checked_add(index as i64).ok_or(ErrorCode::NumericOverflow)?;
                tx.execute("INSERT INTO canonical_usage_sequence SELECT ?1,?2,?3,(SELECT event_id FROM usage_events WHERE ledger_id=?1 AND origin_observation_id=?3)",params![ledger,ordinal,observation])?;
            }
            tx.commit()?;Ok(())
        })
    }
    /// Rechecks canonical origin selection and fixes episode frontiers by canonical order.
    pub fn finish_candidate_alignment(&self, job: String) -> StoreResult<()> {
        let (m, result) = self.snapshot(|tx, _| {
            require_state(tx, &job, JobState::Running)?;
            let m = manifest(tx, &job)?;
            fresh(tx, &m)?;
            let result = load(tx, &m)?;
            Ok((m, result))
        })?;
        self.write(move|conn| {
            let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            require_state(&tx,&job,JobState::Running)?;fresh(&tx,&m)?;
            for group in &result.plan.groups {
                let ledger=&m.ledgers.iter().find(|l|l.session_key==group.session_key).ok_or(ErrorCode::InvalidQuery)?.candidate_ledger_id;
                let mut q=tx.prepare("SELECT ordinal,observation_id FROM canonical_usage_sequence WHERE ledger_id=?1 ORDER BY ordinal")?;
                let mut rows=q.query([ledger])?;let mut count=0usize;
                while let Some(row)=rows.next()? {
                    let ordinal:i64=row.get(0)?;let observation:String=row.get(1)?;
                    let origin=group.origin_sequence_for(count).ok_or(ErrorCode::InvalidUsage)?;
                    if ordinal!=count as i64 || result.sequences[origin].observation_ids[count]!=observation {return Err(ErrorCode::InvalidUsage.into());}
                    count+=1;
                }
                if count!=group.record_count{return Err(ErrorCode::InvalidUsage.into());}
                drop(rows);drop(q);
                for member in &group.member_sequence_keys {
                    let sequence=&result.sequences[member];
                    tx.execute("INSERT INTO file_usage_cursors VALUES(?1,?2,?3,'aligned')",params![ledger,sequence.file_generation_id,sequence.observation_ids.len() as i64])?;
                }
                tx.execute("DELETE FROM stream_frontiers WHERE ledger_id=?1",[ledger])?;
                tx.execute("INSERT INTO stream_frontiers SELECT ledger_id,stream_key,episode_id FROM (SELECT st.ledger_id,st.stream_key,st.episode_id,ROW_NUMBER() OVER(PARTITION BY st.stream_key ORDER BY c.ordinal DESC) AS rank FROM stream_states st JOIN canonical_usage_sequence c ON c.ledger_id=st.ledger_id AND c.observation_id=st.last_observation_id WHERE st.ledger_id=?1) WHERE rank=1",[ledger])?;
                let missing:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM stream_states st WHERE st.ledger_id=?1 AND NOT EXISTS(SELECT 1 FROM canonical_usage_sequence c WHERE c.ledger_id=st.ledger_id AND c.observation_id=st.last_observation_id))",[ledger],|r|r.get(0))?;
                if missing{return Err(ErrorCode::InvalidUsage.into());}
            }
            for isolated in &result.plan.isolated {
                let sequence=&result.sequences[&isolated.sequence_key];
                let ledger=&m.ledgers.iter().find(|l|l.session_key==isolated.session_key).ok_or(ErrorCode::InvalidQuery)?.candidate_ledger_id;
                tx.execute("INSERT INTO file_usage_cursors VALUES(?1,?2,0,'rebuild_required')",params![ledger,sequence.file_generation_id])?;
            }
            tx.commit()?;Ok(())
        })
    }
    /// Produces the evidence internally, then persists it with the same frozen-input checks.
    /// This does not switch an active session or publish consumption.
    pub fn prepare_canonical_replay(&self, job_id: &str) -> StoreResult<CanonicalReplayPlan> {
        let (m, result) = self.snapshot(|tx, _| {
            require_state(tx, job_id, JobState::Running)?;
            let m = manifest(tx, job_id)?;
            fresh(tx, &m)?;
            let result = load(tx, &m)?;
            Ok((m, result))
        })?;
        let mut aliases: BTreeMap<String, (String, String)> = BTreeMap::new();
        for group in &result.plan.groups {
            let evidence = serde_json::to_string(
                &serde_json::json!({"version":1,"primary_sequence_key":group.primary_sequence_key,"longest_sequence_key":group.longest_sequence_key,"member_count":group.member_sequence_keys.len().to_string(),"member_digest":format!("{:x}",Sha256::digest(serde_json::to_vec(&group.member_sequence_keys)?)),"record_count":group.record_count.to_string()}),
            )?;
            for alias in &group.alias_session_keys {
                // A logical owner cannot be partially merged while another of its files conflicts.
                if result.sequences.values().any(|s| {
                    s.physical.owner_session_key == *alias
                        && !group
                            .member_sequence_keys
                            .contains(&s.physical.sequence_key)
                }) {
                    return Err(ErrorCode::InvalidUsage.into());
                }
                if aliases
                    .insert(alias.clone(), (group.session_key.clone(), evidence.clone()))
                    .is_some()
                {
                    return Err(ErrorCode::InvalidUsage.into());
                }
            }
        }
        let job_id = job_id.to_owned();
        self.write(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            require_state(&tx, &job_id, JobState::Running)?; fresh(&tx, &m)?;
            for l in &m.ledgers {
                let staged: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM usage_events WHERE ledger_id=?1 UNION ALL SELECT 1 FROM pending_usage WHERE ledger_id=?1 UNION ALL SELECT 1 FROM stream_states WHERE ledger_id=?1 UNION ALL SELECT 1 FROM context_snapshots WHERE ledger_id=?1)", [&l.candidate_ledger_id], |r|r.get(0))?;
                if staged { return Err(ErrorCode::RevisionConflict.into()); }
            }
            for (alias, (canonical, evidence)) in aliases {
                tx.execute("INSERT INTO candidate_session_aliases VALUES(?1,?2,?3,?4) ON CONFLICT(job_id,alias_session_key) DO NOTHING", params![job_id,alias,canonical,evidence])?;
                let stored: (String,String) = tx.query_row("SELECT canonical_session_key,evidence_json FROM candidate_session_aliases WHERE job_id=?1 AND alias_session_key=?2", params![job_id,alias], |r|Ok((r.get(0)?,r.get(1)?)))?;
                if stored != (canonical,evidence) { return Err(ErrorCode::RevisionConflict.into()); }
            }
            tx.commit()?; Ok(())
        })?;
        Ok(result)
    }
}
