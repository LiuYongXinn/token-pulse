//! Independent collection service; no renderer, IDE, authentication or external CLI dependencies.
pub mod service;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};
use token_pulse_core::{
    accounting::*,
    adapter::{AdaptedRecord, adapt},
    domain::*,
    error::ErrorCode,
    reader::{self, FramedLine, ReaderCheckpoint, ReaderLimits},
    sequence::UsageSignature,
};
use token_pulse_store::{
    Database, FileRegistration, SessionRegistration, StoreResult, batch::*,
    collection::SessionAccounting,
};

pub struct CollectionReceipt {
    pub commit: CommitReceipt,
    pub has_more: bool,
    pub file_generation_id: String,
}
fn id(namespace: &str, value: &str) -> String {
    format!("{namespace}-{:x}", Sha256::digest(value.as_bytes()))
}
fn method_name(method: CalculationMethod) -> String {
    serde_json::to_value(method)
        .unwrap()
        .as_str()
        .unwrap()
        .into()
}

/// Only call with a discovered, enabled source file. One bounded batch becomes one Writer transaction.
pub fn collect_file(
    database: &Database,
    source_id: &str,
    path: &Path,
    observed_at_ms: i64,
) -> StoreResult<CollectionReceipt> {
    if !path.is_absolute() {
        return Err(ErrorCode::InvalidQuery.into());
    }
    let root = database
        .enabled_source_root(source_id)?
        .ok_or(ErrorCode::PermissionDenied)?;
    validate_source_file(Path::new(&root), path)?;
    let path_text = path.to_str().ok_or(ErrorCode::InvalidQuery)?.to_owned();
    let mut saved = database.file_checkpoint(source_id, &path_text, None)?;
    let mut checkpoint = ReaderCheckpoint::default();
    if let Some(saved) = &saved {
        checkpoint = ReaderCheckpoint {
            file_identity: saved.file_identity.clone(),
            observed_size: Some(saved.observed_size as u64),
            committed_offset: saved.committed_offset as u64,
            anchors: saved.anchors.clone(),
            oversized_line: saved.context.oversized_line.clone(),
            modified_at: None,
        };
    }
    let mut batch = reader::read_batch(
        path,
        saved
            .as_ref()
            .map(|f| f.file_generation_id.as_str())
            .unwrap_or("unregistered"),
        &checkpoint,
        &ReaderLimits::default(),
    )
    .map_err(|e| e.code())?;
    if saved.is_none() {
        if let Some(moved) =
            database.file_checkpoint(source_id, &path_text, Some(&batch.file_identity))?
        {
            database.relocate_file(moved.file_id.clone(), path_text.clone(), observed_at_ms)?;
            return collect_file(database, source_id, path, observed_at_ms);
        }
        let file_id = id("file", &format!("{source_id}:{}", batch.file_identity));
        let generation = id("generation", &file_id);
        database.register_file(FileRegistration {
            file_id,
            source_id: source_id.into(),
            canonical_path: path_text.clone(),
            file_identity: Some(batch.file_identity.clone()),
            file_generation_id: generation.clone(),
            observed_size: batch.observed_size as i64,
            created_at_ms: observed_at_ms,
            reader_context: ReaderContext::default(),
        })?;
        for line in &mut batch.lines {
            match line {
                FramedLine::Complete { position, .. } | FramedLine::Oversized { position } => {
                    position.file_generation_id = generation.clone()
                }
            }
        }
        saved = database.file_checkpoint(source_id, &path_text, None)?;
    }
    let saved = saved.ok_or(ErrorCode::CheckpointConflict)?;
    // Effective metadata is copied into each necessary observation, so a short raw usage line can
    // expand substantially. Reduce the record budget before preparing the durable batch.
    let mut preview_context = saved.context.clone();
    let mut largest_metadata = serde_json::to_vec(&preview_context.metadata)?.len();
    for line in &batch.lines {
        if let FramedLine::Complete { position, bytes } = line {
            let _ = adapt(bytes, position.clone(), &mut preview_context);
            largest_metadata =
                largest_metadata.max(serde_json::to_vec(&preview_context.metadata)?.len());
        }
    }
    let safe_records = (16 * 1024 * 1024 / (largest_metadata + 1024)).clamp(1, 500);
    if batch.lines.len() > safe_records {
        batch = reader::read_batch(
            path,
            &saved.file_generation_id,
            &checkpoint,
            &ReaderLimits {
                records: safe_records,
                ..Default::default()
            },
        )
        .map_err(|e| e.code())?;
    }
    let mut context = saved.context;
    let mut write = WriteBatch {
        file_generation_id: saved.file_generation_id.clone(),
        expected_offset: saved.committed_offset,
        expected_checkpoint_revision: saved.checkpoint_revision,
        next_offset: batch.next_offset as i64,
        observed_size: batch.observed_size as i64,
        anchors: batch.anchors,
        reader_context: ReaderContext::default(),
        ledgers: vec![],
        observations: vec![],
        events: vec![],
        streams: vec![],
        provenance: vec![],
        pending: vec![],
        contexts: vec![],
        diagnostics: vec![],
    };
    let mut sessions: BTreeMap<String, SessionAccounting> = BTreeMap::new();
    let mut streams: BTreeMap<(String, String, String), StreamWrite> = BTreeMap::new();
    for line in batch.lines {
        let record = match line {
            FramedLine::Oversized { position } => {
                context.independent_head_available = false;
                diagnostic(
                    &mut write,
                    source_id,
                    position.byte_offset,
                    ErrorCode::UnsupportedFormat,
                    observed_at_ms,
                );
                continue;
            }
            FramedLine::Complete { position, bytes } => match adapt(&bytes, position, &mut context)
            {
                AdaptedRecord::Ignored => continue,
                AdaptedRecord::Diagnostic(d) => {
                    context.independent_head_available = false;
                    diagnostic(
                        &mut write,
                        source_id,
                        d.position.byte_offset,
                        d.code,
                        observed_at_ms,
                    );
                    continue;
                }
                AdaptedRecord::Observation(o) => *o,
            },
        };
        if let NormalizedObservation::SessionMetadata {
            provider_session_id,
            created_at_ms,
            metadata,
            ..
        } = &record
        {
            // Separate physical sequences until a full mirror/lineage proof publishes a shared ledger.
            let session = id(
                "session",
                &format!("{}:{}", saved.file_id, provider_session_id),
            );
            let related = database.related_session_exists(provider_session_id, &session)?;
            context.requires_sequence_rebuild = related || metadata.parent_provider_id.is_some();
            let previously_observed = database.session_has_usage(&session)?
                || write.observations.iter().any(|o| {
                    o.session_key.as_deref() == Some(&session)
                        && matches!(o.record, NormalizedObservation::Usage(_))
                });
            context.independent_head_available =
                !context.requires_sequence_rebuild && !previously_observed;
            context.session_key = Some(session.clone());
            database.ensure_session(SessionRegistration {
                session_key: session.clone(),
                provider_session_id: Some(provider_session_id.clone()),
                parent_key: None,
                parent_provider_id: metadata.parent_provider_id.clone(),
                created_at_ms: *created_at_ms,
                ledger_id: id("ledger", &session),
                registered_at_ms: observed_at_ms,
            })?;
        }
        let session_key = context.session_key.clone();
        if let Some(session) = &session_key {
            if !sessions.contains_key(session) {
                sessions.insert(session.clone(), database.session_accounting(session)?);
            }
        }
        let (position, fingerprint) = match &record {
            NormalizedObservation::SessionMetadata {
                physical_position, ..
            }
            | NormalizedObservation::TurnMetadata {
                physical_position, ..
            }
            | NormalizedObservation::Context {
                physical_position, ..
            } => (
                physical_position,
                id("metadata", &serde_json::to_string(&record)?),
            ),
            NormalizedObservation::Usage(u) => (
                &u.physical_position,
                UsageSignature::from(u).candidate_fingerprint(),
            ),
        };
        let observation_id = id(
            "observation",
            &format!("{}:{}", write.file_generation_id, position.byte_offset),
        );
        if let NormalizedObservation::Usage(usage) = &record {
            let session = sessions
                .get_mut(&usage.session_key)
                .ok_or(ErrorCode::CheckpointConflict)?;
            let evidence = AccountingEvidence {
                independent_new_stream: context.independent_head_available,
                lineage: if context.requires_sequence_rebuild {
                    LineageEvidence::Pending
                } else {
                    LineageEvidence::Independent
                },
                ..Default::default()
            };
            let result = account(&session.state, usage, &evidence);
            context.independent_head_available = false;
            let method = method_name(result.method);
            if let Some(event_usage) = result.event_usage {
                write.events.push(EventWrite {
                    event_id: id("event", &format!("{}:{observation_id}", session.ledger_id)),
                    ledger_id: session.ledger_id.clone(),
                    origin_observation_id: observation_id.clone(),
                    occurred_at_ms: usage.event_time_ms.ok_or(ErrorCode::InvalidUsage)?,
                    semantic_key: usage
                        .request_identity
                        .as_ref()
                        .map(serde_json::to_string)
                        .transpose()?,
                    episode_id: result
                        .episode_id
                        .clone()
                        .unwrap_or_else(|| id("episode", &observation_id)),
                    model: usage.effective_metadata.model.clone(),
                    project_id: None,
                    turn_id: usage.effective_metadata.turn_id.clone(),
                    usage: event_usage,
                    calculation_method: method.clone(),
                });
            }
            if result.quality != ObservationQuality::Confirmed || result.prior_anchor.is_some() {
                write.pending.push(PendingWrite {
                    pending_id: id(
                        "pending",
                        &format!("{}:{observation_id}", session.ledger_id),
                    ),
                    ledger_id: session.ledger_id.clone(),
                    observation_id: observation_id.clone(),
                    quality: if result.quality == ObservationQuality::Confirmed {
                        ObservationQuality::Unattributed
                    } else {
                        result.quality
                    },
                    reason_code: method.clone(),
                    vector: result.prior_anchor.or(usage.last).or(usage.cumulative),
                    evidence: PendingEvidence {
                        candidate_stream_keys: result.candidate_stream_keys.clone(),
                        parent_session_key: None,
                        related_observation_ids: vec![],
                    },
                });
            }
            if let Some(request) = result.context {
                if let Some(time) = request.observed_at_ms {
                    write.contexts.push(ContextWrite {
                        context_id: id(
                            "context",
                            &format!("{}:{observation_id}", session.ledger_id),
                        ),
                        ledger_id: session.ledger_id.clone(),
                        observation_id: observation_id.clone(),
                        observed_at_ms: time,
                        model: usage.effective_metadata.model.clone(),
                        usage: request.usage,
                        model_context_window: request.model_context_window,
                        quality: request.quality,
                    });
                }
            }
            if let Some(error) = result.error {
                diagnostic(
                    &mut write,
                    source_id,
                    position.byte_offset,
                    error,
                    observed_at_ms,
                );
            }
            for (key, baseline) in &result.state.streams {
                if session.state.streams.get(key) != Some(baseline) {
                    streams.insert(
                        (
                            session.ledger_id.clone(),
                            key.clone(),
                            baseline.episode_id.clone(),
                        ),
                        StreamWrite {
                            ledger_id: session.ledger_id.clone(),
                            stream_key: key.clone(),
                            episode_id: baseline.episode_id.clone(),
                            baseline: baseline.cumulative,
                            observation_id: observation_id.clone(),
                            quality: result.quality,
                            expected_state_revision: session
                                .revisions
                                .get(&(key.clone(), baseline.episode_id.clone()))
                                .copied(),
                        },
                    );
                }
            }
            session.state = result.state;
        }
        write.observations.push(ObservationWrite {
            observation_id,
            session_key,
            record,
            payload_fingerprint: fingerprint,
        });
    }
    context.oversized_line = batch.oversized_line;
    write.reader_context = context;
    write.ledgers = sessions
        .into_iter()
        .map(|(session_key, s)| LedgerExpectation {
            session_key,
            ledger_id: s.ledger_id,
        })
        .collect();
    write.streams = streams.into_values().collect();
    let commit = database.commit(write)?;
    Ok(CollectionReceipt {
        commit,
        has_more: batch.has_more,
        file_generation_id: saved.file_generation_id,
    })
}
fn validate_source_file(root: &Path, path: &Path) -> StoreResult<()> {
    // Check lexical location before touching a supplied path, then verify actual location after resolution.
    if ![root.join("sessions"), root.join("archived_sessions")]
        .iter()
        .any(|tree| path.starts_with(tree))
        || !path
            .extension()
            .is_some_and(|s| s.eq_ignore_ascii_case("jsonl"))
    {
        return Err(ErrorCode::PermissionDenied.into());
    }
    let root = std::fs::canonicalize(root).map_err(|_| ErrorCode::SourceUnreadable)?;
    let canonical = std::fs::canonicalize(path).map_err(|_| ErrorCode::SourceUnreadable)?;
    if ![root.join("sessions"), root.join("archived_sessions")]
        .iter()
        .any(|tree| canonical.starts_with(tree))
        || !canonical
            .extension()
            .is_some_and(|s| s.eq_ignore_ascii_case("jsonl"))
    {
        return Err(ErrorCode::PermissionDenied.into());
    }
    Ok(())
}
fn diagnostic(write: &mut WriteBatch, source_id: &str, offset: u64, code: ErrorCode, at_ms: i64) {
    let key = format!("{}:{offset}:{code}", write.file_generation_id);
    write.diagnostics.push(DiagnosticWrite {
        diagnostic_id: id("diagnostic", &key),
        source_id: Some(source_id.into()),
        file_generation_id: Some(write.file_generation_id.clone()),
        byte_offset: Some(offset as i64),
        session_key: None,
        code,
        severity: "warning".into(),
        metadata: DiagnosticMetadata {
            parser_version: Some(PARSER_VERSION.into()),
            ..Default::default()
        },
        dedup_key: key,
        observed_at_ms: at_ms,
    });
}
