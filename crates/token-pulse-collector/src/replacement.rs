//! Bounded, read-only replacement framing into an isolated candidate; never active consumption.
use crate::{id, validate_source_file};
use std::path::Path;
use token_pulse_core::{
    adapter::{AdaptedRecord, adapt},
    domain::*,
    numeric::EpochMs,
    reader::{self, FramedLine, ReadError, ReaderCheckpoint, ReaderLimits},
    sequence::UsageSignature,
};
use token_pulse_store::{
    Database, ErrorCode, StoreResult,
    batch::{DiagnosticMetadata, DiagnosticWrite, ObservationWrite},
    collection::FileCheckpoint,
    file_candidate::{BeginFileCandidate, FileCandidateBatch},
};

pub struct ReplacementReceipt {
    pub generation_id: String,
    pub checkpoint_revision: i64,
    pub read_complete: bool,
    pub has_more: bool,
}
fn checkpoint(file: &FileCheckpoint) -> ReaderCheckpoint {
    ReaderCheckpoint {
        file_identity: file.file_identity.clone(),
        observed_size: Some(file.observed_size as u64),
        committed_offset: file.committed_offset as u64,
        anchors: file.anchors.clone(),
        oversized_line: file.context.oversized_line.clone(),
        modified_at: None,
    }
}
fn position(record: &NormalizedObservation) -> &PhysicalPosition {
    match record {
        NormalizedObservation::SessionMetadata {
            physical_position, ..
        }
        | NormalizedObservation::TurnMetadata {
            physical_position, ..
        }
        | NormalizedObservation::Context {
            physical_position, ..
        } => physical_position,
        NormalizedObservation::Usage(u) => &u.physical_position,
    }
}
fn diagnostic(write: &mut FileCandidateBatch, source: &str, offset: u64, code: ErrorCode, at: i64) {
    let key = format!("{}:{offset}:{code}", write.generation_id);
    write.diagnostics.push(DiagnosticWrite {
        diagnostic_id: id("diagnostic", &key),
        source_id: Some(source.into()),
        file_generation_id: Some(write.generation_id.clone()),
        byte_offset: Some(offset as i64),
        session_key: None,
        code,
        severity: "warning".into(),
        metadata: DiagnosticMetadata {
            parser_version: Some(PARSER_VERSION.into()),
            ..Default::default()
        },
        dedup_key: key,
        observed_at_ms: at,
    });
}
/// One batch. Ready only describes the isolated read; publication still requires a verified ledger.
pub fn read_replacement_file(
    db: &Database,
    source: &str,
    path: &Path,
    at: i64,
) -> StoreResult<ReplacementReceipt> {
    EpochMs::new(at)?;
    let root = db
        .enabled_source_root(source)?
        .ok_or(ErrorCode::PermissionDenied)?;
    validate_source_file(Path::new(&root), path)?;
    let old = db
        .file_checkpoint(source, path.to_str().ok_or(ErrorCode::InvalidQuery)?, None)?
        .ok_or(ErrorCode::InvalidQuery)?;
    let candidate = if let Some(c) = db.active_file_read_candidate(&old.file_id)? {
        if let Err(error) = db.validate_file_read_candidate(&c.checkpoint.file_generation_id) {
            if matches!(c.state.as_str(), "reading" | "ready") {
                db.fail_file_read_candidate(c.checkpoint.file_generation_id, error.code, at)?;
            }
            return Err(error);
        }
        c
    } else {
        // A Writer CAS conflict alone does not establish a new physical generation.
        match reader::read_batch(
            path,
            &old.file_generation_id,
            &checkpoint(&old),
            &ReaderLimits {
                records: 1,
                ..Default::default()
            },
        ) {
            Err(ReadError::InvalidGeneration) => {}
            Err(error) => return Err(error.code().into()),
            Ok(_) => return Err(ErrorCode::InvalidQuery.into()),
        }
        let probe = reader::read_batch(
            path,
            "replacement-probe",
            &ReaderCheckpoint::default(),
            &ReaderLimits {
                records: 1,
                ..Default::default()
            },
        )
        .map_err(|e| e.code())?;
        let mut nonce = [0u8; 32];
        getrandom::fill(&mut nonce).map_err(|_| ErrorCode::DbWriteFailed)?;
        let generation = id("replacement", &format!("{:x?}", nonce));
        db.begin_file_read_candidate(BeginFileCandidate {
            generation_id: generation,
            file_id: old.file_id,
            expected_generation_id: old.file_generation_id,
            expected_checkpoint_revision: old.checkpoint_revision,
            identity: probe.file_identity,
            observed_size: probe.observed_size as i64,
            at_ms: at,
        })?
    };
    if candidate.state == "claimed" {
        return Err(ErrorCode::RevisionConflict.into());
    }
    let saved = candidate.checkpoint;
    let cp = checkpoint(&saved);
    let mut batch = match reader::read_batch(
        path,
        &saved.file_generation_id,
        &cp,
        &ReaderLimits::default(),
    ) {
        Ok(batch) => batch,
        Err(error) => {
            if error == ReadError::InvalidGeneration {
                db.fail_file_read_candidate(saved.file_generation_id, error.code(), at)?;
            }
            return Err(error.code().into());
        }
    };
    if candidate.state == "ready" {
        if batch.observed_size == saved.observed_size as u64 {
            return Ok(ReplacementReceipt {
                generation_id: saved.file_generation_id,
                checkpoint_revision: saved.checkpoint_revision,
                read_complete: true,
                has_more: false,
            });
        }
        db.reopen_file_read_candidate(
            saved.file_generation_id.clone(),
            saved.checkpoint_revision,
            at,
        )?;
    }
    // Metadata can make each necessary observation much larger than its source usage line.
    let mut preview = saved.context.clone();
    let mut largest = serde_json::to_vec(&preview.metadata)?.len();
    for line in &batch.lines {
        if let FramedLine::Complete { position, bytes } = line {
            let _ = adapt(bytes, position.clone(), &mut preview);
            largest = largest.max(serde_json::to_vec(&preview.metadata)?.len());
        }
    }
    let available = (16usize * 1024 * 1024).saturating_sub(largest + 4096);
    let records = (available / (largest + 4096)).clamp(1, 500);
    if batch.lines.len() > records {
        batch = match reader::read_batch(
            path,
            &saved.file_generation_id,
            &cp,
            &ReaderLimits {
                records,
                ..Default::default()
            },
        ) {
            Ok(batch) => batch,
            Err(error) => {
                if error == ReadError::InvalidGeneration {
                    db.fail_file_read_candidate(saved.file_generation_id, error.code(), at)?;
                }
                return Err(error.code().into());
            }
        };
    }
    let mut context = saved.context;
    let has_more = batch.has_more;
    let mut write = FileCandidateBatch {
        generation_id: saved.file_generation_id.clone(),
        expected_offset: saved.committed_offset,
        expected_checkpoint_revision: saved.checkpoint_revision,
        next_offset: batch.next_offset as i64,
        observed_size: batch.observed_size as i64,
        anchors: batch.anchors,
        context: ReaderContext::default(),
        observations: vec![],
        diagnostics: vec![],
        at_ms: at,
    };
    for line in batch.lines {
        let mut record = match line {
            FramedLine::Oversized { position } => {
                context.independent_head_available = false;
                diagnostic(
                    &mut write,
                    source,
                    position.byte_offset,
                    ErrorCode::UnsupportedFormat,
                    at,
                );
                continue;
            }
            FramedLine::Complete { position, bytes } => match adapt(&bytes, position, &mut context)
            {
                AdaptedRecord::Ignored => continue,
                AdaptedRecord::Diagnostic(d) => {
                    context.independent_head_available = false;
                    diagnostic(&mut write, source, d.position.byte_offset, d.code, at);
                    continue;
                }
                AdaptedRecord::Observation(o) => *o,
            },
        };
        if let NormalizedObservation::SessionMetadata {
            provider_session_id,
            ..
        } = &record
        {
            // Proposed identity only. Registering / alias resolution belongs to candidate planning.
            context.session_key = Some(id(
                "session",
                &format!("{}:{provider_session_id}", saved.file_id),
            ));
            context.requires_sequence_rebuild = true;
            context.independent_head_available = false;
        }
        match &mut record {
            NormalizedObservation::Usage(u) => {
                u.session_key = context
                    .session_key
                    .clone()
                    .ok_or(ErrorCode::CheckpointConflict)?
            }
            NormalizedObservation::TurnMetadata { session_key, .. }
            | NormalizedObservation::Context { session_key, .. } => {
                *session_key = context
                    .session_key
                    .clone()
                    .ok_or(ErrorCode::CheckpointConflict)?
            }
            _ => {}
        }
        let p = position(&record);
        let observation_id = id(
            "observation",
            &format!("{}:{}", saved.file_generation_id, p.byte_offset),
        );
        let fingerprint = match &record {
            NormalizedObservation::Usage(u) => UsageSignature::from(u).candidate_fingerprint(),
            _ => id("metadata", &serde_json::to_string(&record)?),
        };
        write.observations.push(ObservationWrite {
            observation_id,
            session_key: context.session_key.clone(),
            record,
            payload_fingerprint: fingerprint,
        });
    }
    context.oversized_line = batch.oversized_line;
    write.context = context;
    let complete =
        write.next_offset == write.observed_size && write.context.oversized_line.is_none();
    let revision = db.stage_file_candidate_batch(write)?;
    if complete {
        db.seal_file_read_candidate(saved.file_generation_id.clone(), revision, at)?;
    }
    Ok(ReplacementReceipt {
        generation_id: saved.file_generation_id,
        checkpoint_revision: revision,
        read_complete: complete,
        has_more,
    })
}
