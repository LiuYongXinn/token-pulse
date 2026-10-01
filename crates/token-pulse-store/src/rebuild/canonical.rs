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
