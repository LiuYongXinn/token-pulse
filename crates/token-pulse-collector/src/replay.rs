//! Bounded replay of necessary observations. The storage layer owns validation and publication.
use crate::{id, method_name, validate_source_file};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
use token_pulse_core::{
    accounting::*,
    domain::*,
    error::ErrorCode,
    jobs::JobProgress,
    numeric::DecimalInt,
    protocol::{JobKind, JobState},
    reader::{self, ReaderCheckpoint, ReaderLimits},
    sequence::*,
};
use token_pulse_store::{Database, StoreResult, batch::*, jobs::JobAdvance, rebuild::*};

struct Sequence {
    identity: SequenceIdentity,
    signatures: Vec<UsageSignature>,
    head: bool,
    complete: bool,
    single_generation: bool,
}
impl Sequence {
    fn view(&self) -> SessionSequence<'_> {
        SessionSequence {
            identity: &self.identity,
            records: &self.signatures,
            starts_at_session_head: self.head,
            scanned_to_upper_bound: self.complete,
        }
    }
}
struct ParentOutcome {
    reference: Option<CanonicalReference>,
    baseline: Option<StreamBaseline>,
    observation_id: String,
}
struct ReplaySequences {
    sequences: BTreeMap<String, Sequence>,
    origins: BTreeMap<String, Vec<String>>,
}
fn position(record: &NormalizedObservation) -> &PhysicalPosition {
    match record {
        NormalizedObservation::Usage(u) => &u.physical_position,
        NormalizedObservation::SessionMetadata {
            physical_position, ..
        }
        | NormalizedObservation::TurnMetadata {
            physical_position, ..
        }
        | NormalizedObservation::Context {
            physical_position, ..
        } => physical_position,
    }
}
fn decimal(n: usize) -> DecimalInt {
    DecimalInt::from_nonnegative(n as i128).unwrap()
}
fn progress_of(db: &Database, id: &str) -> StoreResult<JobProgress> {
    let j = db.get_job(id)?.job;
    Ok(JobProgress {
        phase: j.phase,
        discovered_files: j.discovered_files,
        discovery_complete: j.discovery_complete,
        processed_files: j.processed_files,
        processed_bytes: j.processed_bytes,
        accepted_events: j.accepted_events,
        pending_observations: j.pending_observations,
    })
}
fn advance(
    db: &Database,
    id: &str,
    expected: JobState,
    next: JobState,
    phase: &str,
    now: i64,
) -> StoreResult<()> {
    let mut p = progress_of(db, id)?;
    p.phase = phase.into();
    let checkpoint = db.get_job(id)?.checkpoint;
    db.advance_job(
        id.into(),
        JobAdvance {
            expected,
            next,
            progress: p,
            checkpoint,
            error: None,
            at_ms: now,
        },
    )?;
    Ok(())
}
fn stopped(db: &Database, id: &str, stop: &impl Fn() -> Option<ErrorCode>) -> StoreResult<()> {
    if db.get_job(id)?.job.state == JobState::Cancelling {
        Err(ErrorCode::JobCancelled.into())
    } else if let Some(code) = stop() {
        Err(code.into())
    } else {
        Ok(())
    }
}
fn sequences(canonical: &mut CanonicalReplayPlan) -> StoreResult<ReplaySequences> {
    let mut result = BTreeMap::new();
    let mut origins = BTreeMap::new();
    for group in &canonical.plan.groups {
        let primary = canonical
            .sequences
            .get_mut(&group.primary_sequence_key)
            .ok_or(ErrorCode::InvalidQuery)?;
        let mut signatures = std::mem::take(&mut primary.physical.records);
        let mut observation_ids = primary.observation_ids.clone();
        let mut seq = Sequence {
            identity: primary.physical.identity.clone(),
            signatures: vec![],
            head: primary.physical.starts_at_session_head,
            complete: primary.physical.scanned_to_upper_bound,
            single_generation: true,
        };
        if group.longest_sequence_key != group.primary_sequence_key {
            let longest = canonical
                .sequences
                .get_mut(&group.longest_sequence_key)
                .ok_or(ErrorCode::InvalidQuery)?;
            signatures.extend(longest.physical.records.drain(group.primary_record_count..));
            observation_ids
                .extend_from_slice(&longest.observation_ids[group.primary_record_count..]);
            seq.complete = longest.physical.scanned_to_upper_bound;
        }
        seq.signatures = signatures;
        origins.insert(group.session_key.clone(), observation_ids);
        result.insert(group.session_key.clone(), seq);
    }
    Ok(ReplaySequences {
        sequences: result,
        origins,
    })
}
fn empty_batch(job: &str) -> CandidateBatch {
    CandidateBatch {
        job_id: job.into(),
        events: vec![],
        streams: vec![],
        provenance: vec![],
        pending: vec![],
        contexts: vec![],
    }
}
fn classify_copies(
    db: &Database,
    m: &RebuildManifest,
    canonical: &CanonicalReplayPlan,
    origins: &BTreeMap<String, Vec<String>>,
    stop: &impl Fn() -> Option<ErrorCode>,
) -> StoreResult<()> {
    for group in &canonical.plan.groups {
        let ledger = &m
            .ledgers
            .iter()
            .find(|l| l.session_key == group.session_key)
            .ok_or(ErrorCode::InvalidQuery)?
            .candidate_ledger_id;
        for member in &group.member_sequence_keys {
            let sequence = &canonical.sequences[member];
            let mut index = 0usize;
            while index < sequence.observation_ids.len() {
                stopped(db, &m.job_id, stop)?;
                let end = (index + 256).min(sequence.observation_ids.len());
                let records =
                    db.replay_observation_ids(&m.job_id, &sequence.observation_ids[index..end])?;
                let events =
                    db.candidate_event_ids(&m.job_id, ledger, index as i64, records.len())?;
                let mut batch = empty_batch(&m.job_id);
                for (record, event) in records.into_iter().zip(events) {
                    let origin = &origins[&group.session_key][index];
                    index += 1;
                    if record.observation_id == *origin {
                        continue;
                    }
                    let NormalizedObservation::Usage(u) = record.record else {
                        return Err(ErrorCode::InvalidUsage.into());
                    };
                    if let Some(event_id) = event {
                        batch.provenance.push(ProvenanceWrite {
                            event_id,
                            observation_id: record.observation_id.clone(),
                            relation: "mirror".into(),
                        });
                    }
                    batch.pending.push(PendingWrite {
                        pending_id: id("pending", &format!("{ledger}:{}", record.observation_id)),
                        ledger_id: ledger.clone(),
                        observation_id: record.observation_id,
                        quality: ObservationQuality::Duplicate,
                        reason_code: "verified_mirror".into(),
                        vector: u.last.or(u.cumulative),
                        evidence: PendingEvidence {
                            related_observation_ids: vec![origin.clone()],
                            ..Default::default()
                        },
                    });
                }
                if !batch.pending.is_empty() {
                    db.stage_candidate_batch(batch)?;
                }
            }
        }
    }
    for isolated in &canonical.plan.isolated {
        let sequence = &canonical.sequences[&isolated.sequence_key];
        let ledger = &m
            .ledgers
            .iter()
            .find(|l| l.session_key == isolated.session_key)
            .ok_or(ErrorCode::InvalidQuery)?
            .candidate_ledger_id;
        let mut index = 0usize;
        while index < sequence.observation_ids.len() {
            stopped(db, &m.job_id, stop)?;
            let end = (index + 256).min(sequence.observation_ids.len());
            let records =
                db.replay_observation_ids(&m.job_id, &sequence.observation_ids[index..end])?;
            let mut batch = empty_batch(&m.job_id);
            for record in records {
                index += 1;
                let NormalizedObservation::Usage(u) = record.record else {
                    return Err(ErrorCode::InvalidUsage.into());
                };
                batch.pending.push(PendingWrite {
                    pending_id: id("pending", &format!("{ledger}:{}", record.observation_id)),
                    ledger_id: ledger.clone(),
                    observation_id: record.observation_id,
                    quality: ObservationQuality::Pending,
                    reason_code: match isolated.reason {
                        SequenceDecision::IdentityConflict => "sequence_identity_conflict",
                        SequenceDecision::PendingIncomplete => "sequence_incomplete",
                        _ => "sequence_unproven",
                    }
                    .into(),
                    vector: u.last.or(u.cumulative),
                    evidence: PendingEvidence::default(),
                });
            }
            db.stage_candidate_batch(batch)?;
        }
    }
    Ok(())
}
fn verify_physical_inputs(
    db: &Database,
    m: &RebuildManifest,
    stop: &impl Fn() -> Option<ErrorCode>,
) -> StoreResult<()> {
    for input in &m.files {
        stopped(db, &m.job_id, stop)?;
        let target = db.rebuild_file_path(&input.file_id)?;
        let path = Path::new(&target.path);
        match std::fs::symlink_metadata(path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err(ErrorCode::SourceUnreadable.into()),
            Ok(_) => {}
        }
        validate_source_file(Path::new(&target.source_root), path)?;
        let checkpoint = ReaderCheckpoint {
            file_identity: serde_json::from_str(&input.identity_json)?,
            observed_size: Some(input.observed_size as u64),
            committed_offset: input.committed_offset as u64,
            anchors: serde_json::from_str(&input.anchors_json)?,
            ..Default::default()
        };
        reader::read_batch(
            path,
            &input.generation_id,
            &checkpoint,
            &ReaderLimits {
                records: 1,
                ..Default::default()
            },
        )
        .map_err(|e| {
            if e.code() == ErrorCode::CheckpointConflict {
                ErrorCode::CandidateObsolete
            } else {
                e.code()
            }
        })?;
    }
    Ok(())
}
/// The caller has already persisted a queued Rebuild request. No UI or external process is required.
pub fn execute_rebuild(
    db: &Database,
    job_id: &str,
    stop: impl Fn() -> bool,
    now: impl Fn() -> i64,
) -> StoreResult<i64> {
    execute_rebuild_controlled(
        db,
        job_id,
        || {
            if stop() {
                Some(ErrorCode::JobCancelled)
            } else {
                None
            }
        },
        now,
    )
}
pub fn execute_rebuild_controlled(
    db: &Database,
    job_id: &str,
    stop: impl Fn() -> Option<ErrorCode>,
    now: impl Fn() -> i64,
) -> StoreResult<i64> {
    let job = db.get_job(job_id)?;
    if job.job.kind != JobKind::Rebuild || job.job.state != JobState::Queued {
        return Err(ErrorCode::InvalidQuery.into());
    }
    let result = run(db, job_id, &stop, &now);
    if let Err(error) = &result {
        let _ = db.fail_rebuild(job_id.into(), error.code, now());
    }
    result
}
fn run(
    db: &Database,
    job_id: &str,
    stop: &impl Fn() -> Option<ErrorCode>,
    now: &impl Fn() -> i64,
) -> StoreResult<i64> {
    stopped(db, job_id, stop)?;
    advance(
        db,
        job_id,
        JobState::Queued,
        JobState::Running,
        "planning",
        now(),
    )?;
    if !db.rebuild_has_targets(&db.get_job(job_id)?.request.scope)? {
        let mut progress = progress_of(db, job_id)?;
        progress.discovery_complete = true;
        db.checkpoint_job(
            job_id.into(),
            JobState::Running,
            progress,
            db.get_job(job_id)?.checkpoint,
            now(),
        )?;
        stopped(db, job_id, stop)?;
        advance(
            db,
            job_id,
            JobState::Running,
            JobState::Validating,
            "validating",
            now(),
        )?;
        stopped(db, job_id, stop)?;
        advance(
            db,
            job_id,
            JobState::Validating,
            JobState::Publishing,
            "publishing",
            now(),
        )?;
        advance(
            db,
            job_id,
            JobState::Publishing,
            JobState::Succeeded,
            "complete",
            now(),
        )?;
        return db.snapshot(|_, revision| Ok(revision.data));
    }
    let m = db.prepare_rebuild(job_id.into(), now())?;
    stopped(db, job_id, stop)?;
    let mut canonical = db.prepare_canonical_replay(job_id)?;
    let ReplaySequences {
        sequences: seqs,
        origins,
    } = sequences(&mut canonical)?;
    let mut p = JobProgress {
        phase: "replaying_observations".into(),
        discovered_files: decimal(m.files.len()),
        discovery_complete: true,
        ..Default::default()
    };
    let mut checkpoint = db.get_job(job_id)?.checkpoint;
    db.checkpoint_job(
        job_id.into(),
        JobState::Running,
        p.clone(),
        checkpoint.clone(),
        now(),
    )?;
    let primaries = seqs.keys().cloned().collect::<BTreeSet<_>>();
    let mut order = vec![];
    let mut cycles = BTreeSet::new();
    let mut remaining = seqs.keys().cloned().collect::<BTreeSet<_>>();
    while !remaining.is_empty() {
        let next = remaining
            .iter()
            .find(|key| {
                let s = &seqs[*key];
                !remaining.iter().any(|other| {
                    other != *key
                        && seqs[other].identity.provider_namespace == s.identity.provider_namespace
                        && s.identity.parent_provider_id.as_ref()
                            == Some(&seqs[other].identity.provider_session_id)
                })
            })
            .cloned();
        if let Some(next) = next {
            remaining.remove(&next);
            order.push(next);
        } else {
            cycles.extend(remaining.iter().cloned());
            order.extend(remaining);
            break;
        }
    }
    let mut parent_results: BTreeMap<String, Vec<ParentOutcome>> = BTreeMap::new();
    let mut result_footprint = 0usize;
    for key in order {
        let sequence = &seqs[&key];
        let has_children = seqs.values().any(|child| {
            child.identity.provider_namespace == sequence.identity.provider_namespace
                && child.identity.parent_provider_id.as_ref()
                    == Some(&sequence.identity.provider_session_id)
        });
        let ledger = &m
            .ledgers
            .iter()
            .find(|l| l.session_key == key)
            .ok_or(ErrorCode::InvalidQuery)?
            .candidate_ledger_id;
        let parent_candidates = seqs
            .iter()
            .filter(|(k, s)| {
                primaries.contains(*k)
                    && sequence.identity.parent_provider_id.as_ref()
                        == Some(&s.identity.provider_session_id)
                    && s.identity.provider_namespace == sequence.identity.provider_namespace
            })
            .collect::<Vec<_>>();
        let decision = align_lineage(
            &parent_candidates
                .iter()
                .map(|(_, s)| s.view())
                .collect::<Vec<_>>(),
            &sequence.view(),
        );
        let inherited = match decision {
            SequenceDecision::Inherited { aligned_prefix, .. }
                if parent_candidates.len() == 1 && !cycles.contains(&key) =>
            {
                Some((parent_candidates[0].0.as_str(), aligned_prefix))
            }
            _ => None,
        };
        let mut state = AccountingState::new(key.clone());
        let mut usage_index = 0usize;
        let mut outcomes = vec![];
        let mut revisions = BTreeMap::new();
        loop {
            stopped(db, job_id, stop)?;
            if usage_index == origins[&key].len() {
                break;
            }
            let page_start = usage_index;
            let end = (usage_index + 256).min(origins[&key].len());
            let records = db.replay_observation_ids(job_id, &origins[&key][usage_index..end])?;
            let mut batch = CandidateBatch {
                job_id: job_id.into(),
                events: vec![],
                streams: vec![],
                provenance: vec![],
                pending: vec![],
                contexts: vec![],
            };
            let mut updates = BTreeMap::new();
            for record in records {
                let pos = position(&record.record);
                checkpoint.batch_position = DecimalInt::from_nonnegative(
                    checkpoint
                        .batch_position
                        .value()
                        .checked_add(1)
                        .ok_or(ErrorCode::NumericOverflow)?,
                )?;
                p.processed_bytes = DecimalInt::from_nonnegative(
                    p.processed_bytes
                        .value()
                        .checked_add(i128::from(pos.byte_end - pos.byte_offset))
                        .ok_or(ErrorCode::NumericOverflow)?,
                )?;
                let NormalizedObservation::Usage(mut u) = record.record else {
                    return Err(ErrorCode::InvalidUsage.into());
                };
                u.session_key = key.clone();
                let mut evidence = AccountingEvidence {
                    independent_new_stream: usage_index == 0
                        && sequence.head
                        && sequence.single_generation
                        && primaries.contains(&key),
                    lineage: if primaries.contains(&key)
                        && sequence.identity.parent_provider_id.is_none()
                        && sequence.single_generation
                    {
                        LineageEvidence::Independent
                    } else {
                        LineageEvidence::Pending
                    },
                    ..Default::default()
                };
                let mut related = vec![];
                if let Some((parent, prefix)) = inherited {
                    if usage_index < prefix {
                        if let Some(outcome) = parent_results
                            .get(parent)
                            .and_then(|rows| rows.get(usage_index))
                        {
                            if let Some(reference) = &outcome.reference {
                                evidence.lineage = LineageEvidence::VerifiedInherited {
                                    reference: Box::new(reference.clone()),
                                    baseline: outcome.baseline.clone().map(Box::new),
                                };
                                related.push(outcome.observation_id.clone());
                            }
                        }
                    } else if !state.streams.is_empty() {
                        evidence.lineage = LineageEvidence::Independent;
                    }
                }
                let result = account(&state, &u, &evidence);
                let method = method_name(result.method);
                let event_id = id("event", &format!("{ledger}:{}", record.observation_id));
                if let Some(usage) = result.event_usage {
                    batch.events.push(EventWrite {
                        event_id: event_id.clone(),
                        ledger_id: ledger.clone(),
                        origin_observation_id: record.observation_id.clone(),
                        occurred_at_ms: u.event_time_ms.ok_or(ErrorCode::InvalidUsage)?,
                        semantic_key: u
                            .request_identity
                            .as_ref()
                            .map(serde_json::to_string)
                            .transpose()?,
                        episode_id: result
                            .episode_id
                            .clone()
                            .unwrap_or_else(|| id("episode", &record.observation_id)),
                        model: u.effective_metadata.model.clone(),
                        project_id: None,
                        turn_id: u.effective_metadata.turn_id.clone(),
                        usage,
                        calculation_method: method.clone(),
                    });
                    p.accepted_events = DecimalInt::from_nonnegative(
                        p.accepted_events
                            .value()
                            .checked_add(1)
                            .ok_or(ErrorCode::NumericOverflow)?,
                    )?;
                }
                if result.quality != ObservationQuality::Confirmed
                    || result.prior_anchor.is_some()
                    || result.event_usage.is_none()
                {
                    let quality = if result.prior_anchor.is_some() {
                        ObservationQuality::Unattributed
                    } else if result.quality == ObservationQuality::Confirmed {
                        ObservationQuality::Duplicate
                    } else {
                        result.quality
                    };
                    batch.pending.push(PendingWrite {
                        pending_id: id("pending", &format!("{ledger}:{}", record.observation_id)),
                        ledger_id: ledger.clone(),
                        observation_id: record.observation_id.clone(),
                        quality,
                        reason_code: if result.quality == ObservationQuality::Confirmed
                            && result.event_usage.is_none()
                            && result.prior_anchor.is_none()
                        {
                            "zero_usage_excluded".into()
                        } else {
                            method.clone()
                        },
                        vector: result.prior_anchor.or(u.last).or(u.cumulative),
                        evidence: PendingEvidence {
                            candidate_stream_keys: result.candidate_stream_keys.clone(),
                            parent_session_key: inherited.map(|(k, _)| k.into()),
                            related_observation_ids: related,
                        },
                    });
                    if quality == ObservationQuality::Pending {
                        p.pending_observations = DecimalInt::from_nonnegative(
                            p.pending_observations
                                .value()
                                .checked_add(1)
                                .ok_or(ErrorCode::NumericOverflow)?,
                        )?;
                    }
                }
                if let Some(context) = result.context {
                    if let Some(time) = context.observed_at_ms {
                        batch.contexts.push(ContextWrite {
                            context_id: id(
                                "context",
                                &format!("{ledger}:{}", record.observation_id),
                            ),
                            ledger_id: ledger.clone(),
                            observation_id: record.observation_id.clone(),
                            observed_at_ms: time,
                            model: u.effective_metadata.model.clone(),
                            usage: context.usage,
                            model_context_window: context.model_context_window,
                            quality: context.quality,
                        });
                    }
                }
                for (stream, baseline) in &result.state.streams {
                    if state.streams.get(stream) != Some(baseline) {
                        let index = (stream.clone(), baseline.episode_id.clone());
                        updates.insert(
                            index.clone(),
                            StreamWrite {
                                ledger_id: ledger.clone(),
                                stream_key: stream.clone(),
                                episode_id: baseline.episode_id.clone(),
                                baseline: baseline.cumulative,
                                observation_id: record.observation_id.clone(),
                                quality: result.quality,
                                expected_state_revision: revisions.get(&index).copied(),
                            },
                        );
                    }
                }
                let reference_usage = result
                    .event_usage
                    .or(u.last)
                    .or(u.cumulative)
                    .filter(|v| v.validated_total().is_ok_and(|t| t.is_some()));
                let reference_usage =
                    reference_usage.filter(|_| result.quality != ObservationQuality::Pending);
                let baseline = result
                    .stream_key
                    .as_ref()
                    .and_then(|k| result.state.streams.get(k))
                    .cloned();
                if has_children {
                    result_footprint = result_footprint
                        .checked_add(serde_json::to_vec(&UsageSignature::from(&u))?.len() + 1024)
                        .ok_or(ErrorCode::NumericOverflow)?;
                    if result_footprint > 128 * 1024 * 1024 {
                        return Err(ErrorCode::InvalidQuery.into());
                    }
                    outcomes.push(ParentOutcome {
                        reference: reference_usage.map(|usage| CanonicalReference {
                            event_id,
                            usage,
                            observation: UsageSignature::from(&u),
                        }),
                        baseline,
                        observation_id: record.observation_id,
                    });
                }
                state = result.state;
                usage_index += 1;
            }
            for (index, update) in updates {
                revisions.insert(index, update.expected_state_revision.unwrap_or(0) + 1);
                batch.streams.push(update);
            }
            db.stage_candidate_batch(batch)?;
            db.stage_candidate_ordinals(
                job_id.into(),
                ledger.clone(),
                page_start as i64,
                origins[&key][page_start..usage_index].to_vec(),
            )?;
            db.checkpoint_job(
                job_id.into(),
                JobState::Running,
                p.clone(),
                checkpoint.clone(),
                now(),
            )?;
        }
        parent_results.insert(key, outcomes);
    }
    classify_copies(db, &m, &canonical, &origins, stop)?;
    drop(canonical);
    drop(seqs);
    drop(origins);
    drop(parent_results);
    stopped(db, job_id, stop)?;
    db.finish_candidate_alignment(job_id.into())?;
    p.processed_files = decimal(m.files.len());
    db.checkpoint_job(job_id.into(), JobState::Running, p, checkpoint, now())?;
    stopped(db, job_id, stop)?;
    advance(
        db,
        job_id,
        JobState::Running,
        JobState::Validating,
        "validating",
        now(),
    )?;
    verify_physical_inputs(db, &m, stop)?;
    db.validate_candidate(job_id)?;
    stopped(db, job_id, stop)?;
    advance(
        db,
        job_id,
        JobState::Validating,
        JobState::Publishing,
        "publishing",
        now(),
    )?;
    db.publish_candidate(job_id.into(), now())
}
