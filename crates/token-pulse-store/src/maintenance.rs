//! Writer-only WAL recovery. Never truncates or deletes database files manually.
use crate::{Database, ErrorCode, StoreResult};
use rusqlite::{Connection, ffi};
use std::{path::Path, time::Duration};

pub(crate) const WAL_LIMIT: u64 = 64 * 1024 * 1024;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckpointState {
    Complete,
    Deferred,
}
#[derive(Debug, Clone, Copy)]
pub struct CheckpointResult {
    pub state: CheckpointState,
    pub log_frames: Option<u32>,
    pub checkpointed_frames: Option<u32>,
}

pub(crate) fn checkpoint(conn: &Connection, truncate: bool) -> StoreResult<CheckpointResult> {
    if !conn.is_autocommit() {
        return Err(ErrorCode::InvalidQuery.into());
    }
    // A pinned reader must never make maintenance wait five seconds on Writer.
    conn.busy_timeout(Duration::ZERO)?;
    let mut log = -1;
    let mut checkpointed = -1;
    let mode = if truncate {
        ffi::SQLITE_CHECKPOINT_TRUNCATE
    } else {
        ffi::SQLITE_CHECKPOINT_PASSIVE
    };
    // SAFETY: the sole Writer owns this live connection and runs this between
    // accepted transactions. SQLite owns all WAL operations; pointers reference
    // stack outputs for the duration of the call and the constant main name.
    let code = unsafe {
        ffi::sqlite3_wal_checkpoint_v2(
            conn.handle(),
            c"main".as_ptr(),
            mode,
            &mut log,
            &mut checkpointed,
        )
    };
    conn.busy_timeout(Duration::from_secs(5))?;
    if !matches!(code, ffi::SQLITE_OK | ffi::SQLITE_BUSY | ffi::SQLITE_LOCKED) {
        return Err(rusqlite::Error::SqliteFailure(ffi::Error::new(code), None).into());
    }
    Ok(CheckpointResult {
        state: if code == ffi::SQLITE_OK && log == checkpointed {
            CheckpointState::Complete
        } else {
            CheckpointState::Deferred
        },
        log_frames: u32::try_from(log).ok(),
        checkpointed_frames: u32::try_from(checkpointed).ok(),
    })
}
pub(crate) fn recover_if_large(conn: &Connection, path: &Path, limit: u64) {
    let mut wal = path.as_os_str().to_owned();
    wal.push("-wal");
    if std::fs::metadata(Path::new(&wal)).is_ok_and(|m| m.len() > limit) {
        // Deferred is normal while a reader is pinned; retry on a later tick.
        // Errors preserve the database and its WAL; no fallback file deletion.
        let _ = checkpoint(conn, true);
    }
}
impl Database {
    pub fn checkpoint_wal(&self) -> StoreResult<CheckpointResult> {
        self.write(|conn| checkpoint(conn, true))
    }
}

#[cfg(test)]
mod tests;
