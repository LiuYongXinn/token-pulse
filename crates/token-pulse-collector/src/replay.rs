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
    has_old_events: bool,
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
fn stopped(db: &Database, id: &str, stop: &impl Fn() -> bool) -> StoreResult<()> {
    if stop() || db.get_job(id)?.job.state == JobState::Cancelling {
        Err(ErrorCode::JobCancelled.into())
    } else {
        Ok(())
    }
}
fn sequences(
    db: &Database,
    m: &RebuildManifest,
    stop: &impl Fn() -> bool,
) -> StoreResult<BTreeMap<String, Sequence>> {
    let mut result = BTreeMap::new();
    let mut footprint = 0usize;
    for ledger in &m.ledgers {
        let mut seq = Sequence {
            identity: SequenceIdentity {
                provider_namespace: ledger.provider.clone(),
                provider_session_id: ledger.provider_session_id.clone().unwrap_or_default(),
                created_at_ms: ledger.created_at_ms,
                parent_provider_id: ledger.parent_provider_id.clone(),
            },
            signatures: vec![],
            head: false,
            complete: true,
            single_generation: true,
            has_old_events: db.active_session_event_count(&ledger.session_key)? > 0,
        };
        let mut after: Option<(String, i64)> = None;
        let mut generations = BTreeSet::new();
        loop {
            stopped(db, &m.job_id, stop)?;
            let records = db.replay_records(
                &m.job_id,
                &ledger.session_key,
                after.as_ref().map(|(g, o)| (g.as_str(), *o)),
            )?;
            if records.is_empty() {
                break;
            }
            for record in records {
                let p = position(&record.record);
                after = Some((p.file_generation_id.clone(), p.byte_offset as i64));
                generations.insert(p.file_generation_id.clone());
                if matches!(
                    &record.record,
                    NormalizedObservation::SessionMetadata { .. }
                ) && p.byte_offset == 0
                {
                    seq.head = true;
                }
                if let NormalizedObservation::Usage(u) = record.record {
                    let signature = UsageSignature::from(&u);
                    footprint = footprint
                        .checked_add(serde_json::to_vec(&signature)?.len() + 128)
                        .ok_or(ErrorCode::NumericOverflow)?;
                    if footprint > 128 * 1024 * 1024 {
                        return Err(ErrorCode::InvalidQuery.into());
                    }
                    seq.signatures.push(signature);
                }
            }
        }
        seq.complete = generations.iter().all(|g| {
            m.files
                .iter()
                .any(|f| &f.generation_id == g && f.committed_offset == f.observed_size)
        });
        seq.single_generation = generations.len() <= 1;
        result.insert(ledger.session_key.clone(), seq);
    }
    Ok(result)
}
fn verify_physical_inputs(
    db: &Database,
    m: &RebuildManifest,
    stop: &impl Fn() -> bool,
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
    stop: &impl Fn() -> bool,
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
    let m = db.prepare_rebuild(job_id.into(), now())?;
    let seqs = sequences(db, &m, stop)?;
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
    // Existing trusted primary remains primary. Other physical sequences stay pending until a full mirror mapping is published.
    let mut primaries = BTreeSet::new();
    for (key, seq) in &seqs {
        let peers = seqs
            .iter()
            .filter(|(_, s)| {
                s.identity.provider_namespace == seq.identity.provider_namespace
                    && s.identity.provider_session_id == seq.identity.provider_session_id
            })
            .collect::<Vec<_>>();
        if peers.len() == 1
            || (seq.has_old_events && peers.iter().filter(|(_, s)| s.has_old_events).count() == 1)
        {
            primaries.insert(key.clone());
        }
    }
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
        let mut after: Option<(String, i64)> = None;
        let mut usage_index = 0usize;
        let mut outcomes = vec![];
        let mut revisions = BTreeMap::new();
        loop {
            stopped(db, job_id, stop)?;
            let records =
                db.replay_records(job_id, &key, after.as_ref().map(|(g, o)| (g.as_str(), *o)))?;
            if records.is_empty() {
                break;
            }
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
                after = Some((pos.file_generation_id.clone(), pos.byte_offset as i64));
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
                let NormalizedObservation::Usage(u) = record.record else {
                    continue;
                };
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
                if seqs.values().any(|child| {
                    child.identity.provider_namespace == sequence.identity.provider_namespace
                        && child.identity.parent_provider_id.as_ref()
                            == Some(&sequence.identity.provider_session_id)
                }) {
                    result_footprint = result_footprint
                        .checked_add(1024)
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
