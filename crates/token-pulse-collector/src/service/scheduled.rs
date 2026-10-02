//! A scheduled replacement owns its read/claim until publication; old inputs cannot advance.
use crate::{
    CollectionReceipt, collect_file, replacement::read_replacement_file, validate_source_file,
};
use std::{path::Path, time::Duration};
use token_pulse_store::{Database, ErrorCode, StoreResult};

pub(super) enum Read {
    Committed(CollectionReceipt),
    Pending {
        delay: Duration,
        error: Option<ErrorCode>,
    },
}
fn replacement(db: &Database, source: &str, path: &Path, at: i64) -> StoreResult<Read> {
    let receipt = read_replacement_file(db, source, path, at)?;
    if receipt.read_complete {
        db.enqueue_file_candidate_rebuild(receipt.generation_id, receipt.checkpoint_revision, at)?;
        return Ok(Read::Pending {
            delay: Duration::from_millis(100),
            error: None,
        });
    }
    Ok(Read::Pending {
        delay: if receipt.has_more {
            Duration::ZERO
        } else {
            Duration::from_secs(1)
        },
        error: if receipt.has_more {
            None
        } else {
            Some(ErrorCode::CheckpointConflict)
        },
    })
}
pub(super) fn collect(db: &Database, source: &str, path: &Path, at: i64) -> StoreResult<Read> {
    let root = db
        .enabled_source_root(source)?
        .ok_or(ErrorCode::PermissionDenied)?;
    validate_source_file(Path::new(&root), path)?;
    if let Some(current) =
        db.file_checkpoint(source, path.to_str().ok_or(ErrorCode::InvalidQuery)?, None)?
    {
        if db.file_has_frozen_rebuild(&current.file_id)? {
            return Ok(Read::Pending {
                delay: Duration::from_millis(100),
                error: None,
            });
        }
        if let Some(candidate) = db.active_file_read_candidate(&current.file_id)? {
            if candidate.state == "claimed" {
                // Publication/failure is owned by the job. Revisit promptly even without a hint.
                return Ok(Read::Pending {
                    delay: Duration::from_millis(100),
                    error: None,
                });
            }
            return replacement(db, source, path, at);
        }
    }
    match collect_file(db, source, path, at) {
        Ok(receipt) => Ok(Read::Committed(receipt)),
        Err(error) if error.code == ErrorCode::CheckpointConflict => {
            // The readonly probe must prove a physical generation change. A Writer CAS conflict
            // alone must not create a candidate or a second session identity.
            match replacement(db, source, path, at) {
                Err(probe) if probe.code == ErrorCode::InvalidQuery => Err(error),
                result => result,
            }
        }
        Err(error) => Err(error),
    }
}
